//! REC-005: Loop-to-loop improvement scoring.
//!
//! CSV guardrail: "Implement loop-to-loop improvement scoring (advisory
//!   scores: throughput, stability, review debt, certification pressure,
//!   regression risk)."
//! auto_approval_policy: never_silent
//!
//! Acceptance: scores are advisory only -- they inform decisions but do
//! not autonomously gate or approve.  Every scoring run produces a
//! durable artifact.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Inputs to the scoring function.  Gathered from loop telemetry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScoringInput {
    /// Number of tasks completed in this loop iteration.
    pub tasks_completed: i32,
    /// Number of tasks failed in this loop iteration.
    pub tasks_failed: i32,
    /// Wall-clock duration of the loop iteration in seconds.
    pub iteration_duration_secs: f64,
    /// Number of reviews pending at end of iteration.
    pub pending_reviews: i32,
    /// Number of certifications pending at end of iteration.
    pub pending_certifications: i32,
    /// Number of regressions detected.
    pub regressions_detected: i32,
    /// Number of drift warnings raised (REC-007).
    pub drift_warnings: i32,
}

/// Breakdown of the advisory score into its component dimensions.
/// Each dimension is a value in [0.0, 1.0] where higher is better.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ScoreBreakdown {
    /// Throughput: ratio of completed tasks to attempted tasks.
    pub throughput: f64,
    /// Stability: inverse of regression rate.
    pub stability: f64,
    /// Review debt: inverse of pending review ratio.
    pub review_debt: f64,
    /// Certification pressure: inverse of pending certification ratio.
    pub certification_pressure: f64,
    /// Regression risk: inverse of regression count.
    pub regression_risk: f64,
}

/// Loop score.
///
/// Advisory-only score for a self-improvement loop iteration.  Scores
/// inform human reviewers and governance but never autonomously approve
/// or gate actions (CSV: "advisory scores").
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct LoopScore {
    /// Unique score identifier.
    pub score_id: String,
    /// The self-improvement objective this score belongs to.
    pub objective_id: String,
    /// Loop iteration index.
    pub iteration_index: i32,
    /// Raw inputs used to compute the score.
    pub input: ScoringInput,
    /// Breakdown of the composite score.
    pub breakdown: ScoreBreakdown,
    /// Composite advisory score in [0.0, 1.0].
    pub composite_score: f64,
    /// Whether this score is advisory-only (always true -- CSV constraint).
    pub advisory_only: bool,
    /// Human-readable recommendation based on the score.
    pub recommendation: String,
    pub created_at: DateTime<Utc>,
}

impl LoopScore {
    /// Compute a composite score from a breakdown.
    /// Equal weighting across all five dimensions.
    pub fn compute_composite(breakdown: &ScoreBreakdown) -> f64 {
        (breakdown.throughput
            + breakdown.stability
            + breakdown.review_debt
            + breakdown.certification_pressure
            + breakdown.regression_risk)
            / 5.0
    }
}
