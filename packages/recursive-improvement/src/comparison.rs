//! REC-004: Previous-vs-next comparison summaries.
//!
//! CSV guardrail: "Implement previous-vs-next comparison summaries
//!   (comparison artifact: baseline, proposal, changed surfaces, metric
//!   deltas, risks)."
//! Caution: comparison artifacts must be durable and reviewable without
//!   replaying the full loop context.
//! auto_approval_policy: never_silent
//!
//! Acceptance: every self-improvement loop iteration produces a durable
//! comparison artifact that captures the baseline, proposal, changed
//! surfaces, metric deltas, and regression risks.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── REC-004: Comparison baseline ─────────────────────────────────────────

/// The baseline snapshot against which a self-improvement proposal is
/// compared.  Must be captured before the improvement starts so the
/// artifact is self-contained (CSV: reviewable without replaying).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComparisonBaseline {
    /// Git commit hash or state snapshot ID of the baseline.
    pub snapshot_ref: String,
    /// Summary of the baseline state.
    pub summary: String,
    /// Key metrics at baseline (structured JSON for flexibility).
    pub metrics: serde_json::Value,
}

/// A single metric delta between baseline and proposal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct MetricDelta {
    /// Name of the metric.
    pub metric_name: String,
    /// Baseline value (as string for uniform serialization).
    pub baseline_value: String,
    /// Proposed value.
    pub proposed_value: String,
    /// Direction: "improved", "degraded", or "unchanged".
    pub direction: String,
    /// Magnitude of change (percentage or absolute).
    pub magnitude: String,
}

/// Regression risk identified in a comparison.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RegressionRisk {
    /// What could regress.
    pub description: String,
    /// Severity: "low", "medium", "high", "critical".
    pub severity: String,
    /// Mitigation strategy.
    pub mitigation: String,
    /// Whether this risk blocks integration.
    pub blocks_integration: bool,
}

/// REC-004 -- Comparison artifact.
///
/// A durable record comparing the previous state (baseline) with the
/// proposed improvement.  Includes changed surfaces, metric deltas, and
/// regression risks.  Designed to be self-contained so reviewers can
/// inspect it without replaying the full loop context.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ComparisonArtifact {
    /// Unique comparison artifact identifier.
    pub comparison_id: String,
    /// The self-improvement objective this comparison belongs to.
    pub objective_id: String,
    /// Loop iteration index (which loop-to-loop step produced this).
    pub iteration_index: i32,
    /// The baseline state before the improvement.
    pub baseline: ComparisonBaseline,
    /// Summary of the proposal.
    pub proposal_summary: String,
    /// List of changed surfaces (file paths, API endpoints, schemas, etc.).
    pub changed_surfaces: Vec<String>,
    /// Metric deltas between baseline and proposal.
    pub metric_deltas: Vec<MetricDelta>,
    /// Identified regression risks.
    pub regression_risks: Vec<RegressionRisk>,
    /// Overall assessment: "safe_to_proceed", "needs_review", "blocked".
    pub overall_assessment: String,
    pub created_at: DateTime<Utc>,
}
