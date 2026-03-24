use axum::extract::{Path, State};
use axum::response::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::error::{ApiResult, internal_error, not_found};
use crate::state::AppState;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateCycleRequest {
    pub loop_id: String,
    pub idempotency_key: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CycleResponse {
    pub cycle_id: String,
    pub loop_id: String,
    pub phase: String,
    pub policy_snapshot: Value,
    pub created_at: String,
    pub updated_at: String,
    pub duplicated: bool,
}

pub async fn create_cycle(
    State(state): State<AppState>,
    Json(req): Json<CreateCycleRequest>,
) -> ApiResult<CycleResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    let cycle_id = Uuid::now_v7().to_string();

    // BND-010: scoped idempotency check
    let duplicate: Option<String> = sqlx::query_scalar(
        "select aggregate_id from event_journal where aggregate_kind = 'cycle' and idempotency_key = $1 limit 1",
    )
    .bind(&req.idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    if let Some(existing_id) = duplicate {
        let row = sqlx::query(
            "select cycle_id, loop_id, phase, policy_snapshot, created_at, updated_at from cycles where cycle_id = $1",
        )
        .bind(&existing_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal_error)?;

        tx.commit().await.map_err(internal_error)?;

        let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
        let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;

        return Ok(Json(CycleResponse {
            cycle_id: row.try_get("cycle_id").map_err(internal_error)?,
            loop_id: row.try_get("loop_id").map_err(internal_error)?,
            phase: row.try_get("phase").map_err(internal_error)?,
            policy_snapshot: row.try_get("policy_snapshot").map_err(internal_error)?,
            created_at: created_at.to_rfc3339(),
            updated_at: updated_at.to_rfc3339(),
            duplicated: true,
        }));
    }

    // Server derives phase and policy_snapshot (BND-003)
    let phase = "intake";

    // Fetch policy_snapshot from the active user_policies for this loop's objective
    let policy_snapshot: Value = sqlx::query_scalar(
        r#"SELECT COALESCE(
            (SELECT policy_payload FROM user_policies up
             JOIN loops l ON l.objective_id = up.policy_id
             WHERE l.loop_id = $1
             ORDER BY up.revision DESC LIMIT 1),
            '{}'::jsonb
        )"#,
    )
    .bind(&req.loop_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    let row = sqlx::query(
        r#"insert into cycles (cycle_id, loop_id, phase, policy_snapshot, created_at, updated_at)
           values ($1, $2, $3, $4::jsonb, now(), now())
           returning cycle_id, loop_id, phase, policy_snapshot, created_at, updated_at"#,
    )
    .bind(&cycle_id)
    .bind(&req.loop_id)
    .bind(phase)
    .bind(&policy_snapshot)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    sqlx::query(
        r#"insert into event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           values ($1, 'cycle', $2, 'cycle_created', $3, $4::jsonb, now())
           on conflict (aggregate_kind, aggregate_id, idempotency_key) do nothing"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&cycle_id)
    .bind(&req.idempotency_key)
    .bind(serde_json::json!({
        "cycle_id": cycle_id,
        "loop_id": req.loop_id,
        "phase": phase,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
    let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;

    Ok(Json(CycleResponse {
        cycle_id: row.try_get("cycle_id").map_err(internal_error)?,
        loop_id: row.try_get("loop_id").map_err(internal_error)?,
        phase: row.try_get("phase").map_err(internal_error)?,
        policy_snapshot: row.try_get("policy_snapshot").map_err(internal_error)?,
        created_at: created_at.to_rfc3339(),
        updated_at: updated_at.to_rfc3339(),
        duplicated: false,
    }))
}

#[utoipa::path(
    get,
    path = "/api/cycles/{id}",
    params(("id" = String, Path, description = "Cycle ID")),
    responses(
        (status = 200, description = "Cycle details", body = CycleResponse)
    )
)]
pub async fn get_cycle(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<CycleResponse> {
    let row = sqlx::query(
        "select cycle_id, loop_id, phase, policy_snapshot, created_at, updated_at from cycles where cycle_id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?;

    let Some(row) = row else {
        return Err(not_found("cycle not found"));
    };

    let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
    let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;

    Ok(Json(CycleResponse {
        cycle_id: row.try_get("cycle_id").map_err(internal_error)?,
        loop_id: row.try_get("loop_id").map_err(internal_error)?,
        phase: row.try_get("phase").map_err(internal_error)?,
        policy_snapshot: row.try_get("policy_snapshot").map_err(internal_error)?,
        created_at: created_at.to_rfc3339(),
        updated_at: updated_at.to_rfc3339(),
        duplicated: false,
    }))
}

#[utoipa::path(
    get,
    path = "/api/cycles",
    responses(
        (status = 200, description = "List of cycles", body = Vec<CycleResponse>)
    )
)]
pub async fn list_cycles(
    State(state): State<AppState>,
) -> ApiResult<Vec<CycleResponse>> {
    let rows = sqlx::query(
        "select cycle_id, loop_id, phase, policy_snapshot, created_at, updated_at from cycles order by created_at desc",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
        let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;
        results.push(CycleResponse {
            cycle_id: row.try_get("cycle_id").map_err(internal_error)?,
            loop_id: row.try_get("loop_id").map_err(internal_error)?,
            phase: row.try_get("phase").map_err(internal_error)?,
            policy_snapshot: row.try_get("policy_snapshot").map_err(internal_error)?,
            created_at: created_at.to_rfc3339(),
            updated_at: updated_at.to_rfc3339(),
            duplicated: false,
        });
    }

    Ok(Json(results))
}
