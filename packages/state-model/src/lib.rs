use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use user_policy::UserPolicySnapshot;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum EventKind {
    ObjectiveCreated,
    ObjectiveUpdated,
    PlanCreated,
    PlanUpdated,
    PlanGateChanged,
    PlanArtifactsGenerated,
    PlanDecomposed,
    PlanGateEvaluated,
    PlanGateForcedOverride,
    PlanUpdatedFromExtract,
    LoopCreated,
    LoopCycleAdvanced,
    CycleCreated,
    CyclePhaseTransitioned,
    CycleCompleted,
    ExecutionCompleted,
    IntegrationVerificationFailed,
    PhaseStatusRecorded,
    TaskCreated,
    TaskStatusChanged,
    TaskAttemptStarted,
    TaskAttemptFinished,
    TaskCompleted,
    TaskFailed,
    TaskAttemptCompleted,
    TaskRetryScheduled,
    IntegrationVerificationComplete,
    WorkerRegistered,
    WorkerHeartbeatReceived,
    WorkerCompleted,
    WorkerStatusHeartbeat,
    CertificationSubmitted,
    CertificationReturned,
    CertificationCompleted,
    CertificationCandidateCreated,
    CertificationConfigUpdated,
    CertificationSettingsUpdated,
    DualFormalizationDiverged,
    ConflictCreated,
    ConflictResolved,
    ConflictAutoResolved,
    AdjudicationTaskCreated,
    FileConflictDetected,
    MergeConflictDetected,
    MainlineIntegrationAttempted,
    MainlineIntegrationCompleted,
    IntegrationVerifyNodeCreated,
    RoadmapNodeCreated,
    RoadmapNodeAbsorbed,
    RoadmapReprioritized,
    RoadmapNodeDeferred,
    RoadmapNodeRejected,
    RoadmapAbsorbed,
    RoadmapReordered,
    RoadmapTrackChanged,
    MilestoneBridged,
    ReviewArtifactCreated,
    ReviewCompleted,
    ReviewCreated,
    ReviewUpdated,
    ReviewApproved,
    ReviewAutoApproved,
    ReviewNeeded,
    SkillPackRegistered,
    WorkerTemplateCreated,
    UserPolicySnapshotSaved,
    DeploymentModeChanged,
    ChatSessionCreated,
    ChatSessionLinkedToObjective,
    ConstraintsExtracted,
    ChatMessageAdded,
    ConversationExtracted,
    BacklogDraftCreated,
    ExtractProcessed,
    DriftDetected,
    ObjectiveDriftDetected,
    DependencyDriftDetected,
    ComparisonArtifactCreated,
    LoopScoreCreated,
    MilestoneTemplatesCreated,
    DriftCheckCompleted,
    SelfPromotionBlocked,
    RecursiveReportGenerated,
    SuccessPatternRecorded,
    RoadmapSuggestionRecorded,
    WorktreeBound,
    WorktreeReleased,
    DirtyWorktreeDetected,
    TickHeartbeat,
    RetentionPolicyEnforced,
    ProjectionSnapshot,
    PeerMessageSent,
    PeerMessageAcknowledged,
    NodeEdgeCreated,
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
