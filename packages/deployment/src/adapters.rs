//! DEP-008, DEP-009: Remote certification and compile adapter types.
//!
//! CSV guardrail: "Define remote certification and compile adapter types."
//! Caution: "Do not let remote transport errors masquerade as local
//!   certification."
//! Acceptance: schema validation.
//!
//! Adapter types carry explicit error provenance so a remote transport
//! failure is never confused with a local certification outcome.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A remote transport error. Typed explicitly so it cannot be confused
/// with a certification or compilation outcome.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteTransportError {
    /// HTTP status code, if available.
    pub status_code: Option<u16>,
    /// Error message from the transport layer.
    pub message: String,
    /// Whether this error is considered retryable.
    pub retryable: bool,
    /// Timestamp of the error.
    pub occurred_at: DateTime<Utc>,
}

// ── DEP-008: Remote certification adapter ───────────────────────────────

/// Status of a remote certification request.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RemoteCertificationAdapterStatus {
    /// Request submitted and awaiting response.
    Pending,
    /// Certification completed successfully.
    Completed,
    /// Certification failed (a certification outcome, not a transport
    /// error).
    Failed,
    /// Transport error occurred -- explicitly distinct from a
    /// certification failure.
    TransportError,
    /// Request timed out at the transport level.
    TimedOut,
}

/// Configuration for the remote certification adapter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteCertificationAdapterConfig {
    /// The endpoint ID (references a RemoteCertificationEndpoint).
    pub endpoint_id: String,
    /// Maximum payload size in bytes.
    pub max_payload_bytes: u64,
    /// Whether to include source artifacts in the request.
    pub include_source_artifacts: bool,
    /// Retry count for transport errors (not certification failures).
    pub transport_retry_count: u32,
}

/// Result of a remote certification adapter call. The status
/// distinguishes certification outcomes from transport errors.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteCertificationAdapterResult {
    pub request_id: String,
    pub status: RemoteCertificationAdapterStatus,
    /// Certification grade, if the request completed (Completed or
    /// Failed certification outcome).
    pub certification_grade: Option<String>,
    /// Transport error details, if the failure was at the transport
    /// layer.
    pub transport_error: Option<RemoteTransportError>,
    /// Raw response body for diagnostics.
    pub response_body: Option<String>,
    pub completed_at: DateTime<Utc>,
}

/// Bridge to `integration::http_gateway::HttpFormalClaimGateway`.
///
/// This function connects the deployment layer's
/// `RemoteCertificationEndpoint` to the integration gateway that
/// already handles HTTP transport, retry, and result mapping.
///
/// The bridge constructs a gateway configured to the endpoint's
/// base_url and delegates the certification call. The returned
/// `RemoteCertificationAdapterResult` preserves the distinction
/// between certification outcomes and transport errors (per playbook
/// rule 7: no silent fallback; remote transport errors are never
/// confused with local certification outcomes).
pub async fn bridge_certify_via_gateway(
    endpoint: &crate::endpoints::RemoteCertificationEndpoint,
    claim: &str,
    domain: &str,
) -> RemoteCertificationAdapterResult {
    use chrono::Utc;

    let gateway =
        integration::http_gateway::HttpFormalClaimGateway::with_base_url(endpoint.base_url.clone());
    // Enable the gateway so that calls are not rejected as disabled.
    let mut gw = gateway;
    gw.enabled = true;

    let request_id = format!(
        "cert-{}-{}",
        endpoint.endpoint_id,
        Utc::now().timestamp_millis()
    );

    match gw.certify(claim, domain, None).await {
        Ok(result) => {
            let status = match result.to_gate_effect() {
                integration::gateway::GateEffect::Admit
                | integration::gateway::GateEffect::PartialAdmit => {
                    RemoteCertificationAdapterStatus::Completed
                }
                integration::gateway::GateEffect::Block => {
                    RemoteCertificationAdapterStatus::Failed
                }
                integration::gateway::GateEffect::Hold => {
                    RemoteCertificationAdapterStatus::Pending
                }
            };
            RemoteCertificationAdapterResult {
                request_id,
                status,
                certification_grade: Some(result.verdict.clone()),
                transport_error: None,
                response_body: serde_json::to_string(&result).ok(),
                completed_at: Utc::now(),
            }
        }
        Err(e) => RemoteCertificationAdapterResult {
            request_id,
            status: RemoteCertificationAdapterStatus::TransportError,
            certification_grade: None,
            transport_error: Some(RemoteTransportError {
                status_code: None,
                message: e.to_string(),
                retryable: true,
                occurred_at: Utc::now(),
            }),
            response_body: None,
            completed_at: Utc::now(),
        },
    }
}

// ── DEP-009: Remote compile adapter ─────────────────────────────────────

/// Status of a remote compile request.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CompileAdapterStatus {
    /// Request submitted and awaiting response.
    Pending,
    /// Compilation succeeded.
    Succeeded,
    /// Compilation failed (a compile outcome, not a transport error).
    CompileFailed,
    /// Transport error -- explicitly distinct from a compile failure.
    TransportError,
    /// Request timed out at the transport level.
    TimedOut,
}

/// Configuration for the remote compile adapter.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompileAdapterConfig {
    /// The endpoint ID (references a LeanCompileEndpoint).
    pub endpoint_id: String,
    /// Maximum source size in bytes.
    pub max_source_bytes: u64,
    /// Retry count for transport errors (not compile failures).
    pub transport_retry_count: u32,
}

/// Result of a remote compile adapter call.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompileAdapterResult {
    pub request_id: String,
    pub status: CompileAdapterStatus,
    /// Compiler output (stdout/stderr) when available.
    pub compiler_output: Option<String>,
    /// Transport error details when failure is at the transport layer.
    pub transport_error: Option<RemoteTransportError>,
    pub completed_at: DateTime<Utc>,
}

/// Stub for remote Lean compilation.
///
/// Actual Lean compilation is deferred to a future milestone. This
/// function validates the inputs and returns `CompileAdapterStatus::Pending`
/// to signal that the request was accepted but actual compilation has
/// not yet been implemented.
///
/// Callers must inspect the returned status and must not assume the
/// compilation succeeded. Returning `Pending` (instead of a fake
/// success) ensures no silent fallback (playbook rule 7).
pub async fn remote_compile(
    endpoint: &crate::endpoints::LeanCompileEndpoint,
    source: &str,
) -> Result<CompileAdapterResult, RemoteTransportError> {
    use chrono::Utc;

    if source.is_empty() {
        return Err(RemoteTransportError {
            status_code: None,
            message: "source must not be empty".to_string(),
            retryable: false,
            occurred_at: Utc::now(),
        });
    }

    let request_id = format!(
        "compile-{}-{}",
        endpoint.endpoint_id,
        Utc::now().timestamp_millis()
    );

    tracing::info!(
        endpoint_id = %endpoint.endpoint_id,
        lean_version = %endpoint.lean_version,
        source_len = source.len(),
        "remote_compile: stub accepted (actual Lean compilation deferred)"
    );

    Ok(CompileAdapterResult {
        request_id,
        status: CompileAdapterStatus::Pending,
        compiler_output: None,
        transport_error: None,
        completed_at: Utc::now(),
    })
}
