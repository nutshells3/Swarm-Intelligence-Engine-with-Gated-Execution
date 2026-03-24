//! Integration package -- Formal-Claim Gateway (FCG-001 to FCG-015).
//!
//! This crate provides the types and schemas for projecting
//! correctness-critical outputs through the formal-claim boundary and
//! integrating returned gates back into local orchestration state.
//!
//! Key design rules:
//! - The local system must never reinterpret certification informally.
//! - Gate effects and downstream admissibility come from explicit projection
//!   law (see `robustness_policy::DownstreamAdmissibilityPolicy`).
//! - Same submission idempotency key must not create duplicates.
//! - Malformed external payloads must fail explicitly or be normalized
//!   under bounded rules.
//! - Stale certifications must be explicitly invalidated, never silently kept.
//!
//! Items: FCG-001 through FCG-015.

pub mod certification;
pub mod cli_gateway;
pub mod gateway;
pub mod http_gateway;
pub mod result_projection;
pub mod stale;

// Re-export primary types for ergonomic imports.
pub use certification::{
    BranchMainlineImpactRule, CandidateSelectionRule, CanonicalRefProjection,
    CertificationProvenance, CertificationQueueSnapshot, ClaimKind, ClaimLocalRefLink,
    NormalizedClaim, ResultPollingConfig, RevalidationTrigger, SourceAnchor, StaleReason,
};
pub use gateway::{
    CertificationCandidate, CertificationEligibility, CertificationResultProjection,
    CertificationSubmission, GateEffect, LaneTransition, SubmissionQueueStatus,
};
pub use stale::{NodeStalenessStatus, StaleInvalidationRecord};
pub use cli_gateway::{
    AuditResult, CertificationFrequency, ClaimSubmissionResult, FormalClaimGateway, GatewayError,
};
pub use http_gateway::{
    AssuranceProfile, AuditResult as HttpAuditResult, CertificationApiResult,
    DualFormalizationResult, GatewayMode, HttpFormalClaimGateway, VerificationApiResult,
    VerificationDetail,
};

// ── FCG-015: UI projection for certification state ──────────────────────
//
// CSV guardrail: "UI projection for certification state."
//   projection_effect: "certification queue; branch/mainline panel;
//   task readiness; review queue."
// Acceptance: projection types are explicit for each UI surface.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// A single entry in the certification queue projection, suitable for
/// rendering in the UI certification queue panel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CertificationQueueEntry {
    /// Submission identifier.
    pub submission_id: String,
    /// Candidate identifier.
    pub candidate_id: String,
    /// Node identifier.
    pub node_id: String,
    /// Claim summary for display.
    pub claim_summary: String,
    /// Current queue status.
    pub queue_status: SubmissionQueueStatus,
    /// When the submission was created.
    pub submitted_at: DateTime<Utc>,
    /// Elapsed time description (e.g., "2h 15m").
    pub elapsed_display: String,
}

/// Certification state projected onto the branch/mainline panel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BranchMainlineCertProjection {
    /// Node identifier.
    pub node_id: String,
    /// Current lane.
    pub current_lane: String,
    /// Current lifecycle state.
    pub current_lifecycle: String,
    /// Number of active certifications.
    pub active_certifications: i32,
    /// Number of stale certifications.
    pub stale_certifications: i32,
    /// Number of pending submissions.
    pub pending_submissions: i32,
    /// Whether all required certifications are satisfied.
    pub certification_satisfied: bool,
}

/// Certification impact on task readiness, projected for the task
/// readiness panel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskReadinessCertProjection {
    /// Task identifier.
    pub task_id: String,
    /// Node identifier.
    pub node_id: String,
    /// Whether certification is required for this task to proceed.
    pub certification_required: bool,
    /// Whether certification is currently satisfied.
    pub certification_satisfied: bool,
    /// Blocking reason if certification is required but not satisfied.
    pub blocking_reason: Option<String>,
}

/// Certification entries projected into the review queue for human
/// review of certification results.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewQueueCertProjection {
    /// Submission identifier.
    pub submission_id: String,
    /// The projected grade from the external result.
    pub projected_grade: String,
    /// The gate effect.
    pub gate_effect: GateEffect,
    /// Whether human review is required for this result.
    pub requires_human_review: bool,
    /// Human-readable summary for the reviewer.
    pub review_summary: String,
}
