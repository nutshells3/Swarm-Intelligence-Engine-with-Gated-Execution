//! REC-008: Policy to forbid unsafeguarded self-promotion.
//!
//! CSV guardrail (P0!): "Implement policy to forbid unsafeguarded
//!   self-promotion (P0! denial rule + explicit override workflow)."
//! proof_or_check_hooks: self-promotion denial
//! auto_approval_policy: never (absolute -- not even never_silent)
//!
//! Acceptance: self-generated artifacts must never promote the same
//! recursive loop without explicit override.  The denial rule is
//! absolute by default.  Override requires an explicit request with
//! justification and independent review.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── REC-008: Self-promotion denial ───────────────────────────────────────

/// The denial rule: self-promotion is denied by default.
/// CSV: "P0! denial rule" -- this is the highest priority safety rule.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SelfPromotionDenialRule {
    /// Unique rule identifier.
    pub rule_id: String,
    /// Whether the denial is active (must always be true unless
    /// explicitly overridden via OverrideRequest).
    pub denial_active: bool,
    /// Human-readable description of what is denied.
    pub description: String,
    /// Priority level (always P0 for this rule).
    pub priority: String,
    /// Policy justification.
    pub justification: String,
}

/// A detected self-promotion attempt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PromotionAttempt {
    /// Unique attempt identifier.
    pub attempt_id: String,
    /// The self-improvement objective that produced the artifact.
    pub source_objective_id: String,
    /// The artifact that attempted self-promotion.
    pub artifact_ref: String,
    /// What kind of promotion was attempted (e.g., "merge_to_main",
    /// "trigger_downstream_loop", "elevate_gate_level").
    pub promotion_kind: String,
    /// Description of the attempted promotion.
    pub description: String,
    /// When the attempt was detected.
    pub detected_at: DateTime<Utc>,
}

/// Result of evaluating a promotion attempt against the denial rule.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DenialResult {
    /// Promotion denied (default -- CSV P0! rule).
    Denied,
    /// Promotion allowed via explicit override.
    AllowedViaOverride,
}

/// Status of an override request.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum OverrideStatus {
    /// Override requested, pending independent review.
    Pending,
    /// Override approved by an independent reviewer.
    Approved,
    /// Override rejected.
    Rejected,
    /// Override expired without decision.
    Expired,
}

/// An explicit override request to permit a self-promotion that would
/// otherwise be denied.
///
/// CSV: "explicit override workflow" -- overrides require justification,
/// independent review, and produce a durable record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OverrideRequest {
    /// Unique override request identifier.
    pub override_id: String,
    /// The promotion attempt this override covers.
    pub attempt_id: String,
    /// Who requested the override.
    pub requested_by: String,
    /// Justification for why the override should be permitted.
    pub justification: String,
    /// Current status of the override.
    pub status: OverrideStatus,
    /// Who reviewed the override (must be independent of requester).
    pub reviewed_by: Option<String>,
    /// Review notes.
    pub review_notes: Option<String>,
    pub requested_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
}
