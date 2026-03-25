//! UI metrics projection types.
//!
//! Projection types are **not** authoritative metrics. They are
//! derived, estimated, or aggregated summaries intended only for UI
//! display and operator dashboards. They must never be stored alongside
//! or confused with the authoritative records in `metrics.rs`.
//!
//! CSV caution: "Do not mix projection-only counters with authoritative
//!   metrics; do not blur estimated vs provider-reported counts."

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::metrics::PressureLevel;

/// Projected (estimated) cost summary for dashboard display.
/// All figures here are best-effort aggregations and may lag behind
/// authoritative `CostRecord` entries.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectedCostSummary {
    /// The time window this projection covers.
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    /// Total estimated cost across all providers.
    pub total_estimated_cost: f64,
    /// Currency code (ISO 4217).
    pub currency: String,
    /// Per-provider breakdown (provider name -> estimated cost).
    pub per_provider: Vec<ProviderCostProjection>,
    /// Timestamp when this projection was last recomputed.
    pub projected_at: DateTime<Utc>,
}

/// Per-provider cost projection for dashboard display.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProviderCostProjection {
    pub provider: String,
    pub estimated_cost: f64,
}

/// Projected token-usage summary for dashboard display.
/// Combines provider-reported and estimated counts into a single
/// display-oriented summary, clearly labelled as a projection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectedTokenUsage {
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    /// Total input tokens (may mix provider-reported and estimated).
    pub total_input_tokens: u64,
    /// Total output tokens (may mix provider-reported and estimated).
    pub total_output_tokens: u64,
    /// Fraction of token counts that are provider-reported (0.0..1.0
    /// represented as integer basis points for Eq compliance, e.g.
    /// 9500 = 95.00%).
    pub provider_reported_bps: u32,
    pub projected_at: DateTime<Utc>,
}

/// Projected cycle health summary for dashboard display. Combines
/// saturation, success-rate, and blocking-cause data into a single
/// high-level health signal.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ProjectedCycleHealth {
    /// Number of cycles completed in the window.
    pub cycles_completed: u32,
    /// Number of cycles currently in progress.
    pub cycles_in_progress: u32,
    /// Average cycle duration in milliseconds (projected).
    pub avg_cycle_duration_ms: f64,
    /// Current system pressure level.
    pub pressure_level: PressureLevel,
    /// Aggregate worker success rate across all roles.
    pub aggregate_success_rate: f64,
    pub projected_at: DateTime<Utc>,
}

/// Freshness status of a projection, distinguishing a stale-but-valid
/// projection from one that failed to rebuild.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionFreshness {
    /// Projection was recomputed within the expected interval.
    Fresh,
    /// Projection is older than expected but structurally valid.
    Stale,
    /// Projection rebuild failed; last-known-good data is shown.
    RebuildFailed,
}

/// Top-level UI metrics projection bundle. This is the single type
/// that the UI layer consumes. It aggregates all projection sub-types
/// and is recomputed periodically from authoritative records.
///
/// CSV acceptance: "UI metrics projections with freshness and version
/// markers for dashboards and policy panels."
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct UiMetricsProjection {
    pub cost_summary: ProjectedCostSummary,
    pub token_usage: ProjectedTokenUsage,
    pub cycle_health: ProjectedCycleHealth,
    /// Monotonically increasing version counter for cache-busting.
    pub version: u64,
    /// Whether this projection data is still fresh or has gone stale.
    pub freshness: ProjectionFreshness,
    /// Timestamp of the most recent recomputation.
    pub projected_at: DateTime<Utc>,
}
