//! REC-009: Self-improvement report generation.
//!
//! CSV guardrail: "Implement self-improvement report generation (durable
//!   report: objective, repo target, scores, drift, approvals, blockers)."
//! auto_approval_policy: never_silent
//!
//! Acceptance: every self-improvement loop iteration produces a durable
//! report covering all required sections.  Reports are self-contained
//! and inspectable without replaying loop context.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── REC-009: Report sections ─────────────────────────────────────────────

/// A section within a recursive improvement report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReportSection {
    /// Section title (e.g., "Objective", "Repo Target", "Scores",
    /// "Drift Analysis", "Approvals", "Blockers").
    pub title: String,
    /// Section body (human-readable summary).
    pub body: String,
    /// Structured data for this section (for programmatic consumption).
    pub structured_data: serde_json::Value,
}

/// Next-step recommendation from the report.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NextRecommendation {
    /// What is recommended ("continue", "pause", "escalate", "rollback",
    /// "terminate").
    pub action: String,
    /// Human-readable rationale.
    pub rationale: String,
    /// Prerequisites for acting on this recommendation.
    pub prerequisites: Vec<String>,
}

/// REC-009 -- Recursive improvement report.
///
/// A durable, self-contained report for a self-improvement loop
/// iteration.  Covers all CSV-required sections: objective, repo
/// target, scores, drift, approvals, and blockers.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RecursiveReport {
    /// Unique report identifier.
    pub report_id: String,
    /// The self-improvement objective this report covers.
    pub objective_id: String,
    /// Loop iteration index.
    pub iteration_index: i32,
    /// Report sections (ordered).
    pub sections: Vec<ReportSection>,
    /// Next-step recommendations.
    pub recommendations: Vec<NextRecommendation>,
    /// References to related artifacts (comparison_id, score_id,
    /// drift_check_id, etc.).
    pub related_artifact_refs: Vec<String>,
    /// Whether all required sections are present.
    pub is_complete: bool,
    /// Whether any blockers were identified.
    pub has_blockers: bool,
    pub generated_at: DateTime<Utc>,
}
