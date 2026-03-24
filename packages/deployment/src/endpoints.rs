//! DEP-002, DEP-003: Remote endpoint schemas.
//!
//! CSV guardrail: "Define remote certification and Lean compile endpoint
//!   schemas."
//! Caution: "Do not let remote transport errors masquerade as local
//!   certification."
//! Acceptance: schema validation; routing simulation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Health status of a remote endpoint, polled or updated on each
/// interaction.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum EndpointHealth {
    /// Endpoint is reachable and responding within SLA.
    Healthy,
    /// Endpoint is reachable but responding slowly.
    Degraded,
    /// Endpoint is unreachable or returning errors.
    Unhealthy,
    /// Endpoint health has not been checked yet.
    Unknown,
}

// ── DEP-002: Remote certification endpoint ──────────────────────────────

/// Schema for a remote certification endpoint. The control plane uses
/// this to route certification requests when the deployment mode
/// permits remote certification.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteCertificationEndpoint {
    /// Unique identifier for this endpoint configuration.
    pub endpoint_id: String,
    /// Display name for operator dashboards.
    pub label: String,
    /// Base URL of the remote certification service.
    pub base_url: String,
    /// Authentication method (e.g. "bearer_token", "mtls").
    pub auth_method: String,
    /// Request timeout in milliseconds.
    pub timeout_ms: u64,
    /// Maximum concurrent requests to this endpoint.
    pub max_concurrent_requests: u32,
    /// Current health status.
    pub health: EndpointHealth,
    /// Last time health was checked.
    pub last_health_check: Option<DateTime<Utc>>,
}

// ── DEP-003: Lean compile endpoint ──────────────────────────────────────

/// Schema for a remote Lean compilation endpoint. Used when local Lean
/// toolchain is unavailable or when remote compilation is preferred for
/// performance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LeanCompileEndpoint {
    /// Unique identifier for this endpoint configuration.
    pub endpoint_id: String,
    /// Display name for operator dashboards.
    pub label: String,
    /// Base URL of the Lean compile service.
    pub base_url: String,
    /// Authentication method.
    pub auth_method: String,
    /// Request timeout in milliseconds.
    pub timeout_ms: u64,
    /// Lean toolchain version expected by this endpoint.
    pub lean_version: String,
    /// Current health status.
    pub health: EndpointHealth,
    /// Last time health was checked.
    pub last_health_check: Option<DateTime<Utc>>,
}
