//! DEP-001: Deployment mode schema.
//!
//! CSV guardrail: "Define deployment modes."
//! Caution: "Do not hide deployment mode in env vars."
//! Acceptance: schema validation; mode-compatibility check.
//!
//! The deployment mode is a first-class typed enum stored in the
//! deployment policy record, never inferred from environment variables
//! or implicit configuration.

use serde::{Deserialize, Serialize};

/// The deployment mode governing how the system routes certification
/// and compilation work. Each mode is explicit and self-describing.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DeploymentMode {
    /// All certification and compilation runs locally. No remote
    /// endpoints are contacted.
    LocalOnly,
    /// Local execution is primary, but remote endpoints may be used
    /// as fallback or for parallelism.
    LocalPlusRemote,
    /// Remote certification endpoint is preferred; local is used only
    /// when the remote is unavailable.
    RemoteCertificationPreferred,
    /// Certification is disabled entirely. Artifacts proceed without
    /// formal certification. Appropriate for development/testing only.
    CertificationDisabled,
}
