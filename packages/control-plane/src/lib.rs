//! Control plane -- worker governance core (M3) and live
//! orchestration commands (M4).
//!
//! This crate provides:
//!
//! - **Worker lifecycle** (WRK-001 to WRK-015): Registration, lease
//!   management, heartbeat enforcement, stuck-worker detection,
//!   cancel/kill flows, retry management, and pool status.
//!
//! - **User policy surface** (POL-001 to POL-015): Typed, versioned,
//!   per-cycle-snapshotted execution policy. Policy never lives only
//!   as env vars.
//!
//! - **Commands** (CTL-001 to CTL-015): Typed command structs and
//!   handler trait for the live orchestration loop. All state changes
//!   flow through commands -- UI clicks and projections never mutate
//!   state directly.
//!
//! Key design rules:
//! - Workers use an explicit state machine with typed transitions.
//! - Workers cannot run without an active lease.
//! - Missing heartbeats trigger stuck-worker detection.
//! - All policy is typed structs, versioned, and snapshotted.
//! - Same command idempotency key must not produce duplicate
//!   authoritative transitions.

pub mod commands;
#[cfg(feature = "runtime")]
pub mod executor;
pub mod policy;
pub mod policy_enforcer;
pub mod worker_lifecycle;

// Re-export primary types for ergonomic imports.
pub use commands::{
    CommandHandler, CommandOutcome, CommandRejection, CommandRejectionReason, CommandResult,
    CreateCycleCommand, CreateLoopCommand, CreateNodeFromPlanCommand,
    CreateObjectiveCommand, CreateTaskFromNodeCommand, DispatchResult,
    DispatchSchedulerCommand, DriftRequeueCommand, DriftSource, FailureIngestionCommand,
    FailureKind, LaneAssignmentCommand, LaneTransitionRule, NextCycleGenerationCommand,
    PhaseTransitionCommand, PhaseTransitionRule, PrioritySignal,
    QueuePrioritizationCommand, RetrySchedulingCommand, TaskCompletionCommand,
    TimeoutIngestionCommand, is_valid_lane_transition, is_valid_phase_transition,
    valid_lane_transitions, valid_phase_transitions,
};
pub use policy::{
    AdapterPreference, AdapterSelectionPolicy, CautionEntry, CautionPolicy,
    ConcurrencyPolicy, ExecutionPolicy, OutputFormat, OutputFormatPolicy,
    PolicyCycleSnapshot, PolicyDiff, PolicyDiffEntry, PolicyEvent, PolicyEventKind,
    PolicyField, PolicyOverride, PolicyValidationError, PolicyValidationResult,
    PolicyVersion, RetryBackoff, RetryPolicy, RoleConcurrencyLimit,
    TimeoutAction, TimeoutPolicy, TokenBudgetPolicy, WorkerDispatchPolicy,
};
pub use policy_enforcer::{PolicyDecision, PolicyEnforcer};
pub use worker_lifecycle::{
    CancelFlow, HeartbeatPolicy, HeartbeatRecord, KillFlow, LeaseRenewalRequest,
    LeaseRenewalResult, RetryDecision, StuckWorkerAction, StuckWorkerReport,
    TaskAssignment, TaskAssignmentOutcome, WorkerLease, WorkerPoolStatus,
    WorkerRegistration, WorkerRetryPolicy, WorkerState, WorkerTransition,
    is_valid_transition, valid_transitions,
};
