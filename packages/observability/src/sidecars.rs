//! Sidecar observability types (OBS-007 through OBS-009).
//!
//! Sidecars are lightweight status records that ride alongside the
//! primary event stream. They are not authoritative state transitions
//! but durable diagnostic signals for operators and dashboards.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

// ── OBS-007: Phase-status sidecars ──────────────────────────────────────
//
// CSV guardrail: "Define durable metrics for cycle phase status."
// Acceptance: schema validation; metrics replay check.

/// Current status of a cycle phase, emitted as a sidecar record each
/// time the phase changes or at a regular heartbeat interval.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum PhaseStatus {
    /// Phase has not started yet.
    Pending,
    /// Phase is actively executing.
    Active,
    /// Phase completed successfully.
    Completed,
    /// Phase was skipped (e.g. certification disabled).
    Skipped,
    /// Phase failed and triggered escalation.
    Failed,
    /// Phase is blocked waiting on an external dependency.
    Blocked,
}

/// A sidecar record tracking the status of a single cycle phase.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhaseStatusSidecar {
    pub cycle_id: String,
    /// The phase name (matches CyclePhase variant name).
    pub phase_name: String,
    pub status: PhaseStatus,
    /// Wall-clock milliseconds spent in this phase so far.
    pub elapsed_ms: u64,
    /// Optional human-readable note (e.g. "waiting for Lean server").
    pub note: Option<String>,
    pub recorded_at: DateTime<Utc>,
}

// ── OBS-008: Session heartbeat logs ─────────────────────────────────────
//
// CSV guardrail: "Define durable metrics for session liveness."
// Acceptance: schema validation; metrics replay check.

/// A heartbeat record emitted by a session (orchestration loop instance)
/// at regular intervals. Used for liveness detection and session-level
/// diagnostics.
///
/// CSV caution: use bounded cadence and archive policy -- do not flood
/// the state store with unbounded heartbeat noise.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionHeartbeatLog {
    pub session_id: String,
    /// Monotonically increasing sequence number within the session.
    pub sequence: u64,
    /// Number of active cycles in this session.
    pub active_cycles: u32,
    /// Number of active workers in this session.
    pub active_workers: u32,
    /// Whether the session considers itself healthy.
    pub healthy: bool,
    /// Optional diagnostic message if unhealthy.
    pub diagnostic: Option<String>,
    /// Cadence interval in seconds at which this heartbeat is emitted.
    /// Consumers MUST respect this cadence when archiving to prevent
    /// unbounded state-store growth.
    pub cadence_seconds: u32,
    pub recorded_at: DateTime<Utc>,
}

// ── OBS-009: Retryable failure metrics ──────────────────────────────────
//
// CSV guardrail: "Define durable metrics for retryable failures."
// Acceptance: schema validation; metrics replay check.

/// Classification of a retryable failure, used to distinguish transient
/// provider errors from systematic output problems.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum RetryableFailureKind {
    /// Provider returned a transient HTTP error (429, 500, 503, etc.).
    ProviderTransient,
    /// Worker output was malformed (ROB-001 taxonomy applies).
    MalformedOutput,
    /// Worker exceeded its time budget.
    Timeout,
    /// Worker heartbeat was lost (stuck worker).
    HeartbeatLost,
    /// Network or transport error between the control plane and provider.
    TransportError,
    /// Rate limit hit at the provider level.
    RateLimited,
}

/// A single retryable-failure metric record. Persisted so operators can
/// track failure trends and tune retry budgets.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetryableFailureMetric {
    pub task_id: String,
    pub cycle_id: String,
    pub worker_role: String,
    pub failure_kind: RetryableFailureKind,
    /// Which retry attempt this failure corresponds to (1-based).
    pub attempt_number: u32,
    /// Whether the task was eventually retried after this failure.
    pub will_retry: bool,
    /// Optional error message or code from the provider.
    pub error_detail: Option<String>,
    pub recorded_at: DateTime<Utc>,
}
