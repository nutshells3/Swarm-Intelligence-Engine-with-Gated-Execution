//! Authoritative metrics types (OBS-001 through OBS-006).
//!
//! These are the source-of-truth counters emitted by the control plane
//! and persisted durably. Projection-only aggregations live in
//! `projections.rs` and are never stored as authoritative records.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── OBS-001: Cycle metrics ──────────────────────────────────────────────
//
// CSV guardrail: "Define durable metrics for cycles."
// Caution: "Do not mix projection-only counters with authoritative
//   metrics."
// Acceptance: schema validation; metrics replay check.

/// A cause that blocked progress during a cycle. Recorded so operators
/// can distinguish scheduling delays from certification bottlenecks.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BlockingCause {
    /// Human-readable label (e.g. "certification_queue_full").
    pub label: String,
    /// How long the block lasted, in milliseconds.
    pub duration_ms: u64,
    /// The phase during which the block occurred.
    pub phase: String,
}

/// Authoritative timing and outcome metrics for a single cycle.
///
/// All durations are wall-clock milliseconds measured by the control
/// plane. These are authoritative, not projections.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CycleMetrics {
    pub cycle_id: String,
    /// Total wall-clock duration of the cycle.
    pub duration_ms: u64,
    /// Time spent waiting in the ready queue before execution began.
    pub queue_time_ms: u64,
    /// Time spent in the execution phase.
    pub execution_time_ms: u64,
    /// Time spent in the review phase.
    pub review_time_ms: u64,
    /// Time spent in the certification phase.
    pub certification_time_ms: u64,
    /// Number of tasks that completed successfully.
    pub tasks_completed: u32,
    /// Number of tasks that failed.
    pub tasks_failed: u32,
    /// Causes that blocked forward progress during this cycle.
    pub blocking_causes: Vec<BlockingCause>,
    pub recorded_at: DateTime<Utc>,
}

// ── OBS-002: Task metrics ───────────────────────────────────────────────
//
// CSV guardrail: "Define durable metrics for tasks."
// Acceptance: schema validation; metrics replay check.

/// Authoritative metrics for a single task execution attempt.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskMetrics {
    pub task_id: String,
    pub cycle_id: String,
    pub worker_role: String,
    /// Wall-clock duration of the task, in milliseconds.
    pub duration_ms: u64,
    /// Number of retry attempts before this outcome.
    pub retry_count: u32,
    /// Whether the task succeeded.
    pub succeeded: bool,
    /// Optional failure category when succeeded is false.
    pub failure_category: Option<String>,
    pub recorded_at: DateTime<Utc>,
}

// ── OBS-003: Cost accounting ────────────────────────────────────────────
//
// CSV guardrail: "Define durable metrics for costs."
// Caution: "Do not blur estimated vs provider-reported counts."
// Acceptance: schema validation; projection fixture check.

/// Provenance of a cost record. Distinguishes provider-reported
/// (authoritative) costs from local estimates so downstream consumers
/// never confuse them.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum CostProvenance {
    /// Cost reported by the provider's billing API (authoritative).
    ProviderReported,
    /// Cost estimated locally from token counts and published pricing.
    LocalEstimate,
    /// Cost derived from a metering proxy or gateway.
    MeteringProxy,
}

/// A single cost record. Each provider call that incurs cost produces
/// one record. Estimated and provider-reported records carry distinct
/// `source_provenance` values so they are never conflated.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CostRecord {
    pub record_id: String,
    pub cycle_id: String,
    /// The task that incurred this cost, if attributable.
    pub task_id: Option<String>,
    /// Provider name (e.g. "openai", "anthropic").
    pub provider: String,
    /// Cost amount in the smallest currency unit (e.g. cents).
    pub cost_amount: f64,
    /// ISO 4217 currency code.
    pub cost_currency: String,
    /// Whether this figure is provider-reported or estimated.
    pub source_provenance: CostProvenance,
    pub recorded_at: DateTime<Utc>,
}

// ── OBS-004: Token accounting ───────────────────────────────────────────
//
// CSV guardrail: "Define durable metrics for tokens."
// Caution: "Do not blur estimated vs provider-reported counts."
// Acceptance: schema validation; projection fixture check.

/// Provenance of a token count. Ensures consumers can distinguish exact
/// figures from estimates.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum TokenProvenance {
    /// Count reported by the provider's usage API (authoritative).
    ProviderReported,
    /// Count estimated locally from a tokenizer approximation.
    LocalEstimate,
}

/// A single token-accounting record. Input and output tokens are
/// recorded separately, each with its own provenance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TokenAccountingRecord {
    pub record_id: String,
    pub cycle_id: String,
    pub task_id: Option<String>,
    pub provider: String,
    pub model: String,
    /// Number of input (prompt) tokens.
    pub input_tokens: u64,
    /// Provenance of the input token count.
    pub input_provenance: TokenProvenance,
    /// Number of output (completion) tokens.
    pub output_tokens: u64,
    /// Provenance of the output token count.
    pub output_provenance: TokenProvenance,
    pub recorded_at: DateTime<Utc>,
}

// ── OBS-005: Worker success rates ───────────────────────────────────────
//
// CSV guardrail: "Define durable metrics for worker success rates."
// Acceptance: schema validation; metrics replay check.

/// Rolling success-rate snapshot for a worker role within a time window.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WorkerSuccessRate {
    /// The worker role being measured (e.g. "implementer", "reviewer").
    pub worker_role: String,
    /// Start of the measurement window.
    pub window_start: DateTime<Utc>,
    /// End of the measurement window.
    pub window_end: DateTime<Utc>,
    /// Total task attempts in the window.
    pub total_attempts: u32,
    /// Successful completions.
    pub successes: u32,
    /// Failures (retries exhausted, timeouts, malformed output, etc.).
    pub failures: u32,
    /// Success rate as a ratio [0.0, 1.0].
    pub success_rate: f64,
}

// ── OBS-006: Saturation metrics ─────────────────────────────────────────
//
// CSV guardrail: "Define durable metrics for saturation."
// Acceptance: schema validation; metrics replay check.

/// Qualitative pressure level derived from saturation counters. The
/// control plane may use this to throttle intake or scale workers.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PressureLevel {
    /// System is well within capacity.
    Low,
    /// Queues are growing; consider throttling intake.
    Moderate,
    /// Backlogs are significant; new objectives should be deferred.
    High,
    /// System is at capacity; only drain and recovery work proceeds.
    Critical,
}

/// Point-in-time saturation snapshot. Recorded periodically by the
/// control plane to track queue depths and backlogs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SaturationMetrics {
    /// Number of tasks in the ready queue awaiting dispatch.
    pub ready_queue_depth: u32,
    /// Number of tasks currently being executed by workers.
    pub running_tasks: u32,
    /// Number of tasks blocked (dependency, conflict, etc.).
    pub blocked_tasks: u32,
    /// Number of artifacts awaiting review.
    pub review_backlog: u32,
    /// Number of artifacts awaiting certification.
    pub certification_backlog: u32,
    /// Derived pressure level.
    pub pressure_level: PressureLevel,
    pub recorded_at: DateTime<Utc>,
}
