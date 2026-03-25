//! Certification projection types.
//!
//! This module covers the operational machinery: candidate selection rules,
//! provenance extraction, source-anchor preservation, claim normalization,
//! queue management, result polling, canonical projection, branch/mainline
//! impact, revalidation triggers, and cross-linking.
//!
//! Key design rule: the local system must never create a second canonical
//! authority. All admissibility decisions must flow from the gate lattice
//! and downstream admissibility policy defined in `robustness_policy`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::gateway::{GateEffect, LaneTransition};

/// A rule that determines whether a completed task output is eligible for
/// certification. Rules are evaluated in priority order; the first match
/// wins.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CandidateSelectionRule {
    /// Unique rule identifier.
    pub rule_id: String,
    /// Human-readable description of when this rule fires.
    pub description: String,
    /// Which task statuses trigger evaluation of this rule.
    pub trigger_on_status: Vec<String>,
    /// Required node lifecycle state for this rule to apply.
    pub required_lifecycle: Option<String>,
    /// Required node lane for this rule to apply.
    pub required_lane: Option<String>,
    /// Whether downstream dependency analysis is required.
    pub require_downstream_check: bool,
    /// Priority (lower = higher priority). Used for rule ordering.
    pub priority: i32,
}

/// Provenance record linking a certification candidate back to the worker
/// invocation and adapter call that produced it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CertificationProvenance {
    /// The candidate this provenance belongs to.
    pub candidate_id: String,
    /// The task attempt that produced the output.
    pub task_attempt_id: String,
    /// The worker that executed the task.
    pub worker_id: String,
    /// The adapter invocation ID for full traceability.
    pub adapter_invocation_id: Option<String>,
    /// Hash of the output that was submitted for certification.
    pub output_hash: String,
    /// When the provenance was recorded.
    pub recorded_at: DateTime<Utc>,
}

/// A source anchor preserved for gateway reference. Anchors provide the
/// narrow context the gateway needs without a whole-project context dump.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SourceAnchor {
    /// Unique anchor identifier.
    pub anchor_id: String,
    /// The candidate this anchor belongs to.
    pub candidate_id: String,
    /// The kind of anchor (file_path, symbol, theorem, test_case, etc.).
    pub anchor_kind: String,
    /// The anchor value (e.g., a file path, a function name).
    pub anchor_value: String,
    /// Content hash at the time of anchor creation, for staleness detection.
    pub content_hash: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// The kind of claim being submitted for certification.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ClaimKind {
    /// A correctness property (e.g., type safety, invariant preservation).
    Correctness,
    /// A liveness property (e.g., termination, progress).
    Liveness,
    /// A safety property (e.g., no undefined behavior, no data races).
    Safety,
    /// A performance bound (e.g., O(n log n) complexity).
    PerformanceBound,
    /// A behavioral specification (e.g., API contract).
    BehavioralSpec,
}

/// A normalized claim ready for gateway submission. Normalization strips
/// prose wrappers and enforces the gateway's expected schema envelope.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NormalizedClaim {
    /// The candidate this claim belongs to.
    pub candidate_id: String,
    /// The kind of claim.
    pub claim_kind: ClaimKind,
    /// Normalized claim statement in the gateway's expected format.
    pub normalized_statement: String,
    /// Evidence artifacts supporting the claim (hashes or refs).
    pub evidence_refs: Vec<String>,
    /// Whether normalization altered the original claim text.
    pub was_normalized: bool,
    /// Description of normalization applied (empty if none).
    pub normalization_notes: String,
}

/// Snapshot of the certification queue state for observability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CertificationQueueSnapshot {
    /// Total submissions currently pending.
    pub pending_count: i32,
    /// Total submissions currently submitted (awaiting response).
    pub submitted_count: i32,
    /// Total submissions completed since last snapshot.
    pub completed_count: i32,
    /// Total submissions in transport-error state.
    pub error_count: i32,
    /// Total submissions invalidated (stale).
    pub invalidated_count: i32,
    /// When this snapshot was taken.
    pub snapshot_at: DateTime<Utc>,
}

/// Configuration for polling the external gateway for results.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResultPollingConfig {
    /// Interval in seconds between poll attempts.
    pub poll_interval_secs: u32,
    /// Maximum number of consecutive polls before declaring timeout.
    pub max_poll_attempts: u32,
    /// Whether to use exponential backoff on consecutive empty polls.
    pub backoff_on_empty: bool,
    /// Backoff multiplier (integer, applied to poll_interval_secs).
    pub backoff_multiplier: u32,
}

/// Record of a canonical reference update triggered by a certification result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CanonicalRefProjection {
    /// Unique projection record identifier.
    pub projection_id: String,
    /// The submission whose result triggered this projection.
    pub submission_id: String,
    /// The node whose canonical state was updated.
    pub node_id: String,
    /// The gate effect that drove this projection.
    pub gate_effect: GateEffect,
    /// The lane transition applied (if any).
    pub lane_transition: Option<LaneTransition>,
    /// The node lifecycle state before projection.
    pub previous_lifecycle: String,
    /// The node lifecycle state after projection.
    pub new_lifecycle: String,
    /// When the projection was applied.
    pub projected_at: DateTime<Utc>,
}

/// A rule defining how a certification gate effect impacts branch/mainline
/// state and roadmap progression.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BranchMainlineImpactRule {
    /// Unique rule identifier.
    pub rule_id: String,
    /// The gate effect this rule applies to.
    pub gate_effect: GateEffect,
    /// The projected grade range this rule applies to (min grade name).
    pub minimum_grade: String,
    /// The lane transition to apply when this rule fires.
    pub lane_transition: LaneTransition,
    /// Whether to trigger roadmap reprioritization.
    pub trigger_roadmap_reprioritization: bool,
    /// Human-readable description of the impact.
    pub description: String,
}

/// Reason a certification result became stale.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum StaleReason {
    /// The source artifact was modified after certification.
    SourceModified,
    /// An upstream dependency changed after certification.
    UpstreamDependencyChanged,
    /// The policy or gate lattice was updated.
    PolicyChanged,
    /// A conflict was detected that affects the certified output.
    ConflictDetected,
    /// The certification was explicitly revoked by a human.
    ManualRevocation,
}

/// Revalidation trigger after upstream changes.
///
/// When a stale certification is detected, this trigger requests
/// revalidation.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RevalidationTrigger {
    /// The submission that became stale.
    pub submission_id: String,
    /// Why the certification became stale.
    pub stale_reason: StaleReason,
    /// The upstream change that caused staleness.
    pub triggering_change_ref: String,
    /// Whether automatic resubmission is permitted.
    pub auto_resubmit: bool,
    /// When the staleness was detected.
    pub detected_at: DateTime<Utc>,
}

/// Cross-link between a certified claim and a local state reference
/// (node, task, plan, milestone).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ClaimLocalRefLink {
    /// Unique link identifier.
    pub link_id: String,
    /// The submission carrying the certified claim.
    pub submission_id: String,
    /// The kind of local entity being linked.
    pub local_ref_kind: String,
    /// The ID of the local entity.
    pub local_ref_id: String,
    /// Human-readable description of the linkage.
    pub linkage_description: String,
    pub created_at: DateTime<Utc>,
}
