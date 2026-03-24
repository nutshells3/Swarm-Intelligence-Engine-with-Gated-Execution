use axum::extract::{Path, State};
use axum::response::Json;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::error::{ApiResult, internal_error, not_found};
use crate::state::AppState;

// ── Tasks ───────────────────────────────────────────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateTaskRequest {
    pub node_id: String,
    pub worker_role: String,
    pub skill_pack_id: String,
    pub idempotency_key: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TaskResponse {
    pub task_id: String,
    pub node_id: String,
    pub worker_role: String,
    pub skill_pack_id: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub duplicated: bool,
}

#[utoipa::path(
    post,
    path = "/api/tasks",
    request_body = CreateTaskRequest,
    responses(
        (status = 200, description = "Created task", body = TaskResponse)
    )
)]
pub async fn create_task(
    State(state): State<AppState>,
    Json(req): Json<CreateTaskRequest>,
) -> ApiResult<TaskResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    let task_id = Uuid::now_v7().to_string();

    // BND-010: scoped idempotency check
    let duplicate: Option<String> = sqlx::query_scalar(
        "select aggregate_id from event_journal where aggregate_kind = 'task' and idempotency_key = $1 limit 1",
    )
    .bind(&req.idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    if let Some(existing_id) = duplicate {
        let row = sqlx::query(
            "select task_id, node_id, worker_role, skill_pack_id, status, created_at, updated_at from tasks where task_id = $1",
        )
        .bind(&existing_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal_error)?;

        tx.commit().await.map_err(internal_error)?;

        let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
        let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;

        return Ok(Json(TaskResponse {
            task_id: row.try_get("task_id").map_err(internal_error)?,
            node_id: row.try_get("node_id").map_err(internal_error)?,
            worker_role: row.try_get("worker_role").map_err(internal_error)?,
            skill_pack_id: row.try_get("skill_pack_id").map_err(internal_error)?,
            status: row.try_get("status").map_err(internal_error)?,
            created_at: created_at.to_rfc3339(),
            updated_at: updated_at.to_rfc3339(),
            duplicated: true,
        }));
    }

    let row = sqlx::query(
        r#"insert into tasks (task_id, node_id, worker_role, skill_pack_id, status, created_at, updated_at)
           values ($1, $2, $3, $4, $5, now(), now())
           returning task_id, node_id, worker_role, skill_pack_id, status, created_at, updated_at"#,
    )
    .bind(&task_id)
    .bind(&req.node_id)
    .bind(&req.worker_role)
    .bind(&req.skill_pack_id)
    .bind("queued")
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    sqlx::query(
        r#"insert into event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           values ($1, 'task', $2, 'task_created', $3, $4::jsonb, now())
           on conflict (aggregate_kind, aggregate_id, idempotency_key) do nothing"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&task_id)
    .bind(&req.idempotency_key)
    .bind(serde_json::json!({
        "task_id": task_id,
        "node_id": req.node_id,
        "worker_role": req.worker_role,
        "skill_pack_id": req.skill_pack_id,
        "status": "queued",
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
    let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;

    Ok(Json(TaskResponse {
        task_id: row.try_get("task_id").map_err(internal_error)?,
        node_id: row.try_get("node_id").map_err(internal_error)?,
        worker_role: row.try_get("worker_role").map_err(internal_error)?,
        skill_pack_id: row.try_get("skill_pack_id").map_err(internal_error)?,
        status: row.try_get("status").map_err(internal_error)?,
        created_at: created_at.to_rfc3339(),
        updated_at: updated_at.to_rfc3339(),
        duplicated: false,
    }))
}

#[utoipa::path(
    get,
    path = "/api/tasks/{id}",
    params(("id" = String, Path, description = "Task ID")),
    responses(
        (status = 200, description = "Task details", body = TaskResponse)
    )
)]
pub async fn get_task(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<TaskResponse> {
    let row = sqlx::query(
        "select task_id, node_id, worker_role, skill_pack_id, status, created_at, updated_at from tasks where task_id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?;

    let Some(row) = row else {
        return Err(not_found("task not found"));
    };

    let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
    let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;

    Ok(Json(TaskResponse {
        task_id: row.try_get("task_id").map_err(internal_error)?,
        node_id: row.try_get("node_id").map_err(internal_error)?,
        worker_role: row.try_get("worker_role").map_err(internal_error)?,
        skill_pack_id: row.try_get("skill_pack_id").map_err(internal_error)?,
        status: row.try_get("status").map_err(internal_error)?,
        created_at: created_at.to_rfc3339(),
        updated_at: updated_at.to_rfc3339(),
        duplicated: false,
    }))
}

#[utoipa::path(
    get,
    path = "/api/tasks",
    responses(
        (status = 200, description = "List of tasks", body = Vec<TaskResponse>)
    )
)]
pub async fn list_tasks(
    State(state): State<AppState>,
) -> ApiResult<Vec<TaskResponse>> {
    let rows = sqlx::query(
        "select task_id, node_id, worker_role, skill_pack_id, status, created_at, updated_at from tasks order by created_at desc",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
        let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;
        results.push(TaskResponse {
            task_id: row.try_get("task_id").map_err(internal_error)?,
            node_id: row.try_get("node_id").map_err(internal_error)?,
            worker_role: row.try_get("worker_role").map_err(internal_error)?,
            skill_pack_id: row.try_get("skill_pack_id").map_err(internal_error)?,
            status: row.try_get("status").map_err(internal_error)?,
            created_at: created_at.to_rfc3339(),
            updated_at: updated_at.to_rfc3339(),
            duplicated: false,
        });
    }

    Ok(Json(results))
}

// ── Task Attempts ───────────────────────────────────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateTaskAttemptRequest {
    pub task_id: String,
    pub lease_owner: Option<String>,
    pub idempotency_key: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TaskAttemptResponse {
    pub task_attempt_id: String,
    pub task_id: String,
    pub attempt_index: i32,
    pub lease_owner: Option<String>,
    pub status: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub duplicated: bool,
}

#[utoipa::path(
    post,
    path = "/api/task-attempts",
    request_body = CreateTaskAttemptRequest,
    responses(
        (status = 200, description = "Created task attempt", body = TaskAttemptResponse)
    )
)]
pub async fn create_task_attempt(
    State(state): State<AppState>,
    Json(req): Json<CreateTaskAttemptRequest>,
) -> ApiResult<TaskAttemptResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    let attempt_id = Uuid::now_v7().to_string();

    // BND-010: scoped idempotency check
    let duplicate: Option<String> = sqlx::query_scalar(
        "select aggregate_id from event_journal where aggregate_kind = 'task_attempt' and idempotency_key = $1 limit 1",
    )
    .bind(&req.idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    if let Some(existing_id) = duplicate {
        let row = sqlx::query(
            "select task_attempt_id, task_id, attempt_index, lease_owner, status, started_at, finished_at from task_attempts where task_attempt_id = $1",
        )
        .bind(&existing_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal_error)?;

        tx.commit().await.map_err(internal_error)?;

        let started_at: Option<chrono::DateTime<chrono::Utc>> = row.try_get("started_at").map_err(internal_error)?;
        let finished_at: Option<chrono::DateTime<chrono::Utc>> = row.try_get("finished_at").map_err(internal_error)?;

        return Ok(Json(TaskAttemptResponse {
            task_attempt_id: row.try_get("task_attempt_id").map_err(internal_error)?,
            task_id: row.try_get("task_id").map_err(internal_error)?,
            attempt_index: row.try_get("attempt_index").map_err(internal_error)?,
            lease_owner: row.try_get("lease_owner").map_err(internal_error)?,
            status: row.try_get("status").map_err(internal_error)?,
            started_at: started_at.map(|t| t.to_rfc3339()),
            finished_at: finished_at.map(|t| t.to_rfc3339()),
            duplicated: true,
        }));
    }

    // Server derives attempt_index and status (BND-003)
    let attempt_index: i32 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(attempt_index), -1) + 1 FROM task_attempts WHERE task_id = $1",
    )
    .bind(&req.task_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    let status = "running";

    let row = sqlx::query(
        r#"insert into task_attempts (task_attempt_id, task_id, attempt_index, lease_owner, status, started_at)
           values ($1, $2, $3, $4, $5, now())
           returning task_attempt_id, task_id, attempt_index, lease_owner, status, started_at, finished_at"#,
    )
    .bind(&attempt_id)
    .bind(&req.task_id)
    .bind(attempt_index)
    .bind(&req.lease_owner)
    .bind(status)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    sqlx::query(
        r#"insert into event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           values ($1, 'task_attempt', $2, 'task_attempt_started', $3, $4::jsonb, now())
           on conflict (aggregate_kind, aggregate_id, idempotency_key) do nothing"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&attempt_id)
    .bind(&req.idempotency_key)
    .bind(serde_json::json!({
        "task_attempt_id": attempt_id,
        "task_id": req.task_id,
        "attempt_index": attempt_index,
        "status": status,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    let started_at: Option<chrono::DateTime<chrono::Utc>> = row.try_get("started_at").map_err(internal_error)?;
    let finished_at: Option<chrono::DateTime<chrono::Utc>> = row.try_get("finished_at").map_err(internal_error)?;

    Ok(Json(TaskAttemptResponse {
        task_attempt_id: row.try_get("task_attempt_id").map_err(internal_error)?,
        task_id: row.try_get("task_id").map_err(internal_error)?,
        attempt_index: row.try_get("attempt_index").map_err(internal_error)?,
        lease_owner: row.try_get("lease_owner").map_err(internal_error)?,
        status: row.try_get("status").map_err(internal_error)?,
        started_at: started_at.map(|t| t.to_rfc3339()),
        finished_at: finished_at.map(|t| t.to_rfc3339()),
        duplicated: false,
    }))
}

#[utoipa::path(
    get,
    path = "/api/task-attempts",
    responses(
        (status = 200, description = "List of task attempts", body = Vec<TaskAttemptResponse>)
    )
)]
pub async fn list_task_attempts(
    State(state): State<AppState>,
) -> ApiResult<Vec<TaskAttemptResponse>> {
    let rows = sqlx::query(
        "select task_attempt_id, task_id, attempt_index, lease_owner, status, started_at, finished_at from task_attempts order by started_at desc",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let started_at: Option<chrono::DateTime<chrono::Utc>> = row.try_get("started_at").map_err(internal_error)?;
        let finished_at: Option<chrono::DateTime<chrono::Utc>> = row.try_get("finished_at").map_err(internal_error)?;
        results.push(TaskAttemptResponse {
            task_attempt_id: row.try_get("task_attempt_id").map_err(internal_error)?,
            task_id: row.try_get("task_id").map_err(internal_error)?,
            attempt_index: row.try_get("attempt_index").map_err(internal_error)?,
            lease_owner: row.try_get("lease_owner").map_err(internal_error)?,
            status: row.try_get("status").map_err(internal_error)?,
            started_at: started_at.map(|t| t.to_rfc3339()),
            finished_at: finished_at.map(|t| t.to_rfc3339()),
            duplicated: false,
        });
    }

    Ok(Json(results))
}
