//! Read-model projection endpoints (Tier 3).
//!
//! Each projection is a SQL query that joins relevant tables and returns
//! a shaped JSON response. Projections are derived views -- never the
//! source of truth.
//!
//! ## Canonical types (CD-04)
//!
//! The canonical projection types are defined in `packages/ui-models/src/projections.rs`.
//! The orchestration-api handlers below return shapes that are wire-compatible
//! with the ui-models types but use flat strings instead of typed enums for
//! direct SQL result mapping. The divergences are documented on each struct.
//!
//! When the codebase moves to full runtime decoding, these handlers should
//! import and return the ui-models types directly (RDM-001 through RDM-010).

use axum::extract::State;
use axum::response::Json;
use serde::Serialize;
use sqlx::Row;

use crate::error::{ApiResult, internal_error};
use crate::state::AppState;

// Canonical projection types from ui-models (CD-04).
// These are the authoritative shapes. Handlers build the canonical type first
// from SQL rows, then convert to the API wire type via `From` impls.
use ui_models::projections::{
    TaskBoardProjection as CanonicalTaskBoardProjection,
    TaskBoardItem as CanonicalTaskBoardItem,
    NodeGraphProjection as CanonicalNodeGraphProjection,
    NodeGraphItem as CanonicalNodeGraphItem,
    NodeGraphEdge as CanonicalNodeGraphEdge,
    BranchMainlineProjection as CanonicalBranchMainlineProjection,
    BranchMainlineItem as CanonicalBranchMainlineItem,
    ReviewQueueItem as CanonicalReviewQueueItem,
    CertificationQueueProjection as CanonicalCertificationQueueProjection,
    CertificationQueueItem as CanonicalCertificationQueueItem,
    ObjectiveProgressProjection as CanonicalObjectiveProgressProjection,
    ObjectiveProgressItem as CanonicalObjectiveProgressItem,
    DriftProjection as CanonicalDriftProjection,
    DriftItem as CanonicalDriftItem,
    LoopHistoryCycleItem as CanonicalLoopHistoryCycleItem,
    ArtifactTimelineProjection as CanonicalArtifactTimelineProjection,
    ArtifactTimelineItem as CanonicalArtifactTimelineItem,
};
use state_model::{CyclePhase, NodeLane, NodeLifecycle, TaskStatus};

/// A single task item for the board view.
///
/// **Canonical equivalent (CD-04):** `ui_models::projections::TaskBoardItem` (RDM-001).
///
/// **Mapping:** The canonical type uses `TaskStatus` (typed enum) for `status`
/// and `DateTime<Utc>` for timestamps. This API type flattens both to `String`
/// for direct JSON wire format -- `status` is the enum variant name,
/// `created_at`/`updated_at` are RFC 3339 strings. The canonical type also
/// includes `title`, `assigned_worker_id`, and `attempt_number` which this
/// API type omits.
///
/// **Future:** Generate from `ui_models::TaskBoardItem`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct TaskBoardItem {
    pub task_id: String,
    pub node_id: String,
    pub worker_role: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
}

/// Summary counts for the task board.
///
/// The canonical `ui_models::TaskBoardProjection` embeds equivalent counts
/// as top-level fields (`queued_count`, `running_count`, etc.).
#[derive(Serialize, utoipa::ToSchema)]
pub struct TaskBoardSummary {
    pub queued: i64,
    pub running: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub total: i64,
}

/// Task board projection (RDM-001).
///
/// **Canonical equivalent (CD-04):** `ui_models::projections::TaskBoardProjection`.
///
/// **Mapping:** The canonical type uses a flat `Vec<TaskBoardItem>` plus
/// per-status counts. This API type pre-buckets items into `queued`,
/// `running`, `succeeded`, `failed` arrays for frontend convenience.
/// The canonical type also carries `timed_out_count`, `cancelled_count`,
/// and `computed_at` which this API type omits (response is always live).
#[derive(Serialize, utoipa::ToSchema)]
pub struct TaskBoardProjection {
    pub queued: Vec<TaskBoardItem>,
    pub running: Vec<TaskBoardItem>,
    pub succeeded: Vec<TaskBoardItem>,
    pub failed: Vec<TaskBoardItem>,
    pub summary: TaskBoardSummary,
}

impl From<CanonicalTaskBoardItem> for TaskBoardItem {
    fn from(c: CanonicalTaskBoardItem) -> Self {
        Self {
            task_id: c.task_id,
            node_id: c.node_id,
            worker_role: c.worker_role,
            status: serde_json::to_value(&c.status)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| format!("{:?}", c.status)),
            created_at: c.created_at.to_rfc3339(),
            updated_at: c.updated_at.to_rfc3339(),
        }
    }
}

impl From<CanonicalTaskBoardProjection> for TaskBoardProjection {
    fn from(c: CanonicalTaskBoardProjection) -> Self {
        let mut queued = Vec::new();
        let mut running = Vec::new();
        let mut succeeded = Vec::new();
        let mut failed = Vec::new();

        for item in c.tasks {
            let status = item.status;
            let api_item = TaskBoardItem {
                task_id: item.task_id,
                node_id: item.node_id,
                worker_role: item.worker_role,
                status: serde_json::to_value(&status)
                    .ok()
                    .and_then(|v| v.as_str().map(String::from))
                    .unwrap_or_else(|| format!("{:?}", status)),
                created_at: item.created_at.to_rfc3339(),
                updated_at: item.updated_at.to_rfc3339(),
            };
            match status {
                TaskStatus::Queued => queued.push(api_item),
                TaskStatus::Running => running.push(api_item),
                TaskStatus::Succeeded => succeeded.push(api_item),
                TaskStatus::Failed => failed.push(api_item),
                _ => {} // timed_out, cancelled, etc. omitted from board buckets
            }
        }

        let summary = TaskBoardSummary {
            queued: queued.len() as i64,
            running: running.len() as i64,
            succeeded: succeeded.len() as i64,
            failed: failed.len() as i64,
            total: (queued.len() + running.len() + succeeded.len() + failed.len()) as i64,
        };

        Self { queued, running, succeeded, failed, summary }
    }
}

/// GET /api/projections/task-board
///
/// Returns the full task board projection with items bucketed by status.
/// Canonical type: `ui_models::projections::TaskBoardProjection` (RDM-001).
///
/// Pipeline: SQL rows -> CanonicalTaskBoardProjection -> API TaskBoardProjection.
#[utoipa::path(
    get,
    path = "/api/projections/task-board",
    responses(
        (status = 200, description = "Task board projection", body = TaskBoardProjection)
    )
)]
pub async fn task_board(
    State(state): State<AppState>,
) -> ApiResult<TaskBoardProjection> {
    let rows = sqlx::query(
        r#"SELECT task_id, node_id, title, worker_role, status,
                  assigned_worker_id, attempt_number, created_at, updated_at
           FROM tasks
           ORDER BY created_at DESC
           LIMIT 500"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    // Step 1: Build canonical items (ui-models types with typed enums).
    let mut canonical_tasks = Vec::with_capacity(rows.len());
    let mut queued_count: u32 = 0;
    let mut running_count: u32 = 0;
    let mut completed_count: u32 = 0;
    let mut failed_count: u32 = 0;
    let mut timed_out_count: u32 = 0;
    let mut cancelled_count: u32 = 0;

    for row in &rows {
        let status_str: String = row.try_get("status").map_err(internal_error)?;
        let status: TaskStatus = serde_json::from_value(
            serde_json::Value::String(status_str),
        )
        .unwrap_or(TaskStatus::Queued);

        match status {
            TaskStatus::Queued => queued_count += 1,
            TaskStatus::Running => running_count += 1,
            TaskStatus::Succeeded => completed_count += 1,
            TaskStatus::Failed => failed_count += 1,
            TaskStatus::TimedOut => timed_out_count += 1,
            TaskStatus::Cancelled => cancelled_count += 1,
            _ => {}
        }

        canonical_tasks.push(CanonicalTaskBoardItem {
            task_id: row.try_get("task_id").map_err(internal_error)?,
            node_id: row.try_get("node_id").map_err(internal_error)?,
            title: row.try_get("title").map_err(internal_error)?,
            worker_role: row.try_get("worker_role").map_err(internal_error)?,
            status,
            assigned_worker_id: row.try_get("assigned_worker_id").map_err(internal_error)?,
            attempt_number: row
                .try_get::<i32, _>("attempt_number")
                .map_err(internal_error)? as u32,
            created_at: row.try_get("created_at").map_err(internal_error)?,
            updated_at: row.try_get("updated_at").map_err(internal_error)?,
        });
    }

    let canonical = CanonicalTaskBoardProjection {
        tasks: canonical_tasks,
        queued_count,
        running_count,
        completed_count,
        failed_count,
        timed_out_count,
        cancelled_count,
        computed_at: chrono::Utc::now(),
    };

    // Step 2: Convert canonical -> API wire type.
    Ok(Json(TaskBoardProjection::from(canonical)))
}

// Canonical type: ui_models::projections::NodeGraphProjection.
// Divergence: this handler uses `String` for lane/lifecycle instead of
// the `NodeLane`/`NodeLifecycle` enums from state-model, and omits
// `task_count`/`completed_task_count` (kept simple for React Flow).

/// A node in the graph view.
///
/// Canonical equivalent: `ui_models::projections::NodeGraphItem`.
/// Uses `String` fields for lane/lifecycle for direct SQL mapping.
#[derive(Serialize, utoipa::ToSchema)]
pub struct GraphNode {
    pub id: String,
    pub label: String,
    pub lane: String,
    pub lifecycle: String,
    pub objective_id: String,
}

/// An edge in the graph view.
///
/// Canonical equivalent: `ui_models::projections::NodeGraphEdge`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct GraphEdge {
    pub source: String,
    pub target: String,
    pub kind: String,
}

/// Node graph projection (RDM-002).
///
/// Canonical equivalent: `ui_models::projections::NodeGraphProjection`.
/// This handler omits `computed_at` since the response is always live.
#[derive(Serialize, utoipa::ToSchema)]
pub struct NodeGraphProjection {
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
}

impl From<CanonicalNodeGraphItem> for GraphNode {
    fn from(c: CanonicalNodeGraphItem) -> Self {
        Self {
            id: c.node_id,
            label: c.title,
            lane: serde_json::to_value(&c.lane)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| format!("{:?}", c.lane)),
            lifecycle: serde_json::to_value(&c.lifecycle)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| format!("{:?}", c.lifecycle)),
            objective_id: c.objective_id,
        }
    }
}

impl From<CanonicalNodeGraphEdge> for GraphEdge {
    fn from(c: CanonicalNodeGraphEdge) -> Self {
        Self {
            source: c.from_node_id,
            target: c.to_node_id,
            kind: c.edge_kind,
        }
    }
}

impl From<CanonicalNodeGraphProjection> for NodeGraphProjection {
    fn from(c: CanonicalNodeGraphProjection) -> Self {
        Self {
            nodes: c.nodes.into_iter().map(GraphNode::from).collect(),
            edges: c.edges.into_iter().map(GraphEdge::from).collect(),
        }
    }
}

/// GET /api/projections/node-graph
///
/// Returns the node dependency graph for React Flow visualization.
/// Canonical type: `ui_models::projections::NodeGraphProjection` (RDM-002).
///
/// Pipeline: SQL rows -> CanonicalNodeGraphProjection -> API NodeGraphProjection.
#[utoipa::path(
    get,
    path = "/api/projections/node-graph",
    responses(
        (status = 200, description = "Node graph projection", body = NodeGraphProjection)
    )
)]
pub async fn node_graph(
    State(state): State<AppState>,
) -> ApiResult<NodeGraphProjection> {
    let node_rows = sqlx::query(
        r#"SELECT node_id, title, lane, lifecycle, objective_id
           FROM nodes
           ORDER BY created_at"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let edge_rows = sqlx::query(
        r#"SELECT from_node_id, to_node_id, edge_kind
           FROM node_edges"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    // Step 1: Build canonical types.
    let mut canonical_nodes = Vec::with_capacity(node_rows.len());
    for row in &node_rows {
        let lane_str: String = row.try_get("lane").map_err(internal_error)?;
        let lifecycle_str: String = row.try_get("lifecycle").map_err(internal_error)?;

        canonical_nodes.push(CanonicalNodeGraphItem {
            node_id: row.try_get("node_id").map_err(internal_error)?,
            objective_id: row.try_get("objective_id").map_err(internal_error)?,
            title: row.try_get("title").map_err(internal_error)?,
            lane: serde_json::from_value(serde_json::Value::String(lane_str))
                .unwrap_or(NodeLane::Branch),
            lifecycle: serde_json::from_value(serde_json::Value::String(lifecycle_str))
                .unwrap_or(NodeLifecycle::Proposed),
            task_count: 0,
            completed_task_count: 0,
        });
    }

    let mut canonical_edges = Vec::with_capacity(edge_rows.len());
    for row in &edge_rows {
        canonical_edges.push(CanonicalNodeGraphEdge {
            from_node_id: row.try_get("from_node_id").map_err(internal_error)?,
            to_node_id: row.try_get("to_node_id").map_err(internal_error)?,
            edge_kind: row.try_get("edge_kind").map_err(internal_error)?,
        });
    }

    let canonical = CanonicalNodeGraphProjection {
        nodes: canonical_nodes,
        edges: canonical_edges,
        computed_at: chrono::Utc::now(),
    };

    // Step 2: Convert canonical -> API wire type.
    Ok(Json(NodeGraphProjection::from(canonical)))
}

/// A node in the branch/mainline view.
///
/// **Canonical equivalent (CD-04):** `ui_models::projections::BranchMainlineItem` (RDM-003).
///
/// **Mapping:** The canonical type uses `NodeLane` and `NodeLifecycle` typed
/// enums; this API type flattens both to `String` for JSON wire format. The
/// canonical type also includes `promotion_eligible` and `review_status`
/// which this API type omits.
///
/// **Future:** Generate from `ui_models::BranchMainlineItem`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct BranchMainlineItem {
    pub node_id: String,
    pub title: String,
    pub lane: String,
    pub lifecycle: String,
}

/// Branch/mainline projection (RDM-003).
///
/// **Canonical equivalent (CD-04):** `ui_models::projections::BranchMainlineProjection`.
///
/// **Mapping:** The canonical type uses `branch_nodes`, `mainline_candidate_nodes`,
/// `mainline_nodes`, `blocked_nodes` as field names and includes `computed_at`.
/// This API type uses shortened names (`branch`, `mainline_candidate`, etc.)
/// and omits `computed_at` (response is always live).
#[derive(Serialize, utoipa::ToSchema)]
pub struct BranchMainlineProjection {
    pub branch: Vec<BranchMainlineItem>,
    pub mainline_candidate: Vec<BranchMainlineItem>,
    pub mainline: Vec<BranchMainlineItem>,
    pub blocked: Vec<BranchMainlineItem>,
}

impl From<CanonicalBranchMainlineItem> for BranchMainlineItem {
    fn from(c: CanonicalBranchMainlineItem) -> Self {
        Self {
            node_id: c.node_id,
            title: c.title,
            lane: serde_json::to_value(&c.lane)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| format!("{:?}", c.lane)),
            lifecycle: serde_json::to_value(&c.lifecycle)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| format!("{:?}", c.lifecycle)),
        }
    }
}

impl From<CanonicalBranchMainlineProjection> for BranchMainlineProjection {
    fn from(c: CanonicalBranchMainlineProjection) -> Self {
        Self {
            branch: c.branch_nodes.into_iter().map(BranchMainlineItem::from).collect(),
            mainline_candidate: c
                .mainline_candidate_nodes
                .into_iter()
                .map(BranchMainlineItem::from)
                .collect(),
            mainline: c.mainline_nodes.into_iter().map(BranchMainlineItem::from).collect(),
            blocked: c.blocked_nodes.into_iter().map(BranchMainlineItem::from).collect(),
        }
    }
}

/// GET /api/projections/branch-mainline
///
/// Returns nodes grouped by lane for the branch/mainline panel.
/// Canonical type: `ui_models::projections::BranchMainlineProjection` (RDM-003).
///
/// Pipeline: SQL rows -> CanonicalBranchMainlineProjection -> API BranchMainlineProjection.
#[utoipa::path(
    get,
    path = "/api/projections/branch-mainline",
    responses(
        (status = 200, description = "Branch/mainline projection", body = BranchMainlineProjection)
    )
)]
pub async fn branch_mainline(
    State(state): State<AppState>,
) -> ApiResult<BranchMainlineProjection> {
    let rows = sqlx::query(
        r#"SELECT node_id, title, lane, lifecycle
           FROM nodes
           ORDER BY created_at"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    // Step 1: Build canonical items and bucket them.
    let mut branch_nodes = Vec::new();
    let mut mainline_candidate_nodes = Vec::new();
    let mut mainline_nodes = Vec::new();
    let mut blocked_nodes = Vec::new();

    for row in &rows {
        let lane_str: String = row.try_get("lane").map_err(internal_error)?;
        let lifecycle_str: String = row.try_get("lifecycle").map_err(internal_error)?;
        let lane: NodeLane = serde_json::from_value(serde_json::Value::String(lane_str))
            .unwrap_or(NodeLane::Branch);
        let lifecycle: NodeLifecycle =
            serde_json::from_value(serde_json::Value::String(lifecycle_str))
                .unwrap_or(NodeLifecycle::Proposed);

        let item = CanonicalBranchMainlineItem {
            node_id: row.try_get("node_id").map_err(internal_error)?,
            title: row.try_get("title").map_err(internal_error)?,
            lane,
            lifecycle,
            promotion_eligible: false,
            review_status: None,
        };

        match lane {
            NodeLane::Mainline => mainline_nodes.push(item),
            NodeLane::MainlineCandidate => mainline_candidate_nodes.push(item),
            NodeLane::Blocked => blocked_nodes.push(item),
            _ => {
                if lifecycle == NodeLifecycle::Blocked {
                    blocked_nodes.push(item);
                } else {
                    branch_nodes.push(item);
                }
            }
        }
    }

    let canonical = CanonicalBranchMainlineProjection {
        branch_nodes,
        mainline_candidate_nodes,
        mainline_nodes,
        blocked_nodes,
        computed_at: chrono::Utc::now(),
    };

    // Step 2: Convert canonical -> API wire type.
    Ok(Json(BranchMainlineProjection::from(canonical)))
}

/// A node awaiting review (from node lifecycle state).
///
/// **Canonical equivalent (CD-04):** `ui_models::projections::ReviewQueueItem` (RDM-004).
///
/// **Mapping:** The canonical type has `review_id`, `review_kind`, `target_ref`,
/// `target_title`, `status`, and `submitted_at: DateTime<Utc>`. This API type
/// represents node-level review state (node_id, lifecycle, lane) rather than
/// review-artifact-level data. The `PendingReviewItem` below carries the
/// artifact-level fields that are closer to the canonical shape.
///
/// **Future:** Unify into a single item type generated from `ui_models::ReviewQueueItem`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ReviewQueueItem {
    pub node_id: String,
    pub title: String,
    pub lifecycle: String,
    pub lane: String,
}

/// A pending review artifact entry in the queue.
///
/// This is the artifact-level complement to `ReviewQueueItem` above.
/// Closer to the canonical `ui_models::projections::ReviewQueueItem` shape
/// but uses `String` for timestamps (RFC 3339 wire format) instead of
/// `DateTime<Utc>`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct PendingReviewItem {
    pub review_id: String,
    pub review_kind: String,
    pub target_ref: String,
    pub status: String,
    pub recorded_at: String,
}

/// Review queue projection (RDM-004).
///
/// **Canonical equivalent (CD-04):** `ui_models::projections::ReviewQueueProjection`.
///
/// **Mapping:** The canonical type has a single `items: Vec<ReviewQueueItem>`
/// plus `pending_count`, `in_progress_count`, and `computed_at`. This API
/// type splits into `items` (node-lifecycle) and `pending_reviews`
/// (review-artifacts) for REV-015 dual-source display.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ReviewQueueProjection {
    pub items: Vec<ReviewQueueItem>,
    pub pending_reviews: Vec<PendingReviewItem>,
    pub pending_count: i64,
    pub review_artifact_count: i64,
}

impl From<CanonicalReviewQueueItem> for PendingReviewItem {
    fn from(c: CanonicalReviewQueueItem) -> Self {
        Self {
            review_id: c.review_id,
            review_kind: c.review_kind,
            target_ref: c.target_ref,
            status: c.status,
            recorded_at: c.submitted_at.to_rfc3339(),
        }
    }
}

/// GET /api/projections/review-queue
///
/// Dedicated review queue projection that combines both
/// node lifecycle state and pending review artifacts.
/// Canonical type: `ui_models::projections::ReviewQueueProjection` (RDM-004).
///
/// Pipeline: SQL rows -> CanonicalReviewQueueItem (for artifacts) -> API types.
#[utoipa::path(
    get,
    path = "/api/projections/review-queue",
    responses(
        (status = 200, description = "Review queue projection", body = ReviewQueueProjection)
    )
)]
pub async fn review_queue(
    State(state): State<AppState>,
) -> ApiResult<ReviewQueueProjection> {
    // Node-level review queue (from node lifecycle)
    let rows = sqlx::query(
        r#"SELECT node_id, title, lifecycle, lane
           FROM nodes
           WHERE lifecycle IN ('review_pending', 'in_review')
           ORDER BY created_at"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut items = Vec::with_capacity(rows.len());
    for row in &rows {
        items.push(ReviewQueueItem {
            node_id: row.try_get("node_id").map_err(internal_error)?,
            title: row.try_get("title").map_err(internal_error)?,
            lifecycle: row.try_get("lifecycle").map_err(internal_error)?,
            lane: row.try_get("lane").map_err(internal_error)?,
        });
    }

    // Also include scheduled/in-progress review artifacts from review_artifacts table.
    // Build canonical CanonicalReviewQueueItem first, then convert to API type.
    let review_rows = sqlx::query(
        r#"SELECT review_id, review_kind, target_ref, status, recorded_at
           FROM review_artifacts
           WHERE status IN ('scheduled', 'in_progress')
           ORDER BY recorded_at ASC
           LIMIT 200"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut canonical_review_items = Vec::with_capacity(review_rows.len());
    for row in &review_rows {
        canonical_review_items.push(CanonicalReviewQueueItem {
            review_id: row.try_get("review_id").map_err(internal_error)?,
            review_kind: row.try_get("review_kind").map_err(internal_error)?,
            target_ref: row.try_get("target_ref").map_err(internal_error)?,
            target_title: String::new(), // not available in this query
            status: row.try_get("status").map_err(internal_error)?,
            submitted_at: row.try_get("recorded_at").map_err(internal_error)?,
        });
    }

    // Convert canonical -> API wire types.
    let pending_reviews: Vec<PendingReviewItem> = canonical_review_items
        .into_iter()
        .map(PendingReviewItem::from)
        .collect();

    let pending_count = items.len() as i64;
    let review_artifact_count = pending_reviews.len() as i64;

    Ok(Json(ReviewQueueProjection {
        items,
        pending_reviews,
        pending_count,
        review_artifact_count,
    }))
}

/// A node awaiting certification.
///
/// **Canonical equivalent (CD-04):** `ui_models::projections::CertificationQueueItem` (RDM-005).
///
/// **Mapping:** The canonical type has `certification_id`, `node_id`,
/// `node_title`, `certification_kind`, `status`, and `submitted_at:
/// DateTime<Utc>`. This API type represents node-level certification state
/// (node_id, title, lifecycle, lane) from the nodes table rather than the
/// certification_submissions table.
///
/// **Future:** Generate from `ui_models::CertificationQueueItem`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct CertificationQueueItem {
    pub node_id: String,
    pub title: String,
    pub lifecycle: String,
    pub lane: String,
}

/// Certification queue projection (RDM-005).
///
/// **Canonical equivalent (CD-04):** `ui_models::projections::CertificationQueueProjection`.
///
/// **Mapping:** The canonical type carries `Vec<CertificationQueueItem>`,
/// `pending_count`, `in_progress_count`, and `computed_at`. This API type
/// omits `in_progress_count` and `computed_at`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct CertificationQueueProjection {
    pub items: Vec<CertificationQueueItem>,
    pub pending_count: i64,
}

impl From<CanonicalCertificationQueueProjection> for CertificationQueueProjection {
    fn from(c: CanonicalCertificationQueueProjection) -> Self {
        let items: Vec<CertificationQueueItem> = c
            .items
            .into_iter()
            .map(|ci| CertificationQueueItem {
                node_id: ci.node_id,
                title: ci.node_title,
                lifecycle: ci.status,
                lane: ci.certification_kind,
            })
            .collect();
        let pending_count = items.len() as i64;
        Self { items, pending_count }
    }
}

/// GET /api/projections/certification-queue
///
/// Returns nodes in certification-pending or certifying lifecycle states.
/// Canonical type: `ui_models::projections::CertificationQueueProjection` (RDM-005).
///
/// Pipeline: SQL rows -> CanonicalCertificationQueueItem -> API CertificationQueueItem.
#[utoipa::path(
    get,
    path = "/api/projections/certification-queue",
    responses(
        (status = 200, description = "Certification queue projection", body = CertificationQueueProjection)
    )
)]
pub async fn certification_queue(
    State(state): State<AppState>,
) -> ApiResult<CertificationQueueProjection> {
    let rows = sqlx::query(
        r#"SELECT node_id, title, lifecycle, lane
           FROM nodes
           WHERE lifecycle IN ('certification_pending', 'certifying')
           ORDER BY created_at"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    // Step 1: Build canonical items.
    let mut canonical_items = Vec::with_capacity(rows.len());
    for row in &rows {
        canonical_items.push(CanonicalCertificationQueueItem {
            certification_id: String::new(), // not available from nodes table
            node_id: row.try_get("node_id").map_err(internal_error)?,
            node_title: row.try_get("title").map_err(internal_error)?,
            certification_kind: row.try_get("lane").map_err(internal_error)?,
            status: row.try_get("lifecycle").map_err(internal_error)?,
            submitted_at: chrono::Utc::now(), // not available from nodes table
        });
    }

    let pending_count = canonical_items.len() as u32;
    let canonical = CanonicalCertificationQueueProjection {
        items: canonical_items,
        pending_count,
        in_progress_count: 0,
        computed_at: chrono::Utc::now(),
    };

    // Step 2: Convert canonical -> API wire type.
    Ok(Json(CertificationQueueProjection::from(canonical)))
}

/// Progress summary for a single objective.
///
/// **Canonical equivalent (CD-04):** `ui_models::projections::ObjectiveProgressItem` (RDM-008).
///
/// **Mapping:** Uses `i64` for counts (SQL `COUNT(*)` return type) and `f64`
/// for `progress_percent` instead of the canonical `u32`/`u8`. Also omits
/// `running_nodes` which the canonical type includes.
///
/// **Future:** Generate from `ui_models::ObjectiveProgressItem`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ObjectiveProgressItem {
    pub objective_id: String,
    pub summary: String,
    pub total_nodes: i64,
    pub completed_nodes: i64,
    pub blocked_nodes: i64,
    pub total_tasks: i64,
    pub completed_tasks: i64,
    pub progress_percent: f64,
}

/// Objective progress projection (RDM-008).
///
/// **Canonical equivalent (CD-04):** `ui_models::projections::ObjectiveProgressProjection`.
///
/// **Mapping:** Omits `computed_at` since the response is always live.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ObjectiveProgressProjection {
    pub objectives: Vec<ObjectiveProgressItem>,
}

impl From<CanonicalObjectiveProgressItem> for ObjectiveProgressItem {
    fn from(c: CanonicalObjectiveProgressItem) -> Self {
        Self {
            objective_id: c.objective_id,
            summary: c.summary,
            total_nodes: c.total_nodes as i64,
            completed_nodes: c.completed_nodes as i64,
            blocked_nodes: c.blocked_nodes as i64,
            total_tasks: c.total_tasks as i64,
            completed_tasks: c.completed_tasks as i64,
            progress_percent: c.progress_percent as f64,
        }
    }
}

impl From<CanonicalObjectiveProgressProjection> for ObjectiveProgressProjection {
    fn from(c: CanonicalObjectiveProgressProjection) -> Self {
        Self {
            objectives: c.objectives.into_iter().map(ObjectiveProgressItem::from).collect(),
        }
    }
}

/// GET /api/projections/objective-progress
///
/// Returns progress for all objectives.
/// Canonical type: `ui_models::projections::ObjectiveProgressProjection` (RDM-008).
///
/// Pipeline: SQL rows -> CanonicalObjectiveProgressProjection -> API ObjectiveProgressProjection.
#[utoipa::path(
    get,
    path = "/api/projections/objective-progress",
    responses(
        (status = 200, description = "Objective progress projection", body = ObjectiveProgressProjection)
    )
)]
pub async fn objective_progress(
    State(state): State<AppState>,
) -> ApiResult<ObjectiveProgressProjection> {
    let rows = sqlx::query(
        r#"SELECT
             o.objective_id,
             o.summary,
             COUNT(DISTINCT n.node_id) as total_nodes,
             COUNT(DISTINCT n.node_id) FILTER (WHERE n.lifecycle = 'completed') as completed_nodes,
             COUNT(DISTINCT n.node_id) FILTER (WHERE n.lifecycle = 'running') as running_nodes,
             COUNT(DISTINCT n.node_id) FILTER (WHERE n.lifecycle = 'blocked') as blocked_nodes,
             COUNT(t.task_id) as total_tasks,
             COUNT(t.task_id) FILTER (WHERE t.status = 'succeeded') as completed_tasks
           FROM objectives o
           LEFT JOIN nodes n ON o.objective_id = n.objective_id
           LEFT JOIN tasks t ON n.node_id = t.node_id
           GROUP BY o.objective_id, o.summary
           ORDER BY o.created_at DESC"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    // Step 1: Build canonical items.
    let mut canonical_objectives = Vec::with_capacity(rows.len());
    for row in &rows {
        let total_nodes: i64 = row.try_get("total_nodes").map_err(internal_error)?;
        let completed_nodes: i64 = row.try_get("completed_nodes").map_err(internal_error)?;
        let progress_percent = if total_nodes > 0 {
            ((completed_nodes as f64 / total_nodes as f64) * 100.0) as u8
        } else {
            0
        };

        canonical_objectives.push(CanonicalObjectiveProgressItem {
            objective_id: row.try_get("objective_id").map_err(internal_error)?,
            summary: row.try_get("summary").map_err(internal_error)?,
            total_nodes: total_nodes as u32,
            completed_nodes: completed_nodes as u32,
            running_nodes: row
                .try_get::<i64, _>("running_nodes")
                .map_err(internal_error)? as u32,
            blocked_nodes: row
                .try_get::<i64, _>("blocked_nodes")
                .map_err(internal_error)? as u32,
            total_tasks: row
                .try_get::<i64, _>("total_tasks")
                .map_err(internal_error)? as u32,
            completed_tasks: row
                .try_get::<i64, _>("completed_tasks")
                .map_err(internal_error)? as u32,
            progress_percent,
        });
    }

    let canonical = CanonicalObjectiveProgressProjection {
        objectives: canonical_objectives,
        computed_at: chrono::Utc::now(),
    };

    // Step 2: Convert canonical -> API wire type.
    Ok(Json(ObjectiveProgressProjection::from(canonical)))
}

/// A single drift item.
///
/// **Canonical equivalent (CD-04):** `ui_models::projections::DriftItem` (RDM-006).
///
/// **Mapping:** The canonical type uses `DateTime<Utc>` for `detected_at`;
/// this API type uses RFC 3339 `String`. Field names and semantics are
/// otherwise 1:1 with the canonical type.
///
/// **Future:** Generate from `ui_models::DriftItem`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct DriftItem {
    pub drift_id: String,
    pub node_id: String,
    pub node_title: String,
    pub drift_source: String,
    pub drift_description: String,
    pub detected_at: String,
    pub resolved: bool,
}

/// Drift/revalidation projection (RDM-006).
///
/// **Canonical equivalent (CD-04):** `ui_models::projections::DriftProjection`.
///
/// **Mapping:** The canonical type includes `computed_at: DateTime<Utc>` and
/// `unresolved_count: u32`. This API type omits `computed_at` (response is
/// live) and uses `i64` for `unresolved_count` (SQL COUNT type).
#[derive(Serialize, utoipa::ToSchema)]
pub struct DriftProjection {
    pub items: Vec<DriftItem>,
    pub unresolved_count: i64,
}

impl From<CanonicalDriftItem> for DriftItem {
    fn from(c: CanonicalDriftItem) -> Self {
        Self {
            drift_id: String::new(), // canonical type does not carry drift_id
            node_id: c.node_id,
            node_title: c.node_title,
            drift_source: c.drift_source,
            drift_description: c.drift_description,
            detected_at: c.detected_at.to_rfc3339(),
            resolved: c.resolved,
        }
    }
}

impl From<CanonicalDriftProjection> for DriftProjection {
    fn from(c: CanonicalDriftProjection) -> Self {
        Self {
            items: c.items.into_iter().map(DriftItem::from).collect(),
            unresolved_count: c.unresolved_count as i64,
        }
    }
}

/// GET /api/projections/drift
///
/// Returns nodes with detected drift that need revalidation.
/// Queries the `projection_drift` table.
/// Canonical type: `ui_models::projections::DriftProjection` (RDM-006).
///
/// Pipeline: SQL rows -> CanonicalDriftItem -> API DriftItem.
#[utoipa::path(
    get,
    path = "/api/projections/drift",
    responses(
        (status = 200, description = "Drift projection", body = DriftProjection)
    )
)]
pub async fn drift(
    State(state): State<AppState>,
) -> ApiResult<DriftProjection> {
    let rows = sqlx::query(
        r#"SELECT drift_id, node_id, node_title, drift_source,
                  drift_description, detected_at, resolved
           FROM projection_drift
           ORDER BY detected_at DESC
           LIMIT 500"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    // Step 1: Build canonical items.
    let mut canonical_items = Vec::with_capacity(rows.len());
    let mut unresolved_count: u32 = 0;

    for row in &rows {
        let resolved: bool = row.try_get("resolved").map_err(internal_error)?;
        if !resolved {
            unresolved_count += 1;
        }

        canonical_items.push(CanonicalDriftItem {
            node_id: row.try_get("node_id").map_err(internal_error)?,
            node_title: row.try_get("node_title").map_err(internal_error)?,
            drift_source: row.try_get("drift_source").map_err(internal_error)?,
            drift_description: row.try_get("drift_description").map_err(internal_error)?,
            detected_at: row.try_get("detected_at").map_err(internal_error)?,
            resolved,
        });
    }

    let canonical = CanonicalDriftProjection {
        items: canonical_items,
        unresolved_count,
        computed_at: chrono::Utc::now(),
    };

    // Step 2: Convert canonical -> API wire type, then patch in drift_ids.
    let mut api = DriftProjection::from(canonical);

    // Re-apply drift_id from the original SQL rows (not in canonical type).
    for (api_item, row) in api.items.iter_mut().zip(rows.iter()) {
        api_item.drift_id = row.try_get("drift_id").map_err(internal_error)?;
    }

    Ok(Json(api))
}

/// Summary of a single cycle in a loop's history.
///
/// **Canonical equivalent (CD-04):** `ui_models::projections::LoopHistoryCycleItem` (RDM-009).
///
/// **Mapping:** The canonical type uses `CyclePhase` (typed enum) for `phase`
/// and `DateTime<Utc>` for timestamps. This API type flattens `phase` to
/// `String` and timestamps to RFC 3339 strings. Uses `i64` for counts
/// instead of `u32`.
///
/// **Future:** Generate from `ui_models::LoopHistoryCycleItem`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct LoopHistoryCycleItem {
    pub cycle_id: String,
    pub loop_id: String,
    pub objective_id: String,
    pub cycle_index: i32,
    pub phase: String,
    pub tasks_dispatched: i64,
    pub tasks_completed: i64,
    pub tasks_failed: i64,
    pub nodes_promoted: i64,
    pub started_at: String,
    pub completed_at: Option<String>,
}

/// Loop history projection (RDM-009).
///
/// **Canonical equivalent (CD-04):** `ui_models::projections::LoopHistoryProjection`.
///
/// **Mapping:** The canonical type wraps cycles under a single `loop_id` and
/// `objective_id` with `total_cycles` and `computed_at`. This API type
/// returns a flat list of all cycle items across all loops (no grouping),
/// with `loop_id` and `objective_id` on each item. Uses `i64` for
/// `total_cycles`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct LoopHistoryProjection {
    pub cycles: Vec<LoopHistoryCycleItem>,
    pub total_cycles: i64,
}

/// Convert a canonical cycle item to the API wire type.
///
/// The API type carries `loop_id` and `objective_id` on each item (flat)
/// while the canonical type stores them at the projection level. The caller
/// must set those fields after conversion.
impl From<CanonicalLoopHistoryCycleItem> for LoopHistoryCycleItem {
    fn from(c: CanonicalLoopHistoryCycleItem) -> Self {
        Self {
            cycle_id: c.cycle_id,
            loop_id: String::new(),      // set by caller (lives on projection, not item)
            objective_id: String::new(),  // set by caller
            cycle_index: c.cycle_index,
            phase: serde_json::to_value(&c.phase)
                .ok()
                .and_then(|v| v.as_str().map(String::from))
                .unwrap_or_else(|| format!("{:?}", c.phase)),
            tasks_dispatched: c.tasks_dispatched as i64,
            tasks_completed: c.tasks_completed as i64,
            tasks_failed: c.tasks_failed as i64,
            nodes_promoted: c.nodes_promoted as i64,
            started_at: c.started_at.to_rfc3339(),
            completed_at: c.completed_at.map(|t| t.to_rfc3339()),
        }
    }
}

/// GET /api/projections/loop-history
///
/// Returns the history of cycles across all loops.
/// Queries the `projection_loop_history` table.
/// Canonical type: `ui_models::projections::LoopHistoryProjection` (RDM-009).
///
/// Pipeline: SQL rows -> CanonicalLoopHistoryCycleItem -> API LoopHistoryCycleItem.
#[utoipa::path(
    get,
    path = "/api/projections/loop-history",
    responses(
        (status = 200, description = "Loop history projection", body = LoopHistoryProjection)
    )
)]
pub async fn loop_history(
    State(state): State<AppState>,
) -> ApiResult<LoopHistoryProjection> {
    let rows = sqlx::query(
        r#"SELECT cycle_id, loop_id, objective_id, cycle_index, phase,
                  tasks_dispatched, tasks_completed, tasks_failed,
                  nodes_promoted, started_at, completed_at
           FROM projection_loop_history
           ORDER BY started_at DESC
           LIMIT 500"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let total_cycles = rows.len() as i64;
    let mut cycles = Vec::with_capacity(rows.len());

    for row in &rows {
        let phase_str: String = row.try_get("phase").map_err(internal_error)?;
        let loop_id: String = row.try_get("loop_id").map_err(internal_error)?;
        let objective_id: String = row.try_get("objective_id").map_err(internal_error)?;

        // Step 1: Build canonical item.
        let canonical_item = CanonicalLoopHistoryCycleItem {
            cycle_id: row.try_get("cycle_id").map_err(internal_error)?,
            cycle_index: row.try_get("cycle_index").map_err(internal_error)?,
            phase: serde_json::from_value(serde_json::Value::String(phase_str))
                .unwrap_or(CyclePhase::Intake),
            tasks_dispatched: row
                .try_get::<i64, _>("tasks_dispatched")
                .map_err(internal_error)? as u32,
            tasks_completed: row
                .try_get::<i64, _>("tasks_completed")
                .map_err(internal_error)? as u32,
            tasks_failed: row
                .try_get::<i64, _>("tasks_failed")
                .map_err(internal_error)? as u32,
            nodes_promoted: row
                .try_get::<i64, _>("nodes_promoted")
                .map_err(internal_error)? as u32,
            started_at: row.try_get("started_at").map_err(internal_error)?,
            completed_at: row.try_get("completed_at").map_err(internal_error)?,
        };

        // Step 2: Convert canonical -> API wire type, then set flat fields.
        let mut api_item = LoopHistoryCycleItem::from(canonical_item);
        api_item.loop_id = loop_id;
        api_item.objective_id = objective_id;
        cycles.push(api_item);
    }

    Ok(Json(LoopHistoryProjection {
        cycles,
        total_cycles,
    }))
}

/// A single artifact in the timeline.
///
/// **Canonical equivalent (CD-04):** `ui_models::projections::ArtifactTimelineItem` (RDM-010).
///
/// **Mapping:** The canonical type uses `DateTime<Utc>` for `created_at`;
/// this API type uses RFC 3339 `String`. Field names and semantics are
/// otherwise 1:1 with the canonical type.
///
/// **Future:** Generate from `ui_models::ArtifactTimelineItem`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ArtifactTimelineItem {
    pub artifact_id: String,
    pub artifact_kind: String,
    pub title: String,
    pub source_task_id: Option<String>,
    pub source_node_id: Option<String>,
    pub created_at: String,
}

/// Artifact timeline projection (RDM-010).
///
/// **Canonical equivalent (CD-04):** `ui_models::projections::ArtifactTimelineProjection`.
///
/// **Mapping:** The canonical type includes `computed_at: DateTime<Utc>` and
/// `total_count: u32`. This API type omits `computed_at` (response is live)
/// and uses `i64` for `total_count`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ArtifactTimelineProjection {
    pub artifacts: Vec<ArtifactTimelineItem>,
    pub total_count: i64,
}

impl From<CanonicalArtifactTimelineItem> for ArtifactTimelineItem {
    fn from(c: CanonicalArtifactTimelineItem) -> Self {
        Self {
            artifact_id: c.artifact_id,
            artifact_kind: c.artifact_kind,
            title: c.title,
            source_task_id: c.source_task_id,
            source_node_id: c.source_node_id,
            created_at: c.created_at.to_rfc3339(),
        }
    }
}

impl From<CanonicalArtifactTimelineProjection> for ArtifactTimelineProjection {
    fn from(c: CanonicalArtifactTimelineProjection) -> Self {
        Self {
            artifacts: c.artifacts.into_iter().map(ArtifactTimelineItem::from).collect(),
            total_count: c.total_count as i64,
        }
    }
}

/// GET /api/projections/artifact-timeline
///
/// Returns a chronological list of all artifacts produced.
/// Queries the `projection_artifact_timeline` table.
/// Canonical type: `ui_models::projections::ArtifactTimelineProjection` (RDM-010).
///
/// Pipeline: SQL rows -> CanonicalArtifactTimelineProjection -> API ArtifactTimelineProjection.
#[utoipa::path(
    get,
    path = "/api/projections/artifact-timeline",
    responses(
        (status = 200, description = "Artifact timeline projection", body = ArtifactTimelineProjection)
    )
)]
pub async fn artifact_timeline(
    State(state): State<AppState>,
) -> ApiResult<ArtifactTimelineProjection> {
    let rows = sqlx::query(
        r#"SELECT artifact_id, artifact_kind, title,
                  source_task_id, source_node_id, created_at
           FROM projection_artifact_timeline
           ORDER BY created_at DESC
           LIMIT 500"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    // Step 1: Build canonical items.
    let mut canonical_artifacts = Vec::with_capacity(rows.len());
    for row in &rows {
        canonical_artifacts.push(CanonicalArtifactTimelineItem {
            artifact_id: row.try_get("artifact_id").map_err(internal_error)?,
            artifact_kind: row.try_get("artifact_kind").map_err(internal_error)?,
            title: row.try_get("title").map_err(internal_error)?,
            source_task_id: row.try_get("source_task_id").map_err(internal_error)?,
            source_node_id: row.try_get("source_node_id").map_err(internal_error)?,
            created_at: row.try_get("created_at").map_err(internal_error)?,
        });
    }

    let canonical = CanonicalArtifactTimelineProjection {
        artifacts: canonical_artifacts,
        total_count: rows.len() as u32,
        computed_at: chrono::Utc::now(),
    };

    // Step 2: Convert canonical -> API wire type.
    Ok(Json(ArtifactTimelineProjection::from(canonical)))
}
