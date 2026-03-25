use axum::{extract::State, response::Json};
use serde::Serialize;
use sqlx::Row;

use crate::error::internal_error;
use crate::state::AppState;

#[derive(Debug, Serialize, utoipa::ToSchema)]
pub struct ConflictResponse {
    pub conflict_id: String,
    pub conflict_kind: String,
    pub status: String,
    pub description: String,
    pub node_id: Option<String>,
    pub task_id: Option<String>,
    pub created_at: String,
    pub resolved_at: Option<String>,
}

#[utoipa::path(
    get,
    path = "/api/conflicts",
    responses(
        (status = 200, description = "List all conflicts", body = Vec<ConflictResponse>)
    )
)]
pub async fn list_conflicts(
    State(state): State<AppState>,
) -> Result<Json<Vec<ConflictResponse>>, (axum::http::StatusCode, String)> {
    let rows = sqlx::query(
        "SELECT conflict_id, conflict_kind, status, \
                COALESCE(description, '') as description, \
                node_id, task_id, \
                created_at::text, resolved_at::text \
         FROM conflicts \
         ORDER BY created_at DESC \
         LIMIT 200",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let conflicts = rows
        .iter()
        .map(|r| ConflictResponse {
            conflict_id: r.get("conflict_id"),
            conflict_kind: r.get("conflict_kind"),
            status: r.get("status"),
            description: r.get("description"),
            node_id: r.try_get("node_id").ok(),
            task_id: r.try_get("task_id").ok(),
            created_at: r.get("created_at"),
            resolved_at: r.try_get("resolved_at").ok().flatten(),
        })
        .collect();

    Ok(Json(conflicts))
}

#[utoipa::path(
    get,
    path = "/api/conflicts/{id}",
    responses(
        (status = 200, description = "Get a single conflict", body = ConflictResponse)
    ),
    params(
        ("id" = String, Path, description = "Conflict ID")
    )
)]
pub async fn get_conflict(
    State(state): State<AppState>,
    axum::extract::Path(id): axum::extract::Path<String>,
) -> Result<Json<ConflictResponse>, (axum::http::StatusCode, String)> {
    let row = sqlx::query(
        "SELECT conflict_id, conflict_kind, status, \
                COALESCE(description, '') as description, \
                node_id, task_id, \
                created_at::text, resolved_at::text \
         FROM conflicts WHERE conflict_id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?
    .ok_or_else(|| {
        (
            axum::http::StatusCode::NOT_FOUND,
            format!("Conflict {} not found", id),
        )
    })?;

    Ok(Json(ConflictResponse {
        conflict_id: row.get("conflict_id"),
        conflict_kind: row.get("conflict_kind"),
        status: row.get("status"),
        description: row.get("description"),
        node_id: row.try_get("node_id").ok(),
        task_id: row.try_get("task_id").ok(),
        created_at: row.get("created_at"),
        resolved_at: row.try_get("resolved_at").ok().flatten(),
    }))
}
