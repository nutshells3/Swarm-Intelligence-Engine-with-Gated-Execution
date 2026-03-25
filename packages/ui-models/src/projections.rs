//! Read-model projections.
//!
//! CSV guardrail: "projection update check"
//! Goal: Expose orchestration state as queryable projections for UI
//!   panels and agent context windows.
//! Caution: Do not let projection logic become a hidden second state
//!   store. Projections are *derived* from authoritative events and
//!   commands -- they are never the source of truth.
//!
//! Every projection is a read-only struct. Mutation happens only
//! through commands in the control-plane crate.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use state_model::{CyclePhase, NodeLane, NodeLifecycle, TaskStatus};

/// A single item on the task board.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskBoardItem {
    pub task_id: String,
    pub node_id: String,
    pub title: String,
    pub worker_role: String,
    pub status: TaskStatus,
    pub assigned_worker_id: Option<String>,
    pub attempt_number: u32,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// Task board projection.
///
/// Aggregates all tasks into a board view with status counts.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskBoardProjection {
    pub tasks: Vec<TaskBoardItem>,
    pub queued_count: u32,
    pub running_count: u32,
    pub completed_count: u32,
    pub failed_count: u32,
    pub timed_out_count: u32,
    pub cancelled_count: u32,
    /// Timestamp when this projection was last computed.
    pub computed_at: DateTime<Utc>,
}

/// A single node in the graph projection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeGraphItem {
    pub node_id: String,
    pub objective_id: String,
    pub title: String,
    pub lane: NodeLane,
    pub lifecycle: NodeLifecycle,
    pub task_count: u32,
    pub completed_task_count: u32,
}

/// An edge in the node graph.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeGraphEdge {
    pub from_node_id: String,
    pub to_node_id: String,
    pub edge_kind: String,
}

/// Node graph projection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeGraphProjection {
    pub nodes: Vec<NodeGraphItem>,
    pub edges: Vec<NodeGraphEdge>,
    pub computed_at: DateTime<Utc>,
}

/// A node in the branch/mainline view.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BranchMainlineItem {
    pub node_id: String,
    pub title: String,
    pub lane: NodeLane,
    pub lifecycle: NodeLifecycle,
    /// Whether this node is a candidate for promotion.
    pub promotion_eligible: bool,
    /// Review status if the node is in MainlineCandidate.
    pub review_status: Option<String>,
}

/// Branch/mainline projection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BranchMainlineProjection {
    pub branch_nodes: Vec<BranchMainlineItem>,
    pub mainline_candidate_nodes: Vec<BranchMainlineItem>,
    pub mainline_nodes: Vec<BranchMainlineItem>,
    pub blocked_nodes: Vec<BranchMainlineItem>,
    pub computed_at: DateTime<Utc>,
}

/// A single item in the review queue.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewQueueItem {
    pub review_id: String,
    pub review_kind: String,
    pub target_ref: String,
    pub target_title: String,
    pub status: String,
    pub submitted_at: DateTime<Utc>,
}

/// Review queue projection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewQueueProjection {
    pub items: Vec<ReviewQueueItem>,
    pub pending_count: u32,
    pub in_progress_count: u32,
    pub computed_at: DateTime<Utc>,
}

/// A single item in the certification queue.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CertificationQueueItem {
    pub certification_id: String,
    pub node_id: String,
    pub node_title: String,
    pub certification_kind: String,
    pub status: String,
    pub submitted_at: DateTime<Utc>,
}

/// Certification queue projection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CertificationQueueProjection {
    pub items: Vec<CertificationQueueItem>,
    pub pending_count: u32,
    pub in_progress_count: u32,
    pub computed_at: DateTime<Utc>,
}

/// A single drift item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DriftItem {
    pub node_id: String,
    pub node_title: String,
    pub drift_source: String,
    pub drift_description: String,
    pub detected_at: DateTime<Utc>,
    /// Whether the drift has been addressed (requeued or dismissed).
    pub resolved: bool,
}

/// Drift/revalidation projection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DriftProjection {
    pub items: Vec<DriftItem>,
    pub unresolved_count: u32,
    pub computed_at: DateTime<Utc>,
}

/// A single conflict item.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConflictQueueItem {
    pub conflict_id: String,
    pub description: String,
    pub affected_node_ids: Vec<String>,
    pub status: String,
    pub detected_at: DateTime<Utc>,
}

/// Conflict queue projection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConflictQueueProjection {
    pub items: Vec<ConflictQueueItem>,
    pub unresolved_count: u32,
    pub computed_at: DateTime<Utc>,
}

/// Progress summary for a single objective.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectiveProgressItem {
    pub objective_id: String,
    pub summary: String,
    pub total_nodes: u32,
    pub completed_nodes: u32,
    pub running_nodes: u32,
    pub blocked_nodes: u32,
    pub total_tasks: u32,
    pub completed_tasks: u32,
    /// Progress percentage (0-100).
    pub progress_percent: u8,
}

/// Objective progress projection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectiveProgressProjection {
    pub objectives: Vec<ObjectiveProgressItem>,
    pub computed_at: DateTime<Utc>,
}

/// Summary of a single cycle in the loop history.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoopHistoryCycleItem {
    pub cycle_id: String,
    pub cycle_index: i32,
    pub phase: CyclePhase,
    pub tasks_dispatched: u32,
    pub tasks_completed: u32,
    pub tasks_failed: u32,
    pub nodes_promoted: u32,
    pub started_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

/// Loop history projection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LoopHistoryProjection {
    pub loop_id: String,
    pub objective_id: String,
    pub cycles: Vec<LoopHistoryCycleItem>,
    pub total_cycles: u32,
    pub computed_at: DateTime<Utc>,
}

/// A single artifact in the timeline.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactTimelineItem {
    pub artifact_id: String,
    pub artifact_kind: String,
    pub title: String,
    pub source_task_id: Option<String>,
    pub source_node_id: Option<String>,
    pub created_at: DateTime<Utc>,
}

/// Artifact timeline projection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ArtifactTimelineProjection {
    pub artifacts: Vec<ArtifactTimelineItem>,
    pub total_count: u32,
    pub computed_at: DateTime<Utc>,
}

/// Staleness status for any read-model projection.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProjectionStaleness {
    /// Projection was computed within the expected refresh interval.
    Fresh,
    /// Projection is older than the expected refresh interval but still
    /// structurally valid (no corruption, just lag).
    Stale,
    /// Projection rebuild failed; data shown is from the last successful build.
    RebuildFailed,
}

/// Check whether a projection is stale given its computed_at timestamp
/// and a maximum age threshold in seconds.
pub fn check_staleness(computed_at: DateTime<Utc>, max_age_secs: i64) -> ProjectionStaleness {
    let age = chrono::Utc::now()
        .signed_duration_since(computed_at)
        .num_seconds();
    if age <= max_age_secs {
        ProjectionStaleness::Fresh
    } else {
        ProjectionStaleness::Stale
    }
}
