//! Observability schemas for metrics, sidecars, and projections (M6).
//!
//! This crate defines the durable, machine-readable metrics types for
//! cycles, tasks, costs, tokens, worker success rates, and saturation.
//!
//! Items: OBS-001 through OBS-010.
//!
//! Key design rules:
//! - Projection-only counters (OBS-010) are never mixed with authoritative
//!   metrics. Projection types live in their own module and are clearly
//!   labelled as estimates.
//! - Estimated token counts are always distinguished from provider-reported
//!   counts via the `TokenProvenance` enum.
//! - All metrics are typed structs with explicit provenance, not raw
//!   counters embedded in application code.

pub mod metrics;
pub mod projections;
pub mod sidecars;

// Re-export primary types for ergonomic imports.
pub use metrics::{
    BlockingCause, CostProvenance, CostRecord, CycleMetrics, SaturationMetrics,
    TaskMetrics, TokenAccountingRecord, TokenProvenance, WorkerSuccessRate,
    PressureLevel,
};
pub use projections::{
    ProjectedCostSummary, ProjectedCycleHealth, ProjectedTokenUsage,
    ProjectionFreshness, UiMetricsProjection,
};
pub use sidecars::{
    PhaseStatusSidecar, RetryableFailureMetric, RetryableFailureKind,
    SessionHeartbeatLog,
};
