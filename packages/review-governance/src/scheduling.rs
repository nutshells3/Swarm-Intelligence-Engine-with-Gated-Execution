//! Review scheduling policy types (REV-004 through REV-006, REV-011, REV-013).
//!
//! Key design rule: reviews are scheduled explicitly, not ad hoc.
//! Auto-approval requires explicit threshold and always leaves a durable
//! artifact.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::artifacts::ReviewKind;

// ── REV-004: Review scheduling policy schema ────────────────────────────
//
// CSV guardrail: "Define review scheduling policy schema."
// Acceptance: scheduling is policy-driven, not ad hoc.

/// When a review is triggered.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ReviewTriggerKind {
    /// Triggered at a cycle phase transition.
    PhaseTransition,
    /// Triggered periodically (heartbeat).
    Periodic,
    /// Triggered when a specific event occurs.
    EventDriven,
    /// Triggered manually by a user or supervisor.
    Manual,
}

/// REV-004 -- Review scheduling policy.
///
/// Defines when and how reviews are scheduled for a given review kind.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewSchedulingPolicy {
    /// Unique policy identifier.
    pub policy_id: String,
    /// Which review kind this policy governs.
    pub review_kind: ReviewKind,
    /// What triggers this review.
    pub trigger_kind: ReviewTriggerKind,
    /// For periodic triggers: interval in seconds between reviews.
    pub periodic_interval_secs: Option<u32>,
    /// For phase-transition triggers: which phases trigger the review.
    pub trigger_phases: Vec<String>,
    /// For event-driven triggers: which event kinds trigger the review.
    pub trigger_events: Vec<String>,
    /// Maximum number of concurrent reviews of this kind.
    pub max_concurrent_reviews: i32,
    /// Whether to skip this review if the previous one is still in progress.
    pub skip_if_in_progress: bool,
    /// Whether this policy is currently active.
    pub active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

// ── REV-005: Heartbeat review trigger rules ─────────────────────────────
//
// CSV guardrail: "Define heartbeat review trigger rules."
// Acceptance: heartbeat triggers are explicit and bounded.

/// REV-005 -- Heartbeat review trigger.
///
/// Triggers a review after a certain number of cycles or elapsed time,
/// ensuring the system does not go too long without human oversight.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeartbeatReviewTrigger {
    /// Unique trigger identifier.
    pub trigger_id: String,
    /// Which review kind to trigger.
    pub review_kind: ReviewKind,
    /// Number of cycles between heartbeat reviews.
    pub cycle_interval: i32,
    /// Maximum elapsed wall-clock seconds between heartbeat reviews.
    pub max_elapsed_secs: u32,
    /// Number of completed tasks that triggers a review.
    pub task_count_threshold: Option<i32>,
    /// Whether to force a review even if nothing has changed.
    pub force_on_no_change: bool,
    /// When this trigger last fired.
    pub last_triggered_at: Option<DateTime<Utc>>,
}

// ── REV-006: Auto-approval threshold schema ─────────────────────────────
//
// CSV guardrail: "Define auto-approval threshold schema."
//   auto_approval_policy: various -- some items require human review.
//   "Do not silently auto-approve without leaving a durable review artifact."
// Acceptance: auto-approval is bounded and always leaves a record.

/// REV-006 -- Auto-approval threshold.
///
/// Defines conditions under which a review can be auto-approved. Even when
/// auto-approved, a durable review artifact is always created (CSV caution).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutoApprovalThreshold {
    /// Unique threshold identifier.
    pub threshold_id: String,
    /// Which review kind this threshold applies to.
    pub review_kind: ReviewKind,
    /// Whether auto-approval is enabled for this review kind.
    pub auto_approval_enabled: bool,
    /// Maximum number of changes that can be auto-approved.
    pub max_auto_approvable_changes: Option<i32>,
    /// Required minimum certification grade (from robustness_policy)
    /// for auto-approval to apply. None means grade is not checked.
    pub required_minimum_grade: Option<String>,
    /// Whether all acceptance criteria must be satisfied for auto-approval.
    pub require_all_criteria_satisfied: bool,
    /// Whether auto-approval is forbidden for this review kind
    /// (overrides all other settings). The CSV `auto_approval_policy: "never"`
    /// maps to this being true.
    pub forbidden: bool,
    /// Justification for the auto-approval policy.
    pub policy_justification: String,
}

// ── REV-011: Periodic review scheduler ──────────────────────────────────
//
// CSV guardrail: "Implement periodic review scheduler."
// Acceptance: scheduler is policy-driven and observable.

/// Snapshot of the periodic review scheduler's state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewSchedulerSnapshot {
    /// Number of reviews currently scheduled.
    pub scheduled_count: i32,
    /// Number of reviews currently in progress.
    pub in_progress_count: i32,
    /// Number of reviews completed since last snapshot.
    pub completed_since_last: i32,
    /// Number of overdue reviews (past their periodic interval).
    pub overdue_count: i32,
    /// When this snapshot was taken.
    pub snapshot_at: DateTime<Utc>,
}

// ── REV-012: Review result ingestion ────────────────────────────────────
//
// CSV guardrail: "Implement review result ingestion."
// Acceptance: results are ingested and projected, not silently dropped.

/// A review result ready for ingestion into local state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewResultIngestion {
    /// The review artifact whose result is being ingested.
    pub review_id: String,
    /// The outcome of the review.
    pub outcome: crate::artifacts::ReviewOutcome,
    /// Gate effect on the target entity.
    pub gate_effect: String,
    /// Lifecycle transition to apply to the target.
    pub lifecycle_transition: Option<String>,
    /// Whether the ingestion triggers downstream recertification.
    pub triggers_recertification: bool,
    /// When the result was ingested.
    pub ingested_at: DateTime<Utc>,
}

// ── REV-013: Auto-approval policy resolution ────────────────────────────
//
// CSV guardrail: "Implement auto-approval policy resolution."
// Acceptance: resolution is explicit and traceable.

/// Record of an auto-approval decision, including why it was permitted.
/// This exists to ensure auto-approvals always leave a durable artifact
/// (CSV caution).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutoApprovalDecision {
    /// The review that was auto-approved.
    pub review_id: String,
    /// The threshold that permitted auto-approval.
    pub threshold_id: String,
    /// Whether auto-approval was actually applied (false if conditions
    /// were not met).
    pub was_auto_approved: bool,
    /// Reason auto-approval was permitted or denied.
    pub decision_reason: String,
    /// When the decision was made.
    pub decided_at: DateTime<Utc>,
}
