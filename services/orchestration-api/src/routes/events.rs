use axum::extract::State;
use axum::response::Json;
use serde::Serialize;
use serde_json::Value;
use sqlx::Row;

use crate::error::{ApiResult, internal_error};
use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct EventResponse {
    pub event_id: String,
    pub aggregate_kind: String,
    pub aggregate_id: String,
    pub event_kind: String,
    pub idempotency_key: String,
    pub payload: Value,
    pub created_at: String,
}

#[utoipa::path(
    get,
    path = "/api/events",
    responses(
        (status = 200, description = "List of events", body = Vec<EventResponse>)
    )
)]
pub async fn list_events(
    State(state): State<AppState>,
) -> ApiResult<Vec<EventResponse>> {
    let rows = sqlx::query(
        "select event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at from event_journal order by created_at desc limit 200",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
        results.push(EventResponse {
            event_id: row.try_get("event_id").map_err(internal_error)?,
            aggregate_kind: row.try_get("aggregate_kind").map_err(internal_error)?,
            aggregate_id: row.try_get("aggregate_id").map_err(internal_error)?,
            event_kind: row.try_get("event_kind").map_err(internal_error)?,
            idempotency_key: row.try_get("idempotency_key").map_err(internal_error)?,
            payload: row.try_get("payload").map_err(internal_error)?,
            created_at: created_at.to_rfc3339(),
        });
    }

    Ok(Json(results))
}
