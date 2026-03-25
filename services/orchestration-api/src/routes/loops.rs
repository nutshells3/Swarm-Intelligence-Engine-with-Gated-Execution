use axum::extract::{Path, State};
use axum::response::Json;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::error::{ApiResult, internal_error, not_found};
use crate::state::AppState;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateLoopRequest {
    pub objective_id: String,
    pub active_track: String,
    pub idempotency_key: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct LoopResponse {
    pub loop_id: String,
    pub objective_id: String,
    pub cycle_index: i32,
    pub active_track: String,
    pub created_at: String,
    pub updated_at: String,
    pub duplicated: bool,
}

pub async fn create_loop(
    State(state): State<AppState>,
    Json(req): Json<CreateLoopRequest>,
) -> ApiResult<LoopResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    let loop_id = Uuid::now_v7().to_string();

    // Scoped idempotency check
    let duplicate: Option<String> = sqlx::query_scalar(
        "select aggregate_id from event_journal where aggregate_kind = 'loop' and idempotency_key = $1 limit 1",
    )
    .bind(&req.idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    if let Some(existing_id) = duplicate {
        let row = sqlx::query(
            "select loop_id, objective_id, cycle_index, active_track, created_at, updated_at from loops where loop_id = $1",
        )
        .bind(&existing_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal_error)?;

        tx.commit().await.map_err(internal_error)?;

        let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
        let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;

        return Ok(Json(LoopResponse {
            loop_id: row.try_get("loop_id").map_err(internal_error)?,
            objective_id: row.try_get("objective_id").map_err(internal_error)?,
            cycle_index: row.try_get("cycle_index").map_err(internal_error)?,
            active_track: row.try_get("active_track").map_err(internal_error)?,
            created_at: created_at.to_rfc3339(),
            updated_at: updated_at.to_rfc3339(),
            duplicated: true,
        }));
    }

    let row = sqlx::query(
        r#"insert into loops (loop_id, objective_id, cycle_index, active_track, created_at, updated_at)
           values ($1, $2, 0, $3, now(), now())
           returning loop_id, objective_id, cycle_index, active_track, created_at, updated_at"#,
    )
    .bind(&loop_id)
    .bind(&req.objective_id)
    .bind(&req.active_track)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    sqlx::query(
        r#"insert into event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           values ($1, 'loop', $2, 'loop_created', $3, $4::jsonb, now())
           on conflict (aggregate_kind, aggregate_id, idempotency_key) do nothing"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&loop_id)
    .bind(&req.idempotency_key)
    .bind(serde_json::json!({
        "loop_id": loop_id,
        "objective_id": req.objective_id,
        "active_track": req.active_track,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
    let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;

    Ok(Json(LoopResponse {
        loop_id: row.try_get("loop_id").map_err(internal_error)?,
        objective_id: row.try_get("objective_id").map_err(internal_error)?,
        cycle_index: row.try_get("cycle_index").map_err(internal_error)?,
        active_track: row.try_get("active_track").map_err(internal_error)?,
        created_at: created_at.to_rfc3339(),
        updated_at: updated_at.to_rfc3339(),
        duplicated: false,
    }))
}

#[utoipa::path(
    get,
    path = "/api/loops/{id}",
    params(("id" = String, Path, description = "Loop ID")),
    responses(
        (status = 200, description = "Loop details", body = LoopResponse)
    )
)]
pub async fn get_loop(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<LoopResponse> {
    let row = sqlx::query(
        "select loop_id, objective_id, cycle_index, active_track, created_at, updated_at from loops where loop_id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?;

    let Some(row) = row else {
        return Err(not_found("loop not found"));
    };

    let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
    let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;

    Ok(Json(LoopResponse {
        loop_id: row.try_get("loop_id").map_err(internal_error)?,
        objective_id: row.try_get("objective_id").map_err(internal_error)?,
        cycle_index: row.try_get("cycle_index").map_err(internal_error)?,
        active_track: row.try_get("active_track").map_err(internal_error)?,
        created_at: created_at.to_rfc3339(),
        updated_at: updated_at.to_rfc3339(),
        duplicated: false,
    }))
}

#[utoipa::path(
    get,
    path = "/api/loops",
    responses(
        (status = 200, description = "List of loops", body = Vec<LoopResponse>)
    )
)]
pub async fn list_loops(
    State(state): State<AppState>,
) -> ApiResult<Vec<LoopResponse>> {
    let rows = sqlx::query(
        "select loop_id, objective_id, cycle_index, active_track, created_at, updated_at from loops order by created_at desc",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
        let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;
        results.push(LoopResponse {
            loop_id: row.try_get("loop_id").map_err(internal_error)?,
            objective_id: row.try_get("objective_id").map_err(internal_error)?,
            cycle_index: row.try_get("cycle_index").map_err(internal_error)?,
            active_track: row.try_get("active_track").map_err(internal_error)?,
            created_at: created_at.to_rfc3339(),
            updated_at: updated_at.to_rfc3339(),
            duplicated: false,
        });
    }

    Ok(Json(results))
}
