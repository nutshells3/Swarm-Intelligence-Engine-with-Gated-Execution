//! Formal-claim CLI gateway -- real integration with the `formal-claim` CLI.
//!
//! This module provides async wrappers around the `formal-claim` command-line
//! tool, enabling the orchestration system to submit claims for certification,
//! run audits, retrieve profiles, and request promotions.
//!
//! The gateway is OFF by default and must be explicitly enabled. A
//! `CertificationFrequency` policy controls which candidates are automatically
//! submitted versus requiring explicit requests.

use serde::{Deserialize, Serialize};
use std::fmt;

use crate::gateway::{CertificationCandidate, CertificationEligibility};

/// Controls how aggressively the system submits candidates for certification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CertificationFrequency {
    /// Certify every eligible output.
    Always,
    /// Only certify when explicitly requested by the user or a policy.
    OnRequest,
    /// Only certify outputs that affect contracts or invariants.
    CriticalOnly,
    /// Certification is disabled entirely.
    Off,
}

impl fmt::Display for CertificationFrequency {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Always => write!(f, "always"),
            Self::OnRequest => write!(f, "on_request"),
            Self::CriticalOnly => write!(f, "critical_only"),
            Self::Off => write!(f, "off"),
        }
    }
}

/// Errors returned by the formal-claim CLI gateway.
#[derive(Debug)]
pub enum GatewayError {
    /// The `formal-claim` CLI binary was not found at the configured path.
    CliNotFound(String),
    /// The CLI returned a non-zero exit code.
    CliError(String),
    /// Failed to parse the CLI's JSON output.
    ParseError(String),
    /// The gateway is disabled (enabled = false or frequency = Off).
    Disabled,
    /// An I/O error occurred while spawning or communicating with the CLI.
    IoError(String),
}

impl fmt::Display for GatewayError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CliNotFound(path) => write!(f, "formal-claim CLI not found at: {}", path),
            Self::CliError(msg) => write!(f, "formal-claim CLI error: {}", msg),
            Self::ParseError(msg) => write!(f, "failed to parse CLI output: {}", msg),
            Self::Disabled => write!(f, "formal-claim gateway is disabled"),
            Self::IoError(msg) => write!(f, "I/O error: {}", msg),
        }
    }
}

impl std::error::Error for GatewayError {}

impl From<std::io::Error> for GatewayError {
    fn from(err: std::io::Error) -> Self {
        if err.kind() == std::io::ErrorKind::NotFound {
            GatewayError::CliNotFound(err.to_string())
        } else {
            GatewayError::IoError(err.to_string())
        }
    }
}

/// Result of structuring and submitting a claim through the CLI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClaimSubmissionResult {
    /// The claim identifier assigned by formal-claim.
    pub claim_id: String,
    /// The formal status of the claim (e.g., "structured", "analyzed").
    pub formal_status: String,
    /// The gate assigned to the claim (e.g., "gate_0", "gate_1").
    pub gate: String,
}

/// Result of running an audit on a claim.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditResult {
    /// The audit identifier.
    pub audit_id: String,
    /// Whether the audit passed.
    pub passed: bool,
    /// The assurance profile returned by the audit.
    pub profile: serde_json::Value,
    /// The promotion state after the audit.
    pub promotion_state: serde_json::Value,
}

/// Gateway to the `formal-claim` CLI tool.
///
/// All operations shell out to the CLI with `--format json` and parse the
/// structured output. The gateway is OFF by default and must be explicitly
/// enabled via the `enabled` flag and a non-`Off` frequency.
pub struct FormalClaimGateway {
    /// Path to the `formal-claim` CLI binary.
    pub cli_path: String,
    /// Workspace data directory passed as `--data-dir`.
    pub data_dir: String,
    /// Master ON/OFF toggle.
    pub enabled: bool,
    /// How aggressively to submit candidates.
    pub frequency: CertificationFrequency,
}

impl FormalClaimGateway {
    /// Create a new gateway with the given data directory.
    /// The gateway is OFF by default.
    pub fn new(data_dir: String) -> Self {
        Self {
            cli_path: "formal-claim".to_string(),
            data_dir,
            enabled: false,
            frequency: CertificationFrequency::Off,
        }
    }

    /// Check whether a candidate should be submitted based on the current
    /// frequency policy.
    pub fn should_submit(
        &self,
        candidate: &CertificationCandidate,
        explicitly_requested: bool,
    ) -> bool {
        if !self.enabled {
            return false;
        }
        match self.frequency {
            CertificationFrequency::Off => false,
            CertificationFrequency::Always => true,
            CertificationFrequency::OnRequest => explicitly_requested,
            CertificationFrequency::CriticalOnly => matches!(
                candidate.eligibility_reason,
                CertificationEligibility::ContractOrInvariant
                    | CertificationEligibility::PromotionRequested
            ),
        }
    }

    /// Run a CLI command and return stdout on success.
    async fn run_cli(&self, args: &[&str]) -> Result<String, GatewayError> {
        if !self.enabled {
            return Err(GatewayError::Disabled);
        }

        let mut cmd_args = vec!["--data-dir", &self.data_dir, "--format", "json"];
        cmd_args.extend_from_slice(args);

        let output = tokio::process::Command::new(&self.cli_path)
            .args(&cmd_args)
            .output()
            .await?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            return Err(GatewayError::CliError(format!(
                "exit code {}: {}",
                output.status.code().unwrap_or(-1),
                stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout).to_string();
        Ok(stdout)
    }

    /// Parse a JSON string into a `serde_json::Value`.
    fn parse_json(raw: &str) -> Result<serde_json::Value, GatewayError> {
        serde_json::from_str(raw)
            .map_err(|e| GatewayError::ParseError(format!("{}: input={}", e, raw)))
    }

    /// Extract a string field from a JSON value.
    fn extract_str(value: &serde_json::Value, field: &str) -> Result<String, GatewayError> {
        value
            .get(field)
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| {
                GatewayError::ParseError(format!("missing field '{}' in response", field))
            })
    }

    /// Create or get a formal-claim project for the given objective.
    ///
    /// Runs: `formal-claim --data-dir {dir} --format json project init --name {summary} --domain development`
    pub async fn ensure_project(
        &self,
        _objective_id: &str,
        summary: &str,
    ) -> Result<String, GatewayError> {
        let output = self
            .run_cli(&["project", "init", "--name", summary, "--domain", "development"])
            .await?;

        let parsed = Self::parse_json(&output)?;
        Self::extract_str(&parsed, "project_id")
    }

    /// Submit a claim for certification.
    ///
    /// Step 1: Structure the claim.
    /// Step 2: Analyze the claim (triggers audit if formalizable).
    pub async fn submit_claim(
        &self,
        project_id: &str,
        claim_text: &str,
    ) -> Result<ClaimSubmissionResult, GatewayError> {
        // Step 1: Structure the claim
        let structure_output = self
            .run_cli(&[
                "claim",
                "structure",
                "--project-id",
                project_id,
                "--text",
                claim_text,
            ])
            .await?;

        let structure_parsed = Self::parse_json(&structure_output)?;
        let claim_id = Self::extract_str(&structure_parsed, "claim_id")?;

        // Step 2: Analyze the claim
        let analyze_output = self
            .run_cli(&[
                "claim",
                "analyze",
                "--project-id",
                project_id,
                "--claim-id",
                &claim_id,
            ])
            .await?;

        let analyze_parsed = Self::parse_json(&analyze_output)?;
        let formal_status = Self::extract_str(&analyze_parsed, "formal_status")
            .unwrap_or_else(|_| "unknown".to_string());
        let gate =
            Self::extract_str(&analyze_parsed, "gate").unwrap_or_else(|_| "gate_0".to_string());

        Ok(ClaimSubmissionResult {
            claim_id,
            formal_status,
            gate,
        })
    }

    /// Run a full audit on a claim.
    ///
    /// Runs: `formal-claim --data-dir {dir} --format json audit run --project-id {id} --claim-id {claim_id}`
    pub async fn audit_claim(
        &self,
        project_id: &str,
        claim_id: &str,
    ) -> Result<AuditResult, GatewayError> {
        let output = self
            .run_cli(&[
                "audit",
                "run",
                "--project-id",
                project_id,
                "--claim-id",
                claim_id,
            ])
            .await?;

        let parsed = Self::parse_json(&output)?;
        let audit_id = Self::extract_str(&parsed, "audit_id")
            .unwrap_or_else(|_| "unknown".to_string());
        let passed = parsed
            .get("passed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let profile = parsed
            .get("profile")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let promotion_state = parsed
            .get("promotion_state")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        Ok(AuditResult {
            audit_id,
            passed,
            profile,
            promotion_state,
        })
    }

    /// Get the assurance profile for a claim.
    ///
    /// Runs: `formal-claim --data-dir {dir} --format json artifact show profile --project-id {id} --claim-id {claim_id}`
    pub async fn get_profile(
        &self,
        project_id: &str,
        claim_id: &str,
    ) -> Result<serde_json::Value, GatewayError> {
        let output = self
            .run_cli(&[
                "artifact",
                "show",
                "profile",
                "--project-id",
                project_id,
                "--claim-id",
                claim_id,
            ])
            .await?;

        Self::parse_json(&output)
    }

    /// Request promotion to a target gate.
    ///
    /// Runs: `formal-claim --data-dir {dir} --format json promotion transition --project-id {id} --claim-id {claim_id} --target-gate {gate} --actor system --actor-role orchestrator`
    pub async fn request_promotion(
        &self,
        project_id: &str,
        claim_id: &str,
        target_gate: &str,
    ) -> Result<serde_json::Value, GatewayError> {
        let output = self
            .run_cli(&[
                "promotion",
                "transition",
                "--project-id",
                project_id,
                "--claim-id",
                claim_id,
                "--target-gate",
                target_gate,
                "--actor",
                "system",
                "--actor-role",
                "orchestrator",
            ])
            .await?;

        Self::parse_json(&output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::CertificationCandidate;
    use chrono::Utc;

    fn make_candidate(eligibility: CertificationEligibility) -> CertificationCandidate {
        CertificationCandidate {
            candidate_id: "c-1".to_string(),
            node_id: "n-1".to_string(),
            task_id: "t-1".to_string(),
            claim_summary: "test claim".to_string(),
            source_anchors: vec![],
            eligibility_reason: eligibility,
            provenance_task_attempt_id: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn disabled_gateway_never_submits() {
        let gw = FormalClaimGateway::new("/tmp/test".to_string());
        let candidate = make_candidate(CertificationEligibility::ContractOrInvariant);
        assert!(!gw.should_submit(&candidate, false));
        assert!(!gw.should_submit(&candidate, true));
    }

    #[test]
    fn always_frequency_submits_all() {
        let mut gw = FormalClaimGateway::new("/tmp/test".to_string());
        gw.enabled = true;
        gw.frequency = CertificationFrequency::Always;

        let candidate = make_candidate(CertificationEligibility::DownstreamDependency);
        assert!(gw.should_submit(&candidate, false));
    }

    #[test]
    fn on_request_requires_explicit() {
        let mut gw = FormalClaimGateway::new("/tmp/test".to_string());
        gw.enabled = true;
        gw.frequency = CertificationFrequency::OnRequest;

        let candidate = make_candidate(CertificationEligibility::ContractOrInvariant);
        assert!(!gw.should_submit(&candidate, false));
        assert!(gw.should_submit(&candidate, true));
    }

    #[test]
    fn critical_only_filters_by_eligibility() {
        let mut gw = FormalClaimGateway::new("/tmp/test".to_string());
        gw.enabled = true;
        gw.frequency = CertificationFrequency::CriticalOnly;

        let critical = make_candidate(CertificationEligibility::ContractOrInvariant);
        assert!(gw.should_submit(&critical, false));

        let promotion = make_candidate(CertificationEligibility::PromotionRequested);
        assert!(gw.should_submit(&promotion, false));

        let downstream = make_candidate(CertificationEligibility::DownstreamDependency);
        assert!(!gw.should_submit(&downstream, false));
    }

    #[test]
    fn off_frequency_never_submits() {
        let mut gw = FormalClaimGateway::new("/tmp/test".to_string());
        gw.enabled = true;
        gw.frequency = CertificationFrequency::Off;

        let candidate = make_candidate(CertificationEligibility::ContractOrInvariant);
        assert!(!gw.should_submit(&candidate, false));
        assert!(!gw.should_submit(&candidate, true));
    }
}
