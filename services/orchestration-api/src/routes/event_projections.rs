//! Event-to-projection pipeline endpoints.
//!
//! These routes provide projection rebuild and on-the-fly projection queries
//! computed from the event journal. For MVP, projections are computed on-the-fly
//! by the GET endpoints (no cache table needed). The rebuild endpoint exists as
//! a foundation for future materialized projection tables.
//!
//! ## Canonical types (CD-04)
//!
//! The canonical projection types are defined in `packages/ui-models/src/projections.rs`:
//!   - `ui_models::projections::TaskBoardProjection` (RDM-001)
//!   - `ui_models::projections::BranchMainlineProjection` (RDM-003)
//!   - `ui_models::projections::ReviewQueueProjection` (RDM-004)
//!   - `ui_models::projections::CertificationQueueProjection` (RDM-005)
//!
//! The handlers below return simplified summary shapes (status + count)
//! rather than the full item-level projections from ui-models. This is
//! intentional for the rebuild/summary endpoints; the full projections
//! are served by `routes::projections`.

use axum::extract::State;
use axum::response::Json;
use serde::Serialize;
use sqlx::Row;

use crate::error::{ApiResult, internal_error};
use crate::state::AppState;

// Canonical projection types from ui-models (CD-04).
// Summary endpoints aggregate from canonical types to flat (status, count) pairs.
use ui_models::projections::{
    TaskBoardProjection as CanonicalTaskBoardProjection,
    BranchMainlineProjection as CanonicalBranchMainlineProjection,
    ReviewQueueProjection as CanonicalReviewQueueProjection,
    CertificationQueueProjection as CanonicalCertificationQueueProjection,
};

// ── Projection summary types ────────────────────────────────────────────

/// Summary of tasks by status, computed from the tasks table.
///
/// **Canonical type (CD-04):** `ui_models::projections::TaskBoardProjection` (RDM-001).
///
/// **Mapping:** The canonical type carries a `Vec<TaskBoardItem>` with per-task
/// detail, typed `TaskStatus` enum, and `DateTime<Utc>` timestamps. This
/// summary intentionally collapses that to `(status: String, count: i64)` for
/// the rebuild/summary endpoints. The full item-level projection is served by
/// `routes::projections::task_board`.
///
/// **Future:** Generate this type FROM `ui_models::TaskBoardProjection` once
/// utoipa supports external-crate schema derivation.
#[derive(Serialize, utoipa::ToSchema)]
pub struct TaskBoardProjection {
    pub status: String,
    pub count: i64,
}

/// Summary of nodes by lane, computed from the nodes table.
///
/// **Canonical type (CD-04):** `ui_models::projections::BranchMainlineProjection` (RDM-003).
///
/// **Mapping:** The canonical type groups nodes into four `Vec<BranchMainlineItem>`
/// buckets (`branch_nodes`, `mainline_candidate_nodes`, `mainline_nodes`,
/// `blocked_nodes`) using typed `NodeLane`/`NodeLifecycle` enums. This summary
/// flattens that to `(lane: String, count: i64)` for the rebuild endpoint.
///
/// **Future:** Generate from `ui_models::BranchMainlineProjection`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct BranchMainlineProjection {
    pub lane: String,
    pub count: i64,
}

/// Summary of review artifacts by status.
///
/// **Canonical type (CD-04):** `ui_models::projections::ReviewQueueProjection` (RDM-004).
///
/// **Mapping:** The canonical type carries `Vec<ReviewQueueItem>` with typed
/// `review_kind`, `status`, and `DateTime<Utc>` timestamps. This summary
/// collapses to `(status: String, count: i64)` for rebuild.
///
/// **Future:** Generate from `ui_models::ReviewQueueProjection`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ReviewQueueProjection {
    pub status: String,
    pub count: i64,
}

/// Summary of certification submissions by queue status.
///
/// **Canonical type (CD-04):** `ui_models::projections::CertificationQueueProjection` (RDM-005).
///
/// **Mapping:** The canonical type carries `Vec<CertificationQueueItem>` with
/// per-item detail and `DateTime<Utc>` timestamps. This summary uses
/// `(queue_status: String, count: i64)` for rebuild.
///
/// **Future:** Generate from `ui_models::CertificationQueueProjection`.
#[derive(Serialize, utoipa::ToSchema)]
pub struct CertificationQueueProjection {
    pub queue_status: String,
    pub count: i64,
}

// ── Adapter functions: canonical -> summary ─────────────────────────────
//
// These convert ui-models canonical projections into the flat summary
// shapes used by the rebuild/summary endpoints. Implemented as free
// functions rather than From impls to avoid orphan-rule violations on
// Vec<T>.
//
// Currently the rebuild/summary handlers query aggregate SQL directly.
// When the system moves to materialised canonical projections, these
// functions provide the conversion path.

/// Summarise a canonical `TaskBoardProjection` into `(status, count)` pairs.
#[allow(dead_code)]
fn summarise_task_board(c: &CanonicalTaskBoardProjection) -> Vec<TaskBoardProjection> {
    let mut out = Vec::new();
    if c.queued_count > 0 {
        out.push(TaskBoardProjection { status: "queued".to_string(), count: c.queued_count as i64 });
    }
    if c.running_count > 0 {
        out.push(TaskBoardProjection { status: "running".to_string(), count: c.running_count as i64 });
    }
    if c.completed_count > 0 {
        out.push(TaskBoardProjection { status: "succeeded".to_string(), count: c.completed_count as i64 });
    }
    if c.failed_count > 0 {
        out.push(TaskBoardProjection { status: "failed".to_string(), count: c.failed_count as i64 });
    }
    if c.timed_out_count > 0 {
        out.push(TaskBoardProjection { status: "timed_out".to_string(), count: c.timed_out_count as i64 });
    }
    if c.cancelled_count > 0 {
        out.push(TaskBoardProjection { status: "cancelled".to_string(), count: c.cancelled_count as i64 });
    }
    out.sort_by(|a, b| a.status.cmp(&b.status));
    out
}

/// Summarise a canonical `BranchMainlineProjection` into `(lane, count)` pairs.
#[allow(dead_code)]
fn summarise_branch_mainline(c: &CanonicalBranchMainlineProjection) -> Vec<BranchMainlineProjection> {
    let mut out = Vec::new();
    if !c.branch_nodes.is_empty() {
        out.push(BranchMainlineProjection { lane: "branch".to_string(), count: c.branch_nodes.len() as i64 });
    }
    if !c.blocked_nodes.is_empty() {
        out.push(BranchMainlineProjection { lane: "blocked".to_string(), count: c.blocked_nodes.len() as i64 });
    }
    if !c.mainline_nodes.is_empty() {
        out.push(BranchMainlineProjection { lane: "mainline".to_string(), count: c.mainline_nodes.len() as i64 });
    }
    if !c.mainline_candidate_nodes.is_empty() {
        out.push(BranchMainlineProjection { lane: "mainline_candidate".to_string(), count: c.mainline_candidate_nodes.len() as i64 });
    }
    out.sort_by(|a, b| a.lane.cmp(&b.lane));
    out
}

/// Summarise a canonical `ReviewQueueProjection` into `(status, count)` pairs.
#[allow(dead_code)]
fn summarise_review_queue(c: &CanonicalReviewQueueProjection) -> Vec<ReviewQueueProjection> {
    let mut out = Vec::new();
    if c.pending_count > 0 {
        out.push(ReviewQueueProjection { status: "pending".to_string(), count: c.pending_count as i64 });
    }
    if c.in_progress_count > 0 {
        out.push(ReviewQueueProjection { status: "in_progress".to_string(), count: c.in_progress_count as i64 });
    }
    out.sort_by(|a, b| a.status.cmp(&b.status));
    out
}

/// Summarise a canonical `CertificationQueueProjection` into `(queue_status, count)` pairs.
#[allow(dead_code)]
fn summarise_certification_queue(c: &CanonicalCertificationQueueProjection) -> Vec<CertificationQueueProjection> {
    let mut out = Vec::new();
    if c.pending_count > 0 {
        out.push(CertificationQueueProjection { queue_status: "pending".to_string(), count: c.pending_count as i64 });
    }
    if c.in_progress_count > 0 {
        out.push(CertificationQueueProjection { queue_status: "in_progress".to_string(), count: c.in_progress_count as i64 });
    }
    out.sort_by(|a, b| a.queue_status.cmp(&b.queue_status));
    out
}

/// Complete projection rebuild result.
#[derive(Serialize, utoipa::ToSchema)]
pub struct ProjectionRebuildResponse {
    pub task_board: Vec<TaskBoardProjection>,
    pub branch_mainline: Vec<BranchMainlineProjection>,
    pub review_queue: Vec<ReviewQueueProjection>,
    pub certification_queue: Vec<CertificationQueueProjection>,
    pub total_events_scanned: i64,
}

// ── POST /api/projections/rebuild ───────────────────────────────────────

/// Rebuild all read-model projections from the current database state.
///
/// For MVP this computes projections on-the-fly by aggregating the current
/// tables. In the future this could replay the event journal to rebuild
/// materialized views.
#[utoipa::path(
    post,
    path = "/api/projections/rebuild",
    responses(
        (status = 200, description = "Projection rebuild result", body = ProjectionRebuildResponse)
    )
)]
pub async fn rebuild_projections(
    State(state): State<AppState>,
) -> ApiResult<ProjectionRebuildResponse> {
    // Count total events (to confirm we scanned the journal)
    let total_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM event_journal",
    )
    .fetch_one(&state.pool)
    .await
    .map_err(internal_error)?;

    // 1. Tasks by status
    let task_rows = sqlx::query(
        "SELECT status, COUNT(*) as cnt FROM tasks GROUP BY status ORDER BY status",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let task_board: Vec<TaskBoardProjection> = task_rows
        .iter()
        .map(|row| {
            Ok(TaskBoardProjection {
                status: row.try_get("status").map_err(internal_error)?,
                count: row.try_get("cnt").map_err(internal_error)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    // 2. Nodes by lane
    let node_rows = sqlx::query(
        "SELECT COALESCE(lane, 'unassigned') as lane, COUNT(*) as cnt FROM nodes GROUP BY lane ORDER BY lane",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let branch_mainline: Vec<BranchMainlineProjection> = node_rows
        .iter()
        .map(|row| {
            Ok(BranchMainlineProjection {
                lane: row.try_get("lane").map_err(internal_error)?,
                count: row.try_get("cnt").map_err(internal_error)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    // 3. Reviews by status
    let review_rows = sqlx::query(
        "SELECT status, COUNT(*) as cnt FROM review_artifacts GROUP BY status ORDER BY status",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let review_queue: Vec<ReviewQueueProjection> = review_rows
        .iter()
        .map(|row| {
            Ok(ReviewQueueProjection {
                status: row.try_get("status").map_err(internal_error)?,
                count: row.try_get("cnt").map_err(internal_error)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    // 4. Certifications by queue status
    let cert_rows = sqlx::query(
        "SELECT queue_status, COUNT(*) as cnt FROM certification_submissions GROUP BY queue_status ORDER BY queue_status",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let certification_queue: Vec<CertificationQueueProjection> = cert_rows
        .iter()
        .map(|row| {
            Ok(CertificationQueueProjection {
                queue_status: row.try_get("queue_status").map_err(internal_error)?,
                count: row.try_get("cnt").map_err(internal_error)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(ProjectionRebuildResponse {
        task_board,
        branch_mainline,
        review_queue,
        certification_queue,
        total_events_scanned: total_events,
    }))
}

// ── GET /api/projections/task-board ─────────────────────────────────────

/// Get the current task board projection (tasks by status).
///
/// Canonical type: `ui_models::projections::TaskBoardProjection` (RDM-001).
/// Returns a flat summary; see `routes::projections::task_board` for item-level data.
pub async fn get_task_board_projection(
    State(state): State<AppState>,
) -> ApiResult<Vec<TaskBoardProjection>> {
    let rows = sqlx::query(
        "SELECT status, COUNT(*) as cnt FROM tasks GROUP BY status ORDER BY status",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let results: Vec<TaskBoardProjection> = rows
        .iter()
        .map(|row| {
            Ok(TaskBoardProjection {
                status: row.try_get("status").map_err(internal_error)?,
                count: row.try_get("cnt").map_err(internal_error)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(results))
}

// ── GET /api/projections/branch-mainline ────────────────────────────────

/// Get the current branch/mainline projection (nodes by lane).
///
/// Canonical type: `ui_models::projections::BranchMainlineProjection` (RDM-003).
/// Returns a flat summary; see `routes::projections::branch_mainline` for item-level data.
pub async fn get_branch_mainline_projection(
    State(state): State<AppState>,
) -> ApiResult<Vec<BranchMainlineProjection>> {
    let rows = sqlx::query(
        "SELECT COALESCE(lane, 'unassigned') as lane, COUNT(*) as cnt FROM nodes GROUP BY lane ORDER BY lane",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let results: Vec<BranchMainlineProjection> = rows
        .iter()
        .map(|row| {
            Ok(BranchMainlineProjection {
                lane: row.try_get("lane").map_err(internal_error)?,
                count: row.try_get("cnt").map_err(internal_error)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(results))
}

// ── GET /api/projections/review-queue ───────────────────────────────────

/// Get the current review queue projection (reviews by status).
///
/// Canonical type: `ui_models::projections::ReviewQueueProjection` (RDM-004).
/// Returns a flat summary; see `routes::projections::review_queue` for item-level data.
pub async fn get_review_queue_projection(
    State(state): State<AppState>,
) -> ApiResult<Vec<ReviewQueueProjection>> {
    let rows = sqlx::query(
        "SELECT status, COUNT(*) as cnt FROM review_artifacts GROUP BY status ORDER BY status",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let results: Vec<ReviewQueueProjection> = rows
        .iter()
        .map(|row| {
            Ok(ReviewQueueProjection {
                status: row.try_get("status").map_err(internal_error)?,
                count: row.try_get("cnt").map_err(internal_error)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(results))
}

// ── GET /api/projections/certification-queue ────────────────────────────

/// Get the current certification queue projection (submissions by status).
///
/// Canonical type: `ui_models::projections::CertificationQueueProjection` (RDM-005).
/// Returns a flat summary; see `routes::projections::certification_queue` for item-level data.
pub async fn get_certification_queue_projection(
    State(state): State<AppState>,
) -> ApiResult<Vec<CertificationQueueProjection>> {
    let rows = sqlx::query(
        "SELECT queue_status, COUNT(*) as cnt FROM certification_submissions GROUP BY queue_status ORDER BY queue_status",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let results: Vec<CertificationQueueProjection> = rows
        .iter()
        .map(|row| {
            Ok(CertificationQueueProjection {
                queue_status: row.try_get("queue_status").map_err(internal_error)?,
                count: row.try_get("cnt").map_err(internal_error)?,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    Ok(Json(results))
}
