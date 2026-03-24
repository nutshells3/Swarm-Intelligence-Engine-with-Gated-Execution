use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use user_policy::UserPolicySnapshot;

// ── Event types ─────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    // Objective events (EVT-001)
    ObjectiveCreated,
    ObjectiveUpdated,
    // Plan events (EVT-002)
    PlanCreated,
    PlanUpdated,
    PlanGateChanged,
    // Loop events (EVT-003)
    LoopCreated,
    LoopCycleAdvanced,
    // Cycle events (EVT-004)
    CycleCreated,
    CyclePhaseTransitioned,
    CycleCompleted,
    // Task events (EVT-005)
    TaskCreated,
    TaskStatusChanged,
    TaskAttemptStarted,
    TaskAttemptFinished,
    // Worker events (EVT-006)
    WorkerRegistered,
    WorkerHeartbeatReceived,
    WorkerCompleted,
    // Certification events (EVT-007, EVT-008)
    CertificationSubmitted,
    CertificationReturned,
    // Conflict events (EVT-009)
    ConflictCreated,
    ConflictResolved,
    // Mainline events (EVT-010)
    MainlineIntegrationAttempted,
    MainlineIntegrationCompleted,
    // Roadmap events
    RoadmapNodeCreated,
    RoadmapNodeAbsorbed,
    RoadmapReprioritized,
    // Review events
    ReviewArtifactCreated,
    ReviewCompleted,
    // Skill events
    SkillPackRegistered,
    WorkerTemplateCreated,
    // Policy events
    UserPolicySnapshotSaved,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EventRecord {
    pub event_id: String,
    pub aggregate_kind: String,
    pub aggregate_id: String,
    pub event_kind: EventKind,
    pub idempotency_key: String,
    pub payload: serde_json::Value,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PlanGate {
    Draft,
    NeedsClarification,
    ReadyForExecution,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeLane {
    Branch,
    MainlineCandidate,
    Mainline,
    Blocked,
    Archived,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum NodeLifecycle {
    Proposed,
    Queued,
    Running,
    ReviewNeeded,
    CertificationNeeded,
    Admitted,
    Blocked,
    Superseded,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
    ReviewNeeded,
    Cancelled,
    TimedOut,
    Archived,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CyclePhase {
    Intake,
    ConversationExtraction,
    PlanElaboration,
    PlanValidation,
    Review,
    Decomposition,
    Dispatch,
    Execution,
    Integration,
    CertificationSelection,
    Certification,
    StateUpdate,
    NextCycleReady,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectiveRecord {
    pub objective_id: String,
    pub summary: String,
    pub planning_status: String,
    pub plan_gate: PlanGate,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanRecord {
    pub plan_id: String,
    pub objective_id: String,
    pub architecture_summary: String,
    pub milestone_tree_ref: String,
    pub unresolved_questions: i32,
    pub plan_gate: PlanGate,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoopRecord {
    pub loop_id: String,
    pub objective_id: String,
    pub cycle_index: i32,
    pub active_track: String,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CycleRecord {
    pub cycle_id: String,
    pub loop_id: String,
    pub phase: CyclePhase,
    pub policy_snapshot: UserPolicySnapshot,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeRecord {
    pub node_id: String,
    pub objective_id: String,
    pub title: String,
    pub statement: String,
    pub lane: NodeLane,
    pub lifecycle: NodeLifecycle,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskRecord {
    pub task_id: String,
    pub node_id: String,
    pub worker_role: String,
    pub skill_pack_id: String,
    pub status: TaskStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewArtifact {
    pub review_id: String,
    pub review_kind: String,
    pub target_ref: String,
    pub status: String,
    pub approval_effect: String,
    pub created_at: DateTime<Utc>,
}
