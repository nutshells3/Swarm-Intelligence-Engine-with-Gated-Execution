//! HTTP gateway to the formal-claim certification API.
//!
//! Calls the JSON HTTP endpoints exposed by the formal-claim engine
//! (`POST /api/certify`, `POST /api/verify`, `GET /api/config`,
//! `GET /api/health`) instead of shelling out to the CLI.
//!
//! Endpoint resolved from env vars (`FORMAL_CLAIM_ENDPOINT` or
//! `VERIFY_INTEGRATION_HTTP_API_PORT`, default 8321).

use std::collections::HashMap;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::cli_gateway::GatewayError;
use crate::gateway::GateEffect;

// ── API result types ────────────────────────────────────────────────────

/// Mirrors the Python `CertificationResult` returned by `POST /api/certify`.
///
/// Carries the full OAE response: verdict, assurance profile, dual
/// formalization divergence data, audit results, and per-formalizer
/// verification details. The old "passed"/"failed" `outcome` field is
/// accepted via `verdict_compat` for backward compatibility.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CertificationApiResult {
    /// `"certified"` | `"refuted"` | `"inconclusive"` | `"error"`.
    /// Also accepts legacy `"passed"` / `"failed"` via `verdict_compat`.
    #[serde(default)]
    pub verdict: String,
    /// Claim identifier assigned by the engine.
    #[serde(default)]
    pub claim_id: String,
    /// Project identifier assigned by the engine.
    #[serde(default)]
    pub project_id: String,
    /// Raw gate string from the assurance profile.
    #[serde(default)]
    pub gate: String,
    /// Full assurance profile from OAE.
    #[serde(default)]
    pub assurance_profile: AssuranceProfile,
    /// Dual formalization comparison result.
    #[serde(default)]
    pub dual_formalization: DualFormalizationResult,
    /// Audit pipeline output.
    #[serde(default)]
    pub audit: AuditResult,
    /// Verification detail for formalizer A.
    #[serde(default)]
    pub verification_a: Option<VerificationDetail>,
    /// Verification detail for formalizer B.
    #[serde(default)]
    pub verification_b: Option<VerificationDetail>,
    /// Errors collected during the pipeline run.
    #[serde(default)]
    pub errors: Vec<String>,

    // ── backward-compat fields (old "passed"/"failed" shape) ────────
    /// Legacy `outcome` field. When present and `verdict` is empty,
    /// `to_gate_effect()` falls back to this.
    #[serde(default, alias = "outcome")]
    verdict_compat: String,
    /// Legacy blocking-issues list (old shape).
    #[serde(default)]
    blocking_issues_compat: Vec<String>,
}

/// Assurance profile returned by OAE.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AssuranceProfile {
    #[serde(default)]
    pub gate: String,
    #[serde(default)]
    pub formal_status: String,
    #[serde(default)]
    pub blocking_issues: Vec<String>,
    #[serde(default)]
    pub warnings: Vec<String>,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Dual formalization comparison result from OAE.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DualFormalizationResult {
    #[serde(default)]
    pub divergence_detected: bool,
    #[serde(default)]
    pub formalizer_a_status: String,
    #[serde(default)]
    pub formalizer_b_status: String,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Audit pipeline result from OAE.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AuditResult {
    #[serde(default)]
    pub trust_surface: serde_json::Value,
    #[serde(default)]
    pub probe_results: Vec<serde_json::Value>,
    #[serde(default)]
    pub verdict: String,
    #[serde(flatten)]
    pub extra: HashMap<String, serde_json::Value>,
}

/// Per-formalizer verification detail from OAE.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct VerificationDetail {
    #[serde(default)]
    pub success: bool,
    #[serde(default)]
    pub sorry_count: u32,
    #[serde(default)]
    pub oops_count: u32,
    #[serde(default)]
    pub diagnostics: Vec<String>,
    #[serde(default)]
    pub backend_id: String,
    #[serde(default)]
    pub duration_seconds: f64,
}

impl CertificationApiResult {
    /// Effective verdict, falling back to legacy `outcome` when `verdict`
    /// is empty.
    fn effective_verdict(&self) -> &str {
        if !self.verdict.is_empty() {
            &self.verdict
        } else {
            match self.verdict_compat.as_str() {
                "passed" => "certified",
                "failed" => "refuted",
                other => other,
            }
        }
    }

    /// Map the full OAE result into the local `GateEffect` lattice.
    pub fn to_gate_effect(&self) -> GateEffect {
        match self.effective_verdict() {
            "certified" => {
                if self.has_sorry() {
                    GateEffect::PartialAdmit
                } else {
                    GateEffect::Admit
                }
            }
            "refuted" => GateEffect::Block,
            "inconclusive" => {
                if self.assurance_profile.blocking_issues.is_empty() {
                    GateEffect::Hold
                } else {
                    GateEffect::Block
                }
            }
            "error" => GateEffect::Hold,
            _ => GateEffect::Hold,
        }
    }

    /// Whether any formalizer reported `sorry` holes.
    pub fn has_sorry(&self) -> bool {
        self.verification_a.as_ref().map_or(false, |v| v.sorry_count > 0)
            || self.verification_b.as_ref().map_or(false, |v| v.sorry_count > 0)
    }

    /// Whether the dual formalization produced divergent results.
    pub fn has_divergence(&self) -> bool {
        self.dual_formalization.divergence_detected
    }

    /// Sum of sorry counts across both formalizers.
    pub fn total_sorry_count(&self) -> u32 {
        self.verification_a.as_ref().map_or(0, |v| v.sorry_count)
            + self.verification_b.as_ref().map_or(0, |v| v.sorry_count)
    }
}

/// Mirrors the Python `VerificationResult` returned by `POST /api/verify`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VerificationApiResult {
    /// Whether the proof build succeeded.
    pub success: bool,
    /// Number of `sorry` holes detected.
    #[serde(default)]
    pub sorry_count: u32,
    /// Diagnostic output from the proof checker.
    #[serde(default)]
    pub diagnostics: Vec<String>,
    /// Duration in seconds.
    #[serde(default)]
    pub duration_seconds: f64,
}

// ── Config loading ──────────────────────────────────────────────────────

/// Retry policy parsed from `[retry.certification_transport]` in
/// `verification.toml`.
#[derive(Debug, Clone)]
struct RetryPolicy {
    max_attempts: u32,
    backoff: String,
    base_ms: u64,
    cap_ms: u64,
    jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            backoff: "exponential".into(),
            base_ms: 1000,
            cap_ms: 30_000,
            jitter: true,
        }
    }
}

/// Subset of `verification.toml` that the HTTP gateway needs.
#[derive(Debug, Clone)]
struct HttpGatewayConfig {
    base_url: String,
    retry: RetryPolicy,
}

/// Resolve gateway config from environment variables (no TOML dependency).
///
/// - `FORMAL_CLAIM_ENDPOINT`: full URL override (e.g. `http://10.0.0.5:8321`)
/// - `VERIFY_INTEGRATION_HTTP_API_PORT`: port override (default 8321)
fn load_gateway_config() -> HttpGatewayConfig {
    let base_url = if let Ok(endpoint) = std::env::var("FORMAL_CLAIM_ENDPOINT") {
        endpoint
    } else {
        let port: u16 = std::env::var("VERIFY_INTEGRATION_HTTP_API_PORT")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(8321);
        format!("http://127.0.0.1:{port}")
    };

    debug!("formal-claim endpoint: {base_url}");
    HttpGatewayConfig {
        base_url,
        retry: RetryPolicy::default(),
    }
}

// ── Gateway mode ────────────────────────────────────────────────────────

/// Selects the transport used by the orchestration layer to reach the
/// formal-claim engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GatewayMode {
    /// Call the HTTP API (preferred).
    Http,
    /// Shell out to the `formal-claim` CLI (fallback).
    Cli,
}

// ── HTTP gateway ────────────────────────────────────────────────────────

/// Gateway that calls the formal-claim HTTP API.
pub struct HttpFormalClaimGateway {
    client: reqwest::Client,
    config: HttpGatewayConfig,
    /// Master ON/OFF toggle (mirrors CLI gateway contract).
    pub enabled: bool,
}

impl HttpFormalClaimGateway {
    /// Create a new HTTP gateway, resolving endpoint from env vars.
    pub fn new() -> Self {
        let config = load_gateway_config();
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .expect("failed to build HTTP client"),
            config,
            enabled: false,
        }
    }

    /// Create with an explicit base URL (for tests).
    pub fn with_base_url(base_url: String) -> Self {
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(300))
                .build()
                .expect("failed to build HTTP client"),
            config: HttpGatewayConfig {
                base_url,
                retry: RetryPolicy::default(),
            },
            enabled: false,
        }
    }

    // ── Retry helper ────────────────────────────────────────────────

    fn delay_for_attempt(&self, attempt: u32) -> Duration {
        let policy = &self.config.retry;
        let base = Duration::from_millis(policy.base_ms);
        let cap = Duration::from_millis(policy.cap_ms);

        let delay = match policy.backoff.as_str() {
            "exponential" => {
                let raw = base.saturating_mul(1u32.checked_shl(attempt).unwrap_or(u32::MAX));
                raw.min(cap)
            }
            "linear" => {
                let raw = base.saturating_mul(attempt + 1);
                raw.min(cap)
            }
            _ => Duration::ZERO,
        };

        if policy.jitter && !delay.is_zero() {
            // Simple deterministic jitter: +/- 25% based on attempt number.
            let jitter_factor = 0.75 + 0.5 * ((attempt as f64 * 0.618).fract());
            Duration::from_millis((delay.as_millis() as f64 * jitter_factor) as u64)
        } else {
            delay
        }
    }

    async fn with_retry<F, Fut, T>(&self, mut action: F) -> Result<T, GatewayError>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = Result<T, GatewayError>>,
    {
        let max = self.config.retry.max_attempts.max(1);
        let mut last_err = GatewayError::Disabled;
        for attempt in 0..max {
            match action().await {
                Ok(val) => return Ok(val),
                Err(e) => {
                    last_err = e;
                    if attempt + 1 < max {
                        let delay = self.delay_for_attempt(attempt);
                        if !delay.is_zero() {
                            debug!("retry attempt {}/{max}, waiting {delay:?}", attempt + 1);
                            tokio::time::sleep(delay).await;
                        }
                    }
                }
            }
        }
        Err(last_err)
    }

    // ── HTTP helpers ────────────────────────────────────────────────

    fn url(&self, path: &str) -> String {
        format!("{}{path}", self.config.base_url)
    }

    // ── Public API ──────────────────────────────────────────────────

    /// Check whether the engine is reachable.
    pub async fn health(&self) -> Result<bool, GatewayError> {
        if !self.enabled {
            return Err(GatewayError::Disabled);
        }
        let resp = self
            .client
            .get(self.url("/api/health"))
            .send()
            .await
            .map_err(|e| GatewayError::IoError(e.to_string()))?;
        Ok(resp.status().is_success())
    }

    /// Run the full certification pipeline on a natural-language claim.
    pub async fn certify(
        &self,
        claim: &str,
        domain: &str,
        config_overrides: Option<serde_json::Value>,
    ) -> Result<CertificationApiResult, GatewayError> {
        if !self.enabled {
            return Err(GatewayError::Disabled);
        }

        let url = self.url("/api/certify");
        let mut body = serde_json::json!({
            "claim": claim,
            "domain": domain,
        });
        if let Some(overrides) = config_overrides {
            body["config_overrides"] = overrides;
        }

        self.with_retry(|| {
            let client = &self.client;
            let url = url.clone();
            let body = body.clone();
            async move {
                let resp = client
                    .post(&url)
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| GatewayError::IoError(e.to_string()))?;

                if !resp.status().is_success() {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();
                    return Err(GatewayError::CliError(format!(
                        "HTTP {status}: {text}"
                    )));
                }

                resp.json::<CertificationApiResult>()
                    .await
                    .map_err(|e| GatewayError::ParseError(e.to_string()))
            }
        })
        .await
    }

    /// Run proof-only verification on source code.
    pub async fn verify(
        &self,
        source: &str,
        backend: &str,
    ) -> Result<VerificationApiResult, GatewayError> {
        if !self.enabled {
            return Err(GatewayError::Disabled);
        }

        let url = self.url("/api/verify");
        let body = serde_json::json!({
            "source": source,
            "backend": backend,
        });

        self.with_retry(|| {
            let client = &self.client;
            let url = url.clone();
            let body = body.clone();
            async move {
                let resp = client
                    .post(&url)
                    .json(&body)
                    .send()
                    .await
                    .map_err(|e| GatewayError::IoError(e.to_string()))?;

                if !resp.status().is_success() {
                    let status = resp.status();
                    let text = resp.text().await.unwrap_or_default();
                    return Err(GatewayError::CliError(format!(
                        "HTTP {status}: {text}"
                    )));
                }

                resp.json::<VerificationApiResult>()
                    .await
                    .map_err(|e| GatewayError::ParseError(e.to_string()))
            }
        })
        .await
    }

    /// Fetch the current unified config from the engine.
    pub async fn get_config(&self) -> Result<serde_json::Value, GatewayError> {
        if !self.enabled {
            return Err(GatewayError::Disabled);
        }

        let resp = self
            .client
            .get(self.url("/api/config"))
            .send()
            .await
            .map_err(|e| GatewayError::IoError(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(GatewayError::CliError(format!("HTTP {status}: {text}")));
        }

        resp.json::<serde_json::Value>()
            .await
            .map_err(|e| GatewayError::ParseError(e.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a minimal `CertificationApiResult` with only verdict set.
    fn make_result(verdict: &str) -> CertificationApiResult {
        CertificationApiResult {
            verdict: verdict.into(),
            claim_id: String::new(),
            project_id: String::new(),
            gate: String::new(),
            assurance_profile: AssuranceProfile::default(),
            dual_formalization: DualFormalizationResult::default(),
            audit: AuditResult::default(),
            verification_a: None,
            verification_b: None,
            errors: vec![],
            verdict_compat: String::new(),
            blocking_issues_compat: vec![],
        }
    }

    #[test]
    fn certified_no_sorry_is_admit() {
        let r = make_result("certified");
        assert_eq!(r.to_gate_effect(), GateEffect::Admit);
    }

    #[test]
    fn certified_with_sorry_is_partial_admit() {
        let mut r = make_result("certified");
        r.verification_a = Some(VerificationDetail {
            sorry_count: 2,
            ..Default::default()
        });
        assert_eq!(r.to_gate_effect(), GateEffect::PartialAdmit);
    }

    #[test]
    fn refuted_is_block() {
        let r = make_result("refuted");
        assert_eq!(r.to_gate_effect(), GateEffect::Block);
    }

    #[test]
    fn inconclusive_no_blocking_issues_is_hold() {
        let r = make_result("inconclusive");
        assert_eq!(r.to_gate_effect(), GateEffect::Hold);
    }

    #[test]
    fn inconclusive_with_blocking_issues_is_block() {
        let mut r = make_result("inconclusive");
        r.assurance_profile.blocking_issues = vec!["issue".into()];
        assert_eq!(r.to_gate_effect(), GateEffect::Block);
    }

    #[test]
    fn error_is_hold() {
        let r = make_result("error");
        assert_eq!(r.to_gate_effect(), GateEffect::Hold);
    }

    #[test]
    fn divergence_detection() {
        let mut r = make_result("certified");
        assert!(!r.has_divergence());
        r.dual_formalization.divergence_detected = true;
        assert!(r.has_divergence());
    }

    #[test]
    fn total_sorry_count_aggregation() {
        let mut r = make_result("certified");
        assert_eq!(r.total_sorry_count(), 0);

        r.verification_a = Some(VerificationDetail {
            sorry_count: 3,
            ..Default::default()
        });
        r.verification_b = Some(VerificationDetail {
            sorry_count: 1,
            ..Default::default()
        });
        assert_eq!(r.total_sorry_count(), 4);
    }

    #[test]
    fn legacy_outcome_passed_maps_to_admit() {
        // Old-format payload: `outcome: "passed"`, no `verdict` field.
        let json = serde_json::json!({
            "outcome": "passed",
            "claim_id": "c1"
        });
        let r: CertificationApiResult = serde_json::from_value(json).unwrap();
        assert_eq!(r.to_gate_effect(), GateEffect::Admit);
    }

    #[test]
    fn legacy_outcome_failed_maps_to_block() {
        let json = serde_json::json!({
            "outcome": "failed",
            "claim_id": "c2"
        });
        let r: CertificationApiResult = serde_json::from_value(json).unwrap();
        assert_eq!(r.to_gate_effect(), GateEffect::Block);
    }

    #[test]
    fn default_retry_policy() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_attempts, 3);
        assert_eq!(policy.backoff, "exponential");
        assert!(policy.jitter);
    }

    #[test]
    fn delay_calculation() {
        let gw = HttpFormalClaimGateway::with_base_url("http://localhost:1234".into());

        // Exponential: 1000ms * 2^0 = 1000ms (before jitter)
        let d0 = gw.delay_for_attempt(0);
        assert!(d0.as_millis() >= 750 && d0.as_millis() <= 1250);

        // Exponential: 1000ms * 2^1 = 2000ms (before jitter)
        let d1 = gw.delay_for_attempt(1);
        assert!(d1.as_millis() >= 1500 && d1.as_millis() <= 2500);
    }
}
