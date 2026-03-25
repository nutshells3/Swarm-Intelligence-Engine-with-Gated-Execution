//! IDE surface panel data models (IDE-001 through IDE-003, data models only).
//!
//! Goal: Data models for UI panels.
//! Caution: These are *read-only* display models. Mutations must go
//!   through control-plane commands.
//!
//! The remaining IDE items (IDE-004 through IDE-013) are UI rendering
//! concerns that depend on a specific framework; only data models are
//! defined here.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use state_model::{CyclePhase, NodeLane, NodeLifecycle, PlanGate, TaskStatus};

/// Objective intake panel data model.
///
/// Carries all fields needed to render the objective intake form
/// and its current state.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectiveIntakePanelData {
    /// Objective ID (empty if creating new).
    pub objective_id: Option<String>,
    /// Current summary text.
    pub summary: String,
    /// Current desired outcome text.
    pub desired_outcome: String,
    /// Source conversation reference.
    pub source_conversation_id: Option<String>,
    /// Success metric.
    pub success_metric: String,
    /// Constraints list.
    pub constraints: Vec<String>,
    /// Current planning status.
    pub planning_status: String,
    /// Current plan gate.
    pub plan_gate: PlanGate,
    /// Whether the form is editable (Draft/NeedsClarification).
    pub editable: bool,
    /// Validation errors on the current form state.
    pub validation_errors: Vec<String>,
}

/// Gate condition display item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GateConditionDisplay {
    pub condition_name: String,
    pub status: String,
    pub description: String,
}

/// Planning panel data model.
///
/// Aggregates planning state for display: objective, architecture
/// summary, milestone count, gate status, and open questions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanningPanelData {
    pub objective_id: String,
    pub objective_summary: String,
    /// Architecture draft summary (latest accepted).
    pub architecture_summary: Option<String>,
    /// Total milestones in the tree.
    pub milestone_count: u32,
    /// Milestones completed.
    pub milestones_completed: u32,
    /// Plan gate status.
    pub gate_status: String,
    /// Individual gate conditions for display.
    pub gate_conditions: Vec<GateConditionDisplay>,
    /// Count of unresolved blocking questions.
    pub unresolved_question_count: u32,
    /// Count of identified risks.
    pub risk_count: u32,
    /// Whether the plan is ready for execution.
    pub ready_for_execution: bool,
    pub updated_at: DateTime<Utc>,
}

/// A milestone node for tree rendering.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MilestoneTreeNodeDisplay {
    pub milestone_id: String,
    pub title: String,
    pub status: String,
    pub parent_id: Option<String>,
    pub ordering: i32,
    /// Number of acceptance criteria defined.
    pub criteria_count: u32,
    /// Number of acceptance criteria satisfied.
    pub criteria_satisfied: u32,
    /// Whether this milestone has child milestones.
    pub has_children: bool,
    /// Number of execution nodes derived from this milestone.
    pub derived_node_count: u32,
}

/// Milestone tree panel data model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MilestoneTreePanelData {
    pub objective_id: String,
    pub tree_id: String,
    pub milestones: Vec<MilestoneTreeNodeDisplay>,
    pub total_milestones: u32,
    pub completed_milestones: u32,
    pub blocked_milestones: u32,
    pub updated_at: DateTime<Utc>,
}

/// Task board panel data model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskBoardPanelData {
    pub cycle_id: Option<String>,
    pub columns: Vec<TaskBoardColumn>,
    pub total_tasks: u32,
    pub updated_at: DateTime<Utc>,
}

/// A column in the task board (one per status).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskBoardColumn {
    pub status: TaskStatus,
    pub label: String,
    pub task_count: u32,
    pub tasks: Vec<TaskBoardCardData>,
}

/// A single task card on the board.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskBoardCardData {
    pub task_id: String,
    pub node_title: String,
    pub worker_role: String,
    pub assigned_worker_id: Option<String>,
    pub attempt_number: u32,
    pub status: TaskStatus,
}

/// Branch/mainline panel data model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BranchMainlinePanelData {
    pub lanes: Vec<LaneColumnData>,
    pub total_nodes: u32,
    pub updated_at: DateTime<Utc>,
}

/// A lane column in the branch/mainline panel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaneColumnData {
    pub lane: NodeLane,
    pub label: String,
    pub node_count: u32,
    pub nodes: Vec<LaneNodeCardData>,
}

/// A single node card within a lane.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaneNodeCardData {
    pub node_id: String,
    pub title: String,
    pub lifecycle: NodeLifecycle,
    pub task_progress: String,
    pub promotion_eligible: bool,
}

/// Conflict queue panel data model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConflictQueuePanelData {
    pub conflicts: Vec<ConflictCardData>,
    pub unresolved_count: u32,
    pub updated_at: DateTime<Utc>,
}

/// A single conflict card.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConflictCardData {
    pub conflict_id: String,
    pub description: String,
    pub affected_node_ids: Vec<String>,
    pub status: String,
    pub detected_at: DateTime<Utc>,
}

/// Certification queue panel data model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CertificationQueuePanelData {
    pub items: Vec<CertificationCardData>,
    pub pending_count: u32,
    pub updated_at: DateTime<Utc>,
}

/// A single certification card.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CertificationCardData {
    pub certification_id: String,
    pub node_id: String,
    pub node_title: String,
    pub certification_kind: String,
    pub status: String,
    pub submitted_at: DateTime<Utc>,
}

/// User execution settings panel data model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionSettingsPanelData {
    pub policy_version: u32,
    pub max_concurrent_workers: u32,
    pub default_timeout_seconds: u32,
    pub default_retry_budget: u32,
    pub concurrency_limit: u32,
    pub active_override_count: u32,
    pub editable: bool,
    pub updated_at: DateTime<Utc>,
}

/// Skill/agent template panel data model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillTemplatePanelData {
    pub skill_packs: Vec<SkillPackSummary>,
    pub worker_templates: Vec<WorkerTemplateSummary>,
    pub updated_at: DateTime<Utc>,
}

/// Summary of a skill pack for the panel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkillPackSummary {
    pub skill_pack_id: String,
    pub name: String,
    pub task_kinds: Vec<String>,
    pub active: bool,
}

/// Summary of a worker template for the panel.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerTemplateSummary {
    pub template_id: String,
    pub name: String,
    pub worker_role: String,
    pub skill_pack_id: String,
    pub active: bool,
}

/// Loop-history comparison panel data model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoopHistoryComparisonPanelData {
    pub loop_id: String,
    pub cycles: Vec<CycleComparisonData>,
    pub updated_at: DateTime<Utc>,
}

/// Data for comparing two or more cycles.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CycleComparisonData {
    pub cycle_id: String,
    pub cycle_index: i32,
    pub phase: CyclePhase,
    pub tasks_dispatched: u32,
    pub tasks_completed: u32,
    pub tasks_failed: u32,
    pub nodes_promoted: u32,
    pub duration_seconds: Option<u64>,
}

/// Plan review page data model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanReviewPageData {
    pub plan_id: String,
    pub objective_summary: String,
    pub architecture_summary: String,
    pub milestone_count: u32,
    pub gate_status: String,
    pub gate_conditions: Vec<GateConditionDisplay>,
    pub unresolved_questions: Vec<UnresolvedQuestionDisplay>,
    pub risks: Vec<RiskDisplay>,
    pub updated_at: DateTime<Utc>,
}

/// An unresolved question for display.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnresolvedQuestionDisplay {
    pub question_id: String,
    pub question: String,
    pub severity: String,
    pub resolution_status: String,
}

/// A risk for display.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RiskDisplay {
    pub risk_id: String,
    pub title: String,
    pub severity: String,
    pub likelihood: String,
    pub status: String,
}

/// Architecture review page data model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArchitectureReviewPageData {
    pub draft_id: String,
    pub objective_id: String,
    pub system_boundary: String,
    pub components: Vec<ComponentDisplay>,
    pub revision: i32,
    pub status: String,
    pub invariant_count: u32,
    pub design_decisions: Vec<String>,
    pub updated_at: DateTime<Utc>,
}

/// An architecture component for display.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentDisplay {
    pub component_id: String,
    pub name: String,
    pub role: String,
    pub responsibility: String,
    pub dependency_count: u32,
}

/// Development-direction review page data model.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DevelopmentDirectionReviewPageData {
    pub objective_id: String,
    pub objective_summary: String,
    pub current_loop_id: Option<String>,
    pub current_cycle_index: Option<i32>,
    pub current_phase: Option<CyclePhase>,
    pub nodes_by_lane: Vec<LaneSummary>,
    pub overall_progress_percent: u8,
    pub active_drift_count: u32,
    pub active_conflict_count: u32,
    pub updated_at: DateTime<Utc>,
}

/// Summary of nodes in a single lane.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LaneSummary {
    pub lane: NodeLane,
    pub node_count: u32,
    pub completed_count: u32,
}
