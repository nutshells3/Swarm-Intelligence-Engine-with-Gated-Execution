//! Control-plane commands (CTL-001 through CTL-015).
//!
//! # Design status
//!
//! The [`CommandHandler`] trait below is a **design contract**, not a runtime
//! implementation.  No struct currently implements it.  Runtime execution of
//! each command is split across two services:
//!
//! - **loop-runner** (`services/loop-runner/src/tick.rs`) -- the tick loop
//!   drives automated lifecycle actions (create loops, cycles, nodes, tasks,
//!   dispatch, completion checks, retry, drift, next-cycle generation).
//! - **orchestration-api** (`services/orchestration-api/src/routes/`) --
//!   HTTP handlers for user/UI-initiated mutations (create objective, create
//!   loop, create cycle, create node, create task, task lifecycle).
//!
//! Both services write through direct SQL inside a transaction + event-journal
//! append, enforcing idempotency via `(aggregate_kind, aggregate_id,
//! idempotency_key)` uniqueness (BND-010).
//!
//! Each command struct below documents the exact function(s) that provide its
//! runtime implementation today.
//!
//! # Future work
//!
//! Lift the inline SQL from tick.rs and the API routes into concrete
//! `CommandHandler` impls backed by a shared transaction context.  This will
//! make command logic unit-testable without a live database and allow the
//! same validation rules to be shared between the API and the tick loop.
//!
//! # Guardrails
//!
//! CSV guardrail: "transition simulation; command idempotency check;
//!   next-task generation simulation; projection update check"
//! Caution: Do not let UI clicks mutate state directly; all live
//!   changes must go through commands and authoritative transitions.
//! Idempotency: same command key must not produce duplicate authoritative
//!   transitions.
//!
//! Each command is a typed struct with an explicit idempotency key.
//! Command execution is the *only* write path for live orchestration
//! state -- projections are derived, never mutated directly.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use state_model::{CyclePhase, NodeLane, NodeLifecycle, TaskStatus};

// ── Shared types ──────────────────────────────────────────────────────────

/// Outcome of executing any command.
///
/// Every command returns a typed result so callers can distinguish
/// success, idempotent no-ops, and rejections without inspecting
/// error strings.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandOutcome {
    /// Whether the command produced a new state change.
    pub applied: bool,
    /// If `applied` is false because the idempotency key was already
    /// consumed, this is `true`.
    pub idempotent_skip: bool,
    /// Human-readable explanation of the outcome.
    pub message: String,
    /// IDs of events produced by this command (empty on skip/reject).
    pub event_ids: Vec<String>,
    /// Timestamp of command execution.
    pub executed_at: DateTime<Utc>,
}

/// Reason a command was rejected.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommandRejectionReason {
    /// The idempotency key has already been consumed.
    DuplicateIdempotencyKey,
    /// A required precondition was not met.
    PreconditionFailed,
    /// The requested state transition is illegal.
    IllegalTransition,
    /// A referenced entity was not found.
    EntityNotFound,
    /// The command payload failed validation.
    ValidationError,
}

/// Error returned when a command is rejected.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CommandRejection {
    pub reason: CommandRejectionReason,
    pub detail: String,
}

/// Unified command result type.
pub type CommandResult = Result<CommandOutcome, CommandRejection>;

// ── CTL-001: Objective creation command ───────────────────────────────────
//
// Starts authoritative state through one write path.
// Produces an ObjectiveCreated event.

/// CTL-001: Create a new objective.
///
/// The control plane must verify that `summary` and `desired_outcome`
/// are non-empty. The idempotency key prevents duplicate creation.
///
/// Runtime implementations:
/// - API: `services/orchestration-api/src/routes/objectives.rs::create_objective`
///   -- inserts into `objectives` table, emits `objective_created` event.
/// - Tick: `services/loop-runner/src/tick.rs::create_loops_for_new_objectives`
///   -- detects new objectives (no loop yet) and auto-creates a loop (CTL-002).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateObjectiveCommand {
    /// Idempotency key -- same key must not produce duplicate transitions.
    pub idempotency_key: String,
    /// One-sentence summary of the objective.
    pub summary: String,
    /// Concrete desired outcome.
    pub desired_outcome: String,
    /// Optional conversation that originated this objective.
    pub source_conversation_id: Option<String>,
}

// ── CTL-002: Loop creation command ────────────────────────────────────────
//
// Creates an orchestration loop tied to an objective.

/// CTL-002: Create a loop for an objective.
///
/// A loop is the container for cycles. Each objective has at most one
/// active loop at a time.
///
/// Runtime implementations:
/// - API: `services/orchestration-api/src/routes/loops.rs::create_loop`
///   -- manual loop creation via HTTP POST, with idempotency check.
/// - Tick: `services/loop-runner/src/tick.rs::create_loops_for_new_objectives`
///   -- auto-creates a loop for every objective that lacks one.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateLoopCommand {
    pub idempotency_key: String,
    /// The objective this loop belongs to.
    pub objective_id: String,
    /// Initial active track (e.g. "main", "exploration").
    pub initial_track: String,
}

// ── CTL-003: Cycle creation command ───────────────────────────────────────
//
// Creates a cycle within a loop with durable phase/status initialization.
// Depends on CTL-001, CTL-002.

/// CTL-003: Create a cycle within a loop.
///
/// Each cycle begins in `Intake` phase. The policy snapshot is
/// captured at creation time and frozen for the cycle's lifetime.
///
/// Runtime implementations:
/// - API: `services/orchestration-api/src/routes/cycles.rs::create_cycle`
///   -- manual cycle creation; server derives phase (`intake`) and policy
///   snapshot (BND-003).
/// - Tick: `services/loop-runner/src/tick.rs::create_cycles_for_active_loops`
///   -- auto-creates a cycle for loops whose latest cycle is terminal
///   (`next_cycle_ready`) or that have no cycle at all.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateCycleCommand {
    pub idempotency_key: String,
    /// The loop this cycle belongs to.
    pub loop_id: String,
    /// Cycle index (monotonically increasing within the loop).
    pub cycle_index: i32,
    /// Policy snapshot ID to attach (captured at cycle start).
    pub policy_snapshot_id: String,
}

// ── CTL-004: Node creation from plan ──────────────────────────────────────
//
// Converts a milestone/plan node into an execution node.

/// CTL-004: Create an execution node from a planning artifact.
///
/// This bridges the planning layer (milestones) into the execution
/// layer (nodes). The node inherits lane and lifecycle defaults.
///
/// Runtime implementations:
/// - API: `services/orchestration-api/src/routes/nodes.rs::create_node`
///   -- manual node creation via HTTP POST; server derives lifecycle
///   (`proposed`) per BND-003.
/// - Tick: `services/loop-runner/src/tick.rs::bridge_milestones_to_nodes`
///   (line ~828) -- during decomposition, converts each `milestone_node`
///   into an execution `Node` with a `milestone_ref` foreign key, and
///   creates `depends_on` edges mirroring the milestone hierarchy.
/// - Tick: `services/loop-runner/src/tick.rs::decompose_and_create_tasks`
///   (line ~660) -- orchestrates the decomposition phase, calling
///   `bridge_milestones_to_nodes` followed by `create_tasks_for_objective`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateNodeFromPlanCommand {
    pub idempotency_key: String,
    /// Objective the node belongs to.
    pub objective_id: String,
    /// Milestone ID that originated this node.
    pub milestone_id: String,
    /// Human-readable title.
    pub title: String,
    /// Work statement.
    pub statement: String,
    /// Initial lane assignment.
    pub initial_lane: NodeLane,
    /// Initial lifecycle state (usually Proposed or Queued).
    pub initial_lifecycle: NodeLifecycle,
}

// ── CTL-005: Task creation from nodes ─────────────────────────────────────
//
// Decomposes a node into one or more dispatchable tasks.

/// CTL-005: Create a task from an execution node.
///
/// Tasks are the atomic unit of work dispatched to workers.
///
/// Runtime implementations:
/// - API: `services/orchestration-api/src/routes/tasks.rs::create_task`
///   -- manual task creation; server sets initial status to `queued`.
/// - Tick: `services/loop-runner/src/tick.rs::create_tasks_for_objective`
///   (line ~980) -- auto-creates one task per node that lacks tasks,
///   resolving skill-pack via `SkillRegistryLoader`. Tasks whose node has
///   unmet dependencies start `queued`; others start `running`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateTaskFromNodeCommand {
    pub idempotency_key: String,
    /// Node this task belongs to.
    pub node_id: String,
    /// Worker role required for this task.
    pub worker_role: String,
    /// Skill pack to use.
    pub skill_pack_id: String,
    /// Initial task status (usually Queued).
    pub initial_status: TaskStatus,
}

// ── CTL-006: Dispatch scheduler ───────────────────────────────────────────
//
// Matches queued tasks to available workers.

/// CTL-006: Dispatch scheduler command.
///
/// Instructs the control plane to run a dispatch round: match queued
/// tasks to idle workers, respecting concurrency and policy limits.
///
/// Runtime implementations:
/// - Tick: `services/loop-runner/src/tick.rs::dispatch_phase` (line ~1240)
///   -- orchestrates the dispatch phase for cycles in `dispatch`.
/// - Tick: `services/loop-runner/src/tick.rs::dispatch_queued_tasks`
///   (line ~1289) -- finds tasks with status `queued`, marks them
///   `running`, creates a `task_attempt`, and emits `task_dispatched`
///   events.
/// - No direct API equivalent; dispatch is tick-driven only.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DispatchSchedulerCommand {
    pub idempotency_key: String,
    /// Optional: restrict dispatch to tasks within this cycle.
    pub cycle_id: Option<String>,
    /// Maximum number of tasks to dispatch in this round.
    pub max_dispatches: u32,
}

/// Result of a dispatch round.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DispatchResult {
    /// Base outcome.
    pub outcome: CommandOutcome,
    /// Number of tasks dispatched.
    pub dispatched_count: u32,
    /// Number of tasks that could not be dispatched (no worker available).
    pub skipped_count: u32,
    /// IDs of tasks that were dispatched.
    pub dispatched_task_ids: Vec<String>,
}

// ── CTL-007: Per-phase transition rules ───────────────────────────────────
//
// Governs cycle phase transitions with explicit legality checks.

/// CTL-007: Phase transition command.
///
/// Transitions a cycle from one phase to the next. The control plane
/// must validate that the transition is legal (see `valid_phase_transitions`).
///
/// Runtime implementations:
/// - Tick: `services/loop-runner/src/tick.rs::advance_cycle_phase` (line ~3851)
///   -- shared helper used by every tick function that advances a cycle
///   phase (e.g. `advance_intake_cycles`, `check_plan_gates`,
///   `decompose_and_create_tasks`, `dispatch_phase`,
///   `check_execution_completion`, `complete_integration`).
/// - Tick: `services/loop-runner/src/tick.rs::advance_intake_cycles`
///   (line ~390) -- intake -> plan_elaboration auto-advance.
/// - No direct API equivalent; phase transitions are tick-driven.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhaseTransitionCommand {
    pub idempotency_key: String,
    /// Cycle to transition.
    pub cycle_id: String,
    /// Current phase (must match the cycle's actual phase).
    pub from_phase: CyclePhase,
    /// Target phase.
    pub to_phase: CyclePhase,
    /// Reason for the transition.
    pub reason: String,
}

/// A valid phase transition in the cycle state machine.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct PhaseTransitionRule {
    pub from: CyclePhase,
    pub to: CyclePhase,
}

/// Returns the set of valid cycle phase transitions.
///
/// The cycle follows a linear pipeline with optional skip-backs
/// for review failures.
pub fn valid_phase_transitions() -> Vec<PhaseTransitionRule> {
    use CyclePhase::*;
    vec![
        PhaseTransitionRule { from: Intake, to: ConversationExtraction },
        PhaseTransitionRule { from: ConversationExtraction, to: PlanElaboration },
        PhaseTransitionRule { from: PlanElaboration, to: PlanValidation },
        PhaseTransitionRule { from: PlanValidation, to: Review },
        // Review can advance or send back to elaboration.
        PhaseTransitionRule { from: Review, to: Decomposition },
        PhaseTransitionRule { from: Review, to: PlanElaboration },
        PhaseTransitionRule { from: Decomposition, to: Dispatch },
        PhaseTransitionRule { from: Dispatch, to: Execution },
        PhaseTransitionRule { from: Execution, to: Integration },
        PhaseTransitionRule { from: Integration, to: CertificationSelection },
        PhaseTransitionRule { from: CertificationSelection, to: Certification },
        PhaseTransitionRule { from: Certification, to: StateUpdate },
        // Certification failure sends back to execution.
        PhaseTransitionRule { from: Certification, to: Execution },
        PhaseTransitionRule { from: StateUpdate, to: NextCycleReady },
    ]
}

/// Check whether a phase transition is valid.
pub fn is_valid_phase_transition(from: CyclePhase, to: CyclePhase) -> bool {
    valid_phase_transitions().iter().any(|r| r.from == from && r.to == to)
}

// ── CTL-008: Queue prioritization ─────────────────────────────────────────
//
// Reorders queued tasks based on priority signals.

/// Priority signal used for queue ordering.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrioritySignal {
    /// Dependency chain depth -- deeper chains get priority.
    DependencyDepth,
    /// User-assigned urgency.
    UserUrgency,
    /// Retry count -- tasks with fewer retries get priority.
    RetryCount,
    /// Time in queue -- older tasks get priority.
    QueueAge,
    /// Blocking count -- tasks blocking more downstream work get priority.
    BlockingFanOut,
}

/// CTL-008: Queue prioritization command.
///
/// Instructs the control plane to re-prioritize the task queue using
/// the specified signals.
///
/// Runtime implementations:
/// - **Not yet implemented.** No tick function or API handler currently
///   re-orders the task queue by priority signals. Tasks are dispatched
///   in insertion order by `dispatch_queued_tasks`.
/// - Future: implement as a pre-dispatch sweep in tick.rs or as an
///   on-demand API endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueuePrioritizationCommand {
    pub idempotency_key: String,
    /// Cycle to prioritize (if None, all queued tasks).
    pub cycle_id: Option<String>,
    /// Priority signals in order of importance.
    pub signals: Vec<PrioritySignal>,
}

// ── CTL-009: Branch/mainline lane assignment ──────────────────────────────
//
// Assigns or reassigns a node's lane.

/// CTL-009: Lane assignment command.
///
/// Moves a node between lanes (Branch, MainlineCandidate, Mainline, etc.)
/// following the lane transition rules.
///
/// Runtime implementations:
/// - Tick: `services/loop-runner/src/tick.rs::apply_certification_result`
///   (line ~3681) -- after a certification pass, promotes the node from
///   `Branch` to `MainlineCandidate` (or `Mainline` for gold-gate).
/// - No direct API equivalent; lane changes are driven by certification
///   results in tick.rs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaneAssignmentCommand {
    pub idempotency_key: String,
    /// Node to reassign.
    pub node_id: String,
    /// Current lane (must match actual).
    pub from_lane: NodeLane,
    /// Target lane.
    pub to_lane: NodeLane,
    /// Reason for the lane change.
    pub reason: String,
}

/// A valid lane transition.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaneTransitionRule {
    pub from: NodeLane,
    pub to: NodeLane,
}

/// Returns the set of valid lane transitions.
pub fn valid_lane_transitions() -> Vec<LaneTransitionRule> {
    vec![
        LaneTransitionRule { from: NodeLane::Branch, to: NodeLane::MainlineCandidate },
        LaneTransitionRule { from: NodeLane::MainlineCandidate, to: NodeLane::Mainline },
        LaneTransitionRule { from: NodeLane::MainlineCandidate, to: NodeLane::Branch },
        LaneTransitionRule { from: NodeLane::Branch, to: NodeLane::Blocked },
        LaneTransitionRule { from: NodeLane::MainlineCandidate, to: NodeLane::Blocked },
        LaneTransitionRule { from: NodeLane::Blocked, to: NodeLane::Branch },
        LaneTransitionRule { from: NodeLane::Branch, to: NodeLane::Archived },
        LaneTransitionRule { from: NodeLane::Mainline, to: NodeLane::Archived },
    ]
}

/// Check whether a lane transition is valid.
pub fn is_valid_lane_transition(from: NodeLane, to: NodeLane) -> bool {
    valid_lane_transitions().iter().any(|r| r.from == from && r.to == to)
}

// ── CTL-010: Task completion ingestion ────────────────────────────────────
//
// Ingests a task completion event and updates node/cycle state.

/// CTL-010: Task completion command.
///
/// Records that a task has completed (successfully or otherwise).
/// The control plane updates the node lifecycle and checks whether
/// the cycle can advance.
///
/// Runtime implementations:
/// - API: `services/orchestration-api/src/routes/task_lifecycle.rs::complete_task`
///   -- POST `/api/tasks/{id}/complete`; transitions task to `succeeded`,
///   updates node lifecycle, finishes the running attempt, stores artifacts,
///   and triggers dependency unblocking.
/// - API: `services/orchestration-api/src/routes/task_lifecycle.rs::patch_task`
///   -- PATCH `/api/tasks/{id}`; generic status transition (including
///   `succeeded`, `failed`, `cancelled`).
/// - API: `services/orchestration-api/src/routes/task_lifecycle.rs::complete_attempt`
///   -- POST `/api/task-attempts/{id}/complete`; completes an attempt and
///   propagates the result to the parent task/node.
/// - Tick: `services/loop-runner/src/tick.rs::check_execution_completion`
///   (line ~1410) -- checks if all tasks for a cycle's objective are
///   terminal and advances the cycle from `execution` to `integration`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskCompletionCommand {
    pub idempotency_key: String,
    /// Task that completed.
    pub task_id: String,
    /// Final status.
    pub final_status: TaskStatus,
    /// Worker that executed the task.
    pub worker_id: String,
    /// Optional output artifact reference.
    pub artifact_ref: Option<String>,
    /// Completion timestamp.
    pub completed_at: DateTime<Utc>,
}

// ── CTL-011: Failure ingestion ────────────────────────────────────────────
//
// Ingests a task failure for retry/escalation decisions.

/// Failure classification for retry decisions.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum FailureKind {
    /// Transient error (network, rate limit) -- retryable.
    Transient,
    /// Logic error in the worker output -- may be retryable with repair.
    LogicError,
    /// Permanent failure -- do not retry.
    Permanent,
    /// Worker crashed or was killed.
    WorkerCrash,
    /// Output validation failed.
    ValidationFailure,
}

/// CTL-011: Failure ingestion command.
///
/// Records a task failure with classification for retry decisions.
///
/// Runtime implementations:
/// - API: `services/orchestration-api/src/routes/task_lifecycle.rs::fail_task`
///   -- POST `/api/tasks/{id}/fail`; transitions task to `failed`, updates
///   node lifecycle to `failed`, finishes the running attempt, emits
///   `task_failed` event.
/// - API: `services/orchestration-api/src/routes/task_lifecycle.rs::patch_task`
///   -- PATCH `/api/tasks/{id}` with `status: "failed"`.
/// - Tick: `services/loop-runner/src/tick.rs::check_execution_completion`
///   (line ~1410) -- failure path: when all tasks are terminal (including
///   failures), cycle advances from `execution` to `integration`.
///
/// Note: The `FailureKind` classification is defined in this module but
/// not yet consumed at runtime; the API routes treat all failures uniformly.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FailureIngestionCommand {
    pub idempotency_key: String,
    /// Task that failed.
    pub task_id: String,
    /// Worker that was executing.
    pub worker_id: String,
    /// Failure classification.
    pub failure_kind: FailureKind,
    /// Human-readable error message.
    pub error_message: String,
    /// Attempt number that failed.
    pub attempt_number: u32,
    /// Failure timestamp.
    pub failed_at: DateTime<Utc>,
}

// ── CTL-012: Timeout ingestion ────────────────────────────────────────────
//
// Ingests a task timeout for retry/escalation decisions.

/// CTL-012: Timeout ingestion command.
///
/// Records that a task timed out. The control plane will apply
/// the timeout policy to decide next action (retry, cancel, escalate).
///
/// Runtime implementations:
/// - Tick: `services/loop-runner/src/tick.rs::check_execution_completion`
///   (line ~1410) -- timeout detection: tasks that have been `running`
///   beyond their configured threshold are treated as terminal failures,
///   causing the cycle to advance.
/// - **No dedicated timeout handler yet.** Timeout-specific fields
///   (`elapsed_seconds`, `timeout_threshold_seconds`) are not populated
///   at runtime; the tick loop relies on general task-status checks.
/// - No direct API equivalent; timeouts are detected by the tick loop.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TimeoutIngestionCommand {
    pub idempotency_key: String,
    /// Task that timed out.
    pub task_id: String,
    /// Worker that was executing.
    pub worker_id: String,
    /// How long the task ran before timeout (seconds).
    pub elapsed_seconds: u64,
    /// Configured timeout threshold (seconds).
    pub timeout_threshold_seconds: u64,
    /// Attempt number that timed out.
    pub attempt_number: u32,
    /// Timeout timestamp.
    pub timed_out_at: DateTime<Utc>,
}

// ── CTL-013: Retry scheduling ─────────────────────────────────────────────
//
// Schedules a retry for a failed or timed-out task.

/// CTL-013: Retry scheduling command.
///
/// Schedules a task for retry after failure or timeout. The control
/// plane must verify the retry budget has not been exhausted.
///
/// Runtime implementations:
/// - API: `services/orchestration-api/src/routes/task_lifecycle.rs::patch_task`
///   -- PATCH `/api/tasks/{id}` with `status: "queued"` (the `failed ->
///   queued` transition is the retry path in `is_valid_task_transition`).
/// - Tick: `services/loop-runner/src/tick.rs::check_execution_completion`
///   (line ~1410) -- implicit retry: when execution completes with
///   failures, the cycle still advances; explicit retry scheduling with
///   budget checks is not yet implemented in the tick loop.
/// - **Retry budget enforcement is not yet implemented.** The
///   `default_retry_budget` field exists in policy snapshots but is not
///   checked before re-queuing.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RetrySchedulingCommand {
    pub idempotency_key: String,
    /// Task to retry.
    pub task_id: String,
    /// Next attempt number.
    pub next_attempt_number: u32,
    /// Delay before retry (milliseconds).
    pub delay_ms: u64,
    /// Reason for the retry.
    pub reason: String,
    /// Whether to reassign to a different worker.
    pub reassign_worker: bool,
}

// ── CTL-014: Drift-triggered requeueing ───────────────────────────────────
//
// Re-queues tasks when drift is detected between expected and actual state.

/// Source of a drift signal.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum DriftSource {
    /// An invariant that was previously holding is now violated.
    InvariantViolation,
    /// A dependency changed after the task was queued.
    DependencyChange,
    /// A certification was revoked.
    CertificationRevocation,
    /// External state change detected.
    ExternalChange,
    /// Manual requeue by operator.
    ManualRequeue,
}

/// CTL-014: Drift requeue command.
///
/// Re-queues a node's tasks when drift is detected. This is a
/// corrective action, not a retry -- the work needs to be redone
/// because the ground truth changed.
///
/// Runtime implementations:
/// - Tick: `services/loop-runner/src/tick.rs::detect_drift` (line ~2921)
///   -- checks if upstream nodes were modified after downstream nodes
///   were certified; marks stale certifications and emits drift events.
/// - **Requeue action is not yet implemented.** `detect_drift` emits
///   events but does not yet re-queue tasks for the affected nodes.
/// - No direct API equivalent; drift detection is tick-driven.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DriftRequeueCommand {
    pub idempotency_key: String,
    /// Node whose tasks should be requeued.
    pub node_id: String,
    /// Source of the drift signal.
    pub drift_source: DriftSource,
    /// Description of what drifted.
    pub drift_description: String,
    /// Timestamp when drift was detected.
    pub detected_at: DateTime<Utc>,
}

// ── CTL-015: Next-cycle generation ────────────────────────────────────────
//
// Generates the next cycle when the current cycle reaches NextCycleReady.

/// CTL-015: Next cycle generation command.
///
/// Instructs the control plane to generate the next cycle within a
/// loop. The current cycle must be in `NextCycleReady` phase.
///
/// Runtime implementations:
/// - Tick: `services/loop-runner/src/tick.rs::handle_next_cycle`
///   (line ~1780) -- logs cycles in `next_cycle_ready` for observability.
/// - Tick: `services/loop-runner/src/tick.rs::create_cycles_for_active_loops`
///   (line ~268) -- on the next tick, picks up loops whose latest cycle is
///   `next_cycle_ready` and auto-creates the next cycle (CTL-003).
/// - No direct API equivalent; next-cycle generation is tick-driven.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NextCycleGenerationCommand {
    pub idempotency_key: String,
    /// Loop to advance.
    pub loop_id: String,
    /// Current cycle that has reached NextCycleReady.
    pub current_cycle_id: String,
    /// Policy snapshot ID for the new cycle.
    pub next_policy_snapshot_id: String,
    /// Optional carry-forward items from the current cycle.
    pub carry_forward_node_ids: Vec<String>,
}

// ── Command handler trait ─────────────────────────────────────────────────
//
// All commands are dispatched through a single trait so the control
// plane can enforce idempotency, audit logging, and transition legality
// uniformly.

/// Trait for command execution (design contract -- no impl exists yet).
///
/// Implementors must:
/// 1. Check the idempotency key before applying state changes.
/// 2. Validate preconditions (entity existence, transition legality).
/// 3. Produce events on success.
/// 4. Never mutate projections directly -- projections are derived.
///
/// # Current runtime status
///
/// Commands are executed via direct SQL in two places:
/// - `services/loop-runner/src/tick.rs` (automated lifecycle)
/// - `services/orchestration-api/src/routes/` (user-initiated HTTP)
///
/// No struct implements this trait today.  Each method below references the
/// CTL row and the function(s) that currently provide its runtime behavior.
///
/// # Future work
///
/// Implement this trait on a struct backed by a `sqlx::Transaction`, then
/// call it from both tick.rs and the API routes.  This will centralize
/// validation, idempotency checks, and event emission.
pub trait CommandHandler {
    /// CTL-001: Create objective.
    /// Runtime: API `objectives.rs::create_objective`, Tick `create_loops_for_new_objectives`.
    fn handle_create_objective(&self, cmd: CreateObjectiveCommand) -> CommandResult;

    /// CTL-002: Create loop.
    /// Runtime: API `loops.rs::create_loop`, Tick `create_loops_for_new_objectives`.
    fn handle_create_loop(&self, cmd: CreateLoopCommand) -> CommandResult;

    /// CTL-003: Create cycle.
    /// Runtime: API `cycles.rs::create_cycle`, Tick `create_cycles_for_active_loops`.
    fn handle_create_cycle(&self, cmd: CreateCycleCommand) -> CommandResult;

    /// CTL-004: Create node from plan.
    /// Runtime: API `nodes.rs::create_node`, Tick `bridge_milestones_to_nodes`.
    fn handle_create_node_from_plan(&self, cmd: CreateNodeFromPlanCommand) -> CommandResult;

    /// CTL-005: Create task from node.
    /// Runtime: API `tasks.rs::create_task`, Tick `create_tasks_for_objective`.
    fn handle_create_task_from_node(&self, cmd: CreateTaskFromNodeCommand) -> CommandResult;

    /// CTL-006: Dispatch scheduler.
    /// Runtime: Tick `dispatch_phase` + `dispatch_queued_tasks`.
    fn handle_dispatch_scheduler(&self, cmd: DispatchSchedulerCommand) -> CommandResult;

    /// CTL-007: Phase transition.
    /// Runtime: Tick `advance_cycle_phase` (shared helper for all phase moves).
    fn handle_phase_transition(&self, cmd: PhaseTransitionCommand) -> CommandResult;

    /// CTL-008: Queue prioritization.
    /// Runtime: **Not yet implemented** -- tasks dispatch in insertion order.
    fn handle_queue_prioritization(&self, cmd: QueuePrioritizationCommand) -> CommandResult;

    /// CTL-009: Lane assignment.
    /// Runtime: Tick `apply_certification_result` (promotes lane on cert pass).
    fn handle_lane_assignment(&self, cmd: LaneAssignmentCommand) -> CommandResult;

    /// CTL-010: Task completion.
    /// Runtime: API `task_lifecycle.rs::complete_task`, `patch_task`,
    /// `complete_attempt`; Tick `check_execution_completion`.
    fn handle_task_completion(&self, cmd: TaskCompletionCommand) -> CommandResult;

    /// CTL-011: Failure ingestion.
    /// Runtime: API `task_lifecycle.rs::fail_task`, `patch_task`;
    /// Tick `check_execution_completion` (failure path).
    fn handle_failure_ingestion(&self, cmd: FailureIngestionCommand) -> CommandResult;

    /// CTL-012: Timeout ingestion.
    /// Runtime: Tick `check_execution_completion` (timeout detection).
    /// **No dedicated timeout handler yet.**
    fn handle_timeout_ingestion(&self, cmd: TimeoutIngestionCommand) -> CommandResult;

    /// CTL-013: Retry scheduling.
    /// Runtime: API `task_lifecycle.rs::patch_task` (`failed -> queued`);
    /// Tick `check_execution_completion` (implicit). **Retry budget not enforced.**
    fn handle_retry_scheduling(&self, cmd: RetrySchedulingCommand) -> CommandResult;

    /// CTL-014: Drift requeue.
    /// Runtime: Tick `detect_drift`. **Requeue action not yet implemented.**
    fn handle_drift_requeue(&self, cmd: DriftRequeueCommand) -> CommandResult;

    /// CTL-015: Next cycle generation.
    /// Runtime: Tick `handle_next_cycle` + `create_cycles_for_active_loops`.
    fn handle_next_cycle_generation(&self, cmd: NextCycleGenerationCommand) -> CommandResult;
}
