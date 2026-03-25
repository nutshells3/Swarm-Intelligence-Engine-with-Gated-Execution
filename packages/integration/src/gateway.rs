//! Formal-claim gateway types.
//!
//! These types define the schemas for projecting correctness-critical outputs
//! through the formal-claim boundary and ingesting returned gates back into
//! local state.
//!
//! Key design rule: the local system must never reinterpret certification
//! informally. Gate effects and downstream admissibility must come from
//! explicit projection law (see `robustness_policy::DownstreamAdmissibilityPolicy`).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Why a particular node/task is eligible for certification.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CertificationEligibility {
    /// The output is required by a downstream dependency chain.
    DownstreamDependency,
    /// The output declares or enforces a contract or invariant.
    ContractOrInvariant,
    /// A user or policy explicitly requested promotion through certification.
    PromotionRequested,
    /// The output is involved in a conflict that requires adjudication.
    ConflictAdjudication,
}

/// Certification candidate.
///
/// Represents a node/task output that has been identified as eligible for
/// formal-claim certification. The `source_anchors` field provides the
/// narrow context references the gateway needs (no whole-project context
/// dump -- CSV context_selection_policy).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CertificationCandidate {
    /// Unique candidate identifier (UUIDv7 recommended).
    pub candidate_id: String,
    /// The execution node whose output is being certified.
    pub node_id: String,
    /// The specific task that produced the certifiable output.
    pub task_id: String,
    /// One-sentence summary of the claim to be certified.
    pub claim_summary: String,
    /// Narrow source-anchor references (file paths, symbol names, theorem
    /// names) that the gateway needs for evaluation. Must be kept bounded
    /// per CSV context_selection_policy.
    pub source_anchors: Vec<String>,
    /// Why this candidate is eligible for certification.
    pub eligibility_reason: CertificationEligibility,
    /// Optional provenance chain linking back to the task attempt that
    /// produced this output.
    pub provenance_task_attempt_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Queue status of a certification submission.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SubmissionQueueStatus {
    /// Submission enqueued, waiting to be sent to the gateway.
    Pending,
    /// Submission has been sent to the external gateway.
    Submitted,
    /// Gateway acknowledged receipt.
    Acknowledged,
    /// Gateway returned a result (success or failure).
    Completed,
    /// Submission failed at the transport level (retryable).
    TransportError,
    /// Submission was invalidated before completion (stale).
    Invalidated,
}

/// Certification submission record.
///
/// Tracks the lifecycle of a single submission to the formal-claim gateway.
/// The `idempotency_key` prevents duplicate submissions per CSV
/// idempotency_rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CertificationSubmission {
    /// Unique submission identifier (UUIDv7 recommended).
    pub submission_id: String,
    /// The candidate being submitted.
    pub candidate_id: String,
    /// Idempotency key: same key must not create duplicate submissions.
    pub idempotency_key: String,
    /// When the submission was created.
    pub submitted_at: DateTime<Utc>,
    /// Current queue status.
    pub queue_status: SubmissionQueueStatus,
    /// Number of transport-level retries attempted.
    pub retry_count: i32,
    /// Maximum transport-level retries allowed (bounded per CSV).
    pub max_retries: i32,
    /// When the queue status last changed.
    pub status_changed_at: DateTime<Utc>,
}

/// The effect a gateway result has on local state.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum GateEffect {
    /// The certification passed; local node may advance.
    Admit,
    /// The certification failed; local node is blocked.
    Block,
    /// The certification was inconclusive; local node stays in current state.
    Hold,
    /// The certification returned a partial result; some claims admitted.
    PartialAdmit,
}

impl Default for GateEffect {
    fn default() -> Self {
        GateEffect::Hold
    }
}

/// A lane transition triggered by a certification result.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum LaneTransition {
    /// Branch node promoted to mainline candidate.
    BranchToMainlineCandidate,
    /// Mainline candidate promoted to mainline.
    MainlineCandidateToMainline,
    /// Node moved to blocked lane.
    ToBlocked,
    /// No lane change; node stays where it is.
    NoChange,
}

/// Certification result projection.
///
/// Maps an external gateway result into the local state model. The
/// `external_gate` field carries the raw gate identifier returned by the
/// formal-claim stack; the `local_gate_effect` and `lane_transition`
/// fields encode what that gate means for local orchestration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CertificationResultProjection {
    /// The submission that produced this result.
    pub submission_id: String,
    /// Raw gate identifier from the external formal-claim stack.
    pub external_gate: String,
    /// The local effect of this gate on node lifecycle.
    pub local_gate_effect: GateEffect,
    /// Optional lane transition triggered by this result.
    pub lane_transition: Option<LaneTransition>,
    /// The certification grade projected from the external result.
    /// References `robustness_policy::CertificationGrade`.
    pub projected_grade: String,
    /// When this projection was computed.
    pub projected_at: DateTime<Utc>,
}
