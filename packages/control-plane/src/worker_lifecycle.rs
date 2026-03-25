//! Worker lifecycle management (WRK-001 through WRK-015).
//!
//! CSV guardrail: "lifecycle simulation; lease expiry check;
//!   stuck-worker detection simulation"
//! Caution: Do not let workers run indefinitely without heartbeat
//!   enforcement.
//!
//! This module implements the worker lifecycle as an explicit state
//! machine with typed transitions. Workers cannot run without an
//! active lease, and missing heartbeats trigger stuck-worker detection.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// The lifecycle state of a registered worker.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum WorkerState {
    /// Worker has been registered but not yet assigned a lease.
    Registered,
    /// Worker has an active lease and is ready to accept tasks.
    Idle,
    /// Worker is actively executing a task.
    Running,
    /// Worker is in the process of graceful shutdown.
    Draining,
    /// Worker's lease has expired without renewal.
    LeaseExpired,
    /// Worker has been flagged as stuck (no heartbeat within threshold).
    Stuck,
    /// Worker has been cancelled (graceful).
    Cancelled,
    /// Worker has been force-killed.
    Killed,
    /// Worker has completed its task and deregistered.
    Deregistered,
}

/// Valid state transitions for a worker.
/// The control plane must reject any transition not listed here.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerTransition {
    pub from: WorkerState,
    pub to: WorkerState,
}

/// Returns the set of valid worker state transitions.
pub fn valid_transitions() -> Vec<WorkerTransition> {
    vec![
        // Registration flow
        WorkerTransition { from: WorkerState::Registered, to: WorkerState::Idle },
        // Task assignment
        WorkerTransition { from: WorkerState::Idle, to: WorkerState::Running },
        // Task completion
        WorkerTransition { from: WorkerState::Running, to: WorkerState::Idle },
        WorkerTransition { from: WorkerState::Running, to: WorkerState::Deregistered },
        // Graceful shutdown
        WorkerTransition { from: WorkerState::Idle, to: WorkerState::Draining },
        WorkerTransition { from: WorkerState::Running, to: WorkerState::Draining },
        WorkerTransition { from: WorkerState::Draining, to: WorkerState::Deregistered },
        // Lease expiry (can happen from Idle or Running)
        WorkerTransition { from: WorkerState::Idle, to: WorkerState::LeaseExpired },
        WorkerTransition { from: WorkerState::Running, to: WorkerState::LeaseExpired },
        // Stuck detection (only from Running)
        WorkerTransition { from: WorkerState::Running, to: WorkerState::Stuck },
        // Cancel and kill
        WorkerTransition { from: WorkerState::Running, to: WorkerState::Cancelled },
        WorkerTransition { from: WorkerState::Stuck, to: WorkerState::Killed },
        WorkerTransition { from: WorkerState::Cancelled, to: WorkerState::Deregistered },
        WorkerTransition { from: WorkerState::Killed, to: WorkerState::Deregistered },
        WorkerTransition { from: WorkerState::LeaseExpired, to: WorkerState::Deregistered },
        // Idle -> Deregistered (voluntary unregister)
        WorkerTransition { from: WorkerState::Idle, to: WorkerState::Deregistered },
    ]
}

/// Check whether a state transition is valid.
pub fn is_valid_transition(from: WorkerState, to: WorkerState) -> bool {
    valid_transitions().iter().any(|t| t.from == from && t.to == to)
}

/// Registration record for a worker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerRegistration {
    /// Unique worker identifier.
    pub worker_id: String,
    /// Worker role (e.g. "implementer", "reviewer").
    pub worker_role: String,
    /// Skill pack this worker uses.
    pub skill_pack_id: String,
    /// Current lifecycle state.
    pub state: WorkerState,
    /// Task kinds this worker accepts.
    pub accepted_task_kinds: Vec<String>,
    /// Maximum concurrent tasks.
    pub max_concurrency: u32,
    /// Provider mode.
    pub provider_mode: String,
    /// Model binding.
    pub model_binding: String,
    /// Registration timestamp.
    pub registered_at: DateTime<Utc>,
    /// Last state change timestamp.
    pub state_changed_at: DateTime<Utc>,
}

/// Worker lease record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerLease {
    /// Unique lease identifier.
    pub lease_id: String,
    /// Worker this lease belongs to.
    pub worker_id: String,
    /// Task this lease is for (if task-scoped).
    pub task_id: Option<String>,
    /// When the lease was granted.
    pub granted_at: DateTime<Utc>,
    /// When the lease expires.
    pub expires_at: DateTime<Utc>,
    /// Whether the lease is currently active.
    pub active: bool,
    /// Number of times this lease has been renewed.
    pub renewal_count: u32,
    /// Maximum allowed renewals before the worker must re-register.
    pub max_renewals: u32,
}

/// Lease renewal request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LeaseRenewalRequest {
    pub lease_id: String,
    pub worker_id: String,
    /// Requested extension in seconds.
    pub extension_seconds: u32,
}

/// Result of a lease renewal attempt.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LeaseRenewalResult {
    /// Renewal succeeded.
    Renewed,
    /// Renewal denied: lease already expired.
    Expired,
    /// Renewal denied: max renewals reached.
    MaxRenewalsReached,
    /// Renewal denied: worker not in valid state.
    InvalidWorkerState,
}

/// Heartbeat record stored by the control plane.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeartbeatRecord {
    /// Unique heartbeat identifier.
    pub heartbeat_id: String,
    /// Worker that sent this heartbeat.
    pub worker_id: String,
    /// Task being executed (if any).
    pub task_id: Option<String>,
    /// Worker's reported status.
    pub status: String,
    /// Progress percentage (0-100).
    pub progress_percent: u8,
    /// Current execution phase.
    pub phase: String,
    /// Resource usage snapshot.
    pub resource_usage: Option<serde_json::Value>,
    /// Timestamp of the heartbeat.
    pub received_at: DateTime<Utc>,
}

/// Heartbeat policy defining expected intervals and thresholds.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeartbeatPolicy {
    /// Expected interval between heartbeats in seconds.
    pub interval_seconds: u32,
    /// Number of missed heartbeats before the worker is considered stuck.
    pub missed_threshold: u32,
    /// Whether to automatically kill stuck workers.
    pub auto_kill_stuck: bool,
    /// Grace period in seconds after stuck detection before kill.
    pub kill_grace_seconds: u32,
}

impl Default for HeartbeatPolicy {
    fn default() -> Self {
        Self {
            interval_seconds: 30,
            missed_threshold: 3,
            auto_kill_stuck: false,
            kill_grace_seconds: 60,
        }
    }
}

/// Stuck worker detection result.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StuckWorkerReport {
    /// Worker that is stuck.
    pub worker_id: String,
    /// Task the worker was executing.
    pub task_id: Option<String>,
    /// Last heartbeat received.
    pub last_heartbeat_at: Option<DateTime<Utc>>,
    /// How many heartbeats were missed.
    pub missed_heartbeats: u32,
    /// Time since last heartbeat in seconds.
    pub seconds_since_heartbeat: u64,
    /// Action taken by the control plane.
    pub action_taken: StuckWorkerAction,
}

/// Action taken when a stuck worker is detected.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum StuckWorkerAction {
    /// Worker state transitioned to Stuck, awaiting manual intervention.
    MarkedStuck,
    /// Cancel request sent to worker.
    CancelSent,
    /// Kill request sent after cancel grace period expired.
    KillSent,
    /// Worker's lease was revoked and task re-queued.
    LeaseRevokedTaskRequeued,
}

/// Task assignment binding a worker to a task.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskAssignment {
    /// Unique assignment identifier.
    pub assignment_id: String,
    /// Worker assigned to this task.
    pub worker_id: String,
    /// Task being executed.
    pub task_id: String,
    /// Lease covering this assignment.
    pub lease_id: String,
    /// Attempt number (1-indexed).
    pub attempt_number: u32,
    /// Assignment timestamp.
    pub assigned_at: DateTime<Utc>,
    /// Completion timestamp (None if still running).
    pub completed_at: Option<DateTime<Utc>>,
    /// Outcome of the assignment.
    pub outcome: Option<TaskAssignmentOutcome>,
}

/// Outcome of a task assignment.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskAssignmentOutcome {
    /// Task completed successfully.
    Succeeded,
    /// Task failed with a retryable error.
    FailedRetryable,
    /// Task failed with a permanent error.
    FailedPermanent,
    /// Task was cancelled.
    Cancelled,
    /// Task timed out.
    TimedOut,
    /// Worker was killed during execution.
    WorkerKilled,
    /// Worker's lease expired during execution.
    LeaseExpired,
}

/// Cancel flow record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CancelFlow {
    pub worker_id: String,
    pub task_id: String,
    /// Reason for cancellation.
    pub reason: String,
    /// Whether the cancel request was acknowledged by the worker.
    pub acknowledged: bool,
    /// Grace period in seconds before escalation to kill.
    pub grace_period_seconds: u32,
    /// Cancel request timestamp.
    pub requested_at: DateTime<Utc>,
    /// Acknowledgement timestamp (None if not acknowledged).
    pub acknowledged_at: Option<DateTime<Utc>>,
    /// Whether the cancel escalated to a kill.
    pub escalated_to_kill: bool,
}

/// Kill flow record.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct KillFlow {
    pub worker_id: String,
    pub task_id: String,
    /// Reason for the kill.
    pub reason: String,
    /// Kill request timestamp.
    pub requested_at: DateTime<Utc>,
    /// Whether the worker process was successfully terminated.
    pub terminated: bool,
    /// Termination timestamp.
    pub terminated_at: Option<DateTime<Utc>>,
}

/// Retry policy for failed task assignments.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerRetryPolicy {
    /// Maximum retry attempts.
    pub max_attempts: u32,
    /// Backoff base in milliseconds.
    pub backoff_base_ms: u32,
    /// Backoff multiplier (exponential).
    pub backoff_multiplier: u32,
    /// Maximum backoff in milliseconds.
    pub max_backoff_ms: u32,
    /// Whether to retry on timeout.
    pub retry_on_timeout: bool,
    /// Whether to retry on lease expiry.
    pub retry_on_lease_expiry: bool,
}

impl Default for WorkerRetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            backoff_base_ms: 1000,
            backoff_multiplier: 2,
            max_backoff_ms: 30_000,
            retry_on_timeout: true,
            retry_on_lease_expiry: true,
        }
    }
}

/// Retry decision made by the control plane.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetryDecision {
    pub task_id: String,
    pub attempt_number: u32,
    pub max_attempts: u32,
    /// Whether the task should be retried.
    pub should_retry: bool,
    /// Delay in milliseconds before the retry.
    pub delay_ms: u64,
    /// Reason for the retry decision.
    pub reason: String,
}

/// Aggregate status of the worker pool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerPoolStatus {
    /// Total registered workers.
    pub total_registered: u32,
    /// Workers in Idle state.
    pub idle_count: u32,
    /// Workers in Running state.
    pub running_count: u32,
    /// Workers in Stuck state.
    pub stuck_count: u32,
    /// Workers with expired leases.
    pub lease_expired_count: u32,
    /// Workers in Draining state.
    pub draining_count: u32,
    /// Active leases.
    pub active_leases: u32,
    /// Tasks currently assigned.
    pub tasks_assigned: u32,
    /// Tasks queued waiting for a worker.
    pub tasks_queued: u32,
}
