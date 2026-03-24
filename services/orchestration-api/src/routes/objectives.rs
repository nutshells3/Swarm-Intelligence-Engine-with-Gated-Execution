use axum::extract::{Path, State};
use axum::response::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::error::{ApiResult, internal_error, not_found};
use crate::state::AppState;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateObjectiveRequest {
    pub summary: String,
    #[serde(default = "default_planning_status")]
    pub planning_status: String,
    #[serde(default = "default_plan_gate")]
    pub plan_gate: String,
    pub idempotency_key: String,
}

fn default_planning_status() -> String {
    "planning".to_string()
}

fn default_plan_gate() -> String {
    "draft".to_string()
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ObjectiveResponse {
    pub objective_id: String,
    pub summary: String,
    pub planning_status: String,
    pub plan_gate: String,
    pub created_at: String,
    pub updated_at: String,
    pub duplicated: bool,
}

#[utoipa::path(
    post,
    path = "/api/objectives",
    request_body = CreateObjectiveRequest,
    responses(
        (status = 200, description = "Created objective", body = ObjectiveResponse)
    )
)]
pub async fn create_objective(
    State(state): State<AppState>,
    Json(req): Json<CreateObjectiveRequest>,
) -> ApiResult<ObjectiveResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    let objective_id = Uuid::now_v7().to_string();

    // BND-010: scoped idempotency check (aggregate_kind + idempotency_key unique index)
    let duplicate: Option<String> = sqlx::query_scalar(
        "select aggregate_id from event_journal where aggregate_kind = 'objective' and idempotency_key = $1 limit 1",
    )
    .bind(&req.idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    if let Some(existing_id) = duplicate {
        let row = sqlx::query(
            "select objective_id, summary, planning_status, plan_gate, created_at, updated_at from objectives where objective_id = $1",
        )
        .bind(&existing_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal_error)?;

        tx.commit().await.map_err(internal_error)?;

        let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
        let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;

        return Ok(Json(ObjectiveResponse {
            objective_id: row.try_get("objective_id").map_err(internal_error)?,
            summary: row.try_get("summary").map_err(internal_error)?,
            planning_status: row.try_get("planning_status").map_err(internal_error)?,
            plan_gate: row.try_get("plan_gate").map_err(internal_error)?,
            created_at: created_at.to_rfc3339(),
            updated_at: updated_at.to_rfc3339(),
            duplicated: true,
        }));
    }

    // Insert objective
    let row = sqlx::query(
        r#"insert into objectives (objective_id, summary, planning_status, plan_gate, created_at, updated_at)
           values ($1, $2, $3, $4, now(), now())
           returning objective_id, summary, planning_status, plan_gate, created_at, updated_at"#,
    )
    .bind(&objective_id)
    .bind(&req.summary)
    .bind(&req.planning_status)
    .bind(&req.plan_gate)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Emit event (ON CONFLICT guards against races with the idempotency check)
    sqlx::query(
        r#"insert into event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           values ($1, 'objective', $2, 'objective_created', $3, $4::jsonb, now())
           on conflict (aggregate_kind, aggregate_id, idempotency_key) do nothing"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&objective_id)
    .bind(&req.idempotency_key)
    .bind(serde_json::json!({
        "objective_id": objective_id,
        "summary": req.summary,
        "planning_status": req.planning_status,
        "plan_gate": req.plan_gate,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
    let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;

    Ok(Json(ObjectiveResponse {
        objective_id: row.try_get("objective_id").map_err(internal_error)?,
        summary: row.try_get("summary").map_err(internal_error)?,
        planning_status: row.try_get("planning_status").map_err(internal_error)?,
        plan_gate: row.try_get("plan_gate").map_err(internal_error)?,
        created_at: created_at.to_rfc3339(),
        updated_at: updated_at.to_rfc3339(),
        duplicated: false,
    }))
}

#[utoipa::path(
    get,
    path = "/api/objectives/{id}",
    params(("id" = String, Path, description = "Objective ID")),
    responses(
        (status = 200, description = "Objective details", body = ObjectiveResponse)
    )
)]
pub async fn get_objective(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<ObjectiveResponse> {
    let row = sqlx::query(
        "select objective_id, summary, planning_status, plan_gate, created_at, updated_at from objectives where objective_id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?;

    let Some(row) = row else {
        return Err(not_found("objective not found"));
    };

    let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
    let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;

    Ok(Json(ObjectiveResponse {
        objective_id: row.try_get("objective_id").map_err(internal_error)?,
        summary: row.try_get("summary").map_err(internal_error)?,
        planning_status: row.try_get("planning_status").map_err(internal_error)?,
        plan_gate: row.try_get("plan_gate").map_err(internal_error)?,
        created_at: created_at.to_rfc3339(),
        updated_at: updated_at.to_rfc3339(),
        duplicated: false,
    }))
}

#[utoipa::path(
    get,
    path = "/api/objectives",
    responses(
        (status = 200, description = "List of objectives", body = Vec<ObjectiveResponse>)
    )
)]
pub async fn list_objectives(
    State(state): State<AppState>,
) -> ApiResult<Vec<ObjectiveResponse>> {
    let rows = sqlx::query(
        "select objective_id, summary, planning_status, plan_gate, created_at, updated_at from objectives order by created_at desc",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
        let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;
        results.push(ObjectiveResponse {
            objective_id: row.try_get("objective_id").map_err(internal_error)?,
            summary: row.try_get("summary").map_err(internal_error)?,
            planning_status: row.try_get("planning_status").map_err(internal_error)?,
            plan_gate: row.try_get("plan_gate").map_err(internal_error)?,
            created_at: created_at.to_rfc3339(),
            updated_at: updated_at.to_rfc3339(),
            duplicated: false,
        });
    }

    Ok(Json(results))
}

// ── Plan Gate for an objective ──────────────────────────────────────

#[derive(Serialize, utoipa::ToSchema)]
pub struct GateConditionEntry {
    pub label: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct PlanGateResponse {
    pub status: String,
    pub conditions: Vec<GateConditionEntry>,
    pub unresolved_questions: i32,
    pub block_reason: Option<String>,
}

/// GET /api/objectives/{id}/gate
///
/// Returns the plan gate for an objective (via objective -> plan -> plan_gates),
/// or null if no plan exists.
#[utoipa::path(
    get,
    path = "/api/objectives/{id}/gate",
    params(("id" = String, Path, description = "Objective ID")),
    responses(
        (status = 200, description = "Plan gate for objective", body = Option<PlanGateResponse>)
    )
)]
pub async fn get_objective_gate(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Option<PlanGateResponse>> {
    // Verify objective exists
    let obj_exists: Option<String> = sqlx::query_scalar(
        "SELECT objective_id FROM objectives WHERE objective_id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?;

    if obj_exists.is_none() {
        return Err(not_found("objective not found"));
    }

    // Join through plans to plan_gates
    let row = sqlx::query(
        r#"SELECT pg.current_status, pg.condition_entries, pg.unresolved_question_count, pg.override_reason
           FROM plan_gates pg
           JOIN plans p ON p.plan_id = pg.plan_id
           WHERE p.objective_id = $1
           ORDER BY pg.evaluated_at DESC
           LIMIT 1"#,
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?;

    let Some(row) = row else {
        return Ok(Json(None));
    };

    let status: String = row.try_get("current_status").map_err(internal_error)?;
    let condition_entries: Value = row.try_get("condition_entries").map_err(internal_error)?;
    let unresolved_questions: i32 = row.try_get("unresolved_question_count").map_err(internal_error)?;
    let override_reason: Option<String> = row.try_get("override_reason").map_err(internal_error)?;

    // Parse condition_entries JSON array into Vec<GateConditionEntry>
    let conditions: Vec<GateConditionEntry> = if let Some(arr) = condition_entries.as_array() {
        arr.iter()
            .map(|entry| GateConditionEntry {
                label: entry.get("label").and_then(|v| v.as_str()).unwrap_or("").to_string(),
                passed: entry.get("passed").and_then(|v| v.as_bool()).unwrap_or(false),
                detail: entry.get("detail").and_then(|v| v.as_str()).unwrap_or("").to_string(),
            })
            .collect()
    } else {
        vec![]
    };

    // Determine block_reason: if status is "open" and there's no override,
    // check for blocking unresolved questions
    let block_reason = if status == "open" {
        let blocking_count: Option<i64> = sqlx::query_scalar(
            r#"SELECT COUNT(*) FROM unresolved_questions
               WHERE objective_id = $1 AND severity = 'blocking' AND resolution_status = 'open'"#,
        )
        .bind(&id)
        .fetch_one(&state.pool)
        .await
        .map_err(internal_error)?;

        if blocking_count.unwrap_or(0) > 0 {
            Some(format!("{} blocking question(s) unresolved", blocking_count.unwrap_or(0)))
        } else {
            None
        }
    } else if status == "overridden" {
        override_reason
    } else {
        None
    };

    Ok(Json(Some(PlanGateResponse {
        status,
        conditions,
        unresolved_questions,
        block_reason,
    })))
}

// ── Milestones for an objective ─────────────────────────────────────

#[derive(Serialize, utoipa::ToSchema)]
pub struct MilestoneNodeResponse {
    pub milestone_id: String,
    pub title: String,
    pub description: String,
    pub status: String,
    pub parent_id: Option<String>,
    pub ordering: i32,
}

/// GET /api/objectives/{id}/milestones
///
/// Returns all milestone nodes for an objective (via objective -> milestone_trees -> milestone_nodes).
#[utoipa::path(
    get,
    path = "/api/objectives/{id}/milestones",
    params(("id" = String, Path, description = "Objective ID")),
    responses(
        (status = 200, description = "Milestones for objective", body = Vec<MilestoneNodeResponse>)
    )
)]
pub async fn get_objective_milestones(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<Vec<MilestoneNodeResponse>> {
    // Verify objective exists
    let obj_exists: Option<String> = sqlx::query_scalar(
        "SELECT objective_id FROM objectives WHERE objective_id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?;

    if obj_exists.is_none() {
        return Err(not_found("objective not found"));
    }

    let rows = sqlx::query(
        r#"SELECT mn.milestone_id, mn.title, mn.description, mn.status, mn.parent_id, mn.ordering
           FROM milestone_nodes mn
           JOIN milestone_trees mt ON mt.tree_id = mn.tree_id
           WHERE mt.objective_id = $1
           ORDER BY mn.ordering ASC"#,
    )
    .bind(&id)
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        results.push(MilestoneNodeResponse {
            milestone_id: row.try_get("milestone_id").map_err(internal_error)?,
            title: row.try_get("title").map_err(internal_error)?,
            description: row.try_get("description").map_err(internal_error)?,
            status: row.try_get("status").map_err(internal_error)?,
            parent_id: row.try_get("parent_id").map_err(internal_error)?,
            ordering: row.try_get("ordering").map_err(internal_error)?,
        });
    }

    Ok(Json(results))
}
