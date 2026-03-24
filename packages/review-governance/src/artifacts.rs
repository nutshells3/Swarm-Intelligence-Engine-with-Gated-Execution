//! Review artifact types (REV-001 through REV-003).
//!
//! Key design rule: do not silently auto-approve without leaving a durable
//! review artifact. Every review decision must be recorded as a first-class
//! artifact that humans can inspect without replaying full context.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── REV-001: Review artifact schema ─────────────────────────────────────
//
// CSV guardrail: "Define review artifact schema."
//   "Do not silently auto-approve without leaving a durable review artifact."
// Acceptance: every review produces a persistent, inspectable record.

// ── REV-002: Review kinds enum ──────────────────────────────────────────
//
// CSV guardrail: "Define review kinds enum."
// Acceptance: kinds are a typed enum, not a string.

/// The kind of review being performed.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ReviewKind {
    /// Review of the planning phase (objective, milestones, dependencies).
    Planning,
    /// Review of the architecture draft.
    Architecture,
    /// Review of the development direction and strategic choices.
    Direction,
    /// Review of a specific milestone's deliverables.
    Milestone,
    /// Review of implementation artifacts (code, config, etc.).
    Implementation,
}

// ── REV-003: Review status lifecycle ────────────────────────────────────
//
// CSV guardrail: "Define review status lifecycle."
// Acceptance: lifecycle is a typed enum with explicit transitions.

/// Lifecycle status of a review artifact.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    /// Review has been scheduled but not yet started.
    Scheduled,
    /// Review is in progress (assigned to a reviewer).
    InProgress,
    /// Reviewer has submitted findings; awaiting integration.
    Submitted,
    /// Review findings have been integrated into the plan/state.
    Integrated,
    /// Review was approved (with or without conditions).
    Approved,
    /// Review was rejected; changes required.
    ChangesRequested,
    /// Review was superseded by a newer review.
    Superseded,
    /// Review was cancelled (e.g., the target was abandoned).
    Cancelled,
}

/// Outcome of a review. Separates the binary approve/reject decision from
/// the detailed findings stored in the artifact.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ReviewOutcome {
    /// Approved unconditionally.
    Approved,
    /// Approved with conditions that must be met before proceeding.
    ApprovedWithConditions,
    /// Rejected; changes required before re-review.
    Rejected,
    /// Inconclusive; more information needed.
    Inconclusive,
}

/// REV-001 -- Review artifact record.
///
/// A durable, inspectable record of a review. Every review decision --
/// including auto-approvals -- must produce one of these records.
/// The `findings_summary` field provides a human-readable digest so
/// reviewers do not need to replay full context (CSV goal).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewArtifactRecord {
    /// Unique review artifact identifier (UUIDv7 recommended).
    pub review_id: String,
    /// The kind of review.
    pub review_kind: ReviewKind,
    /// What is being reviewed (plan_id, draft_id, milestone_id, node_id, etc.).
    pub target_ref: String,
    /// The kind of target entity.
    pub target_kind: String,
    /// Current lifecycle status.
    pub status: ReviewStatus,
    /// Review outcome (populated when status is Submitted or later).
    pub outcome: Option<ReviewOutcome>,
    /// Human-readable findings summary (the "digest" that avoids full
    /// context replay).
    pub findings_summary: String,
    /// Detailed findings as structured JSON.
    pub detailed_findings: serde_json::Value,
    /// Conditions attached to an ApprovedWithConditions outcome.
    pub conditions: Vec<String>,
    /// Who performed the review (worker ID, "human", or "auto").
    pub reviewer: String,
    /// Whether this was an auto-approval.
    pub is_auto_approval: bool,
    /// The approval effect on downstream state (gate change, lifecycle
    /// transition, etc.).
    pub approval_effect: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
