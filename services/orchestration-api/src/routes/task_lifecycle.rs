//! Task lifecycle endpoints: complete, fail, patch, and attempt completion.
//!
//! These endpoints allow workers and agents to report back the results
//! of task execution, closing the loop between dispatch and integration.

use axum::extract::{Path, State};
use axum::response::Json;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::error::{ApiResult, bad_request, internal_error, not_found};
use crate::state::AppState;

/// Returns `true` when `from -> to` is a legal task status transition.
fn is_valid_task_transition(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("queued", "running")
            | ("running", "succeeded")
            | ("running", "failed")
            | ("running", "cancelled")
            | ("queued", "cancelled")
            | ("failed", "queued") // retry
    )
}

/// Returns `true` when `from -> to` is a legal attempt status transition.
fn is_valid_attempt_transition(from: &str, to: &str) -> bool {
    matches!(
        (from, to),
        ("running", "succeeded")
            | ("running", "failed")
            | ("running", "cancelled")
    )
}

/// Map task status to the corresponding node lifecycle value.
fn node_lifecycle_for_task_status(status: &str) -> &'static str {
    match status {
        "succeeded" => "done",
        "failed" => "failed",
        "cancelled" => "cancelled",
        "running" => "running",
        "queued" => "queued",
        _ => "proposed",
    }
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CompleteTaskRequest {
    /// Freeform output summary produced by the worker.
    pub output: Option<String>,
    /// Artifact references (URIs) produced during execution.
    #[serde(default)]
    pub artifacts: Vec<ArtifactEntry>,
}

#[derive(Deserialize, Serialize, Clone, utoipa::ToSchema)]
pub struct ArtifactEntry {
    pub artifact_kind: String,
    pub artifact_uri: String,
    #[serde(default)]
    pub metadata: serde_json::Value,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TaskLifecycleResponse {
    pub task_id: String,
    pub status: String,
    pub previous_status: String,
    pub node_id: String,
    pub node_lifecycle: String,
    pub artifacts_stored: usize,
}

#[utoipa::path(
    post,
    path = "/api/tasks/{id}/complete",
    params(("id" = String, Path, description = "Task ID")),
    request_body = CompleteTaskRequest,
    responses(
        (status = 200, description = "Completed task", body = TaskLifecycleResponse)
    )
)]
pub async fn complete_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(req): Json<CompleteTaskRequest>,
) -> ApiResult<TaskLifecycleResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;

    // Fetch current task
    let task_row = sqlx::query(
        "SELECT task_id, node_id, status FROM tasks WHERE task_id = $1 FOR UPDATE",
    )
    .bind(&task_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    let Some(task_row) = task_row else {
        return Err(not_found("task not found"));
    };

    let current_status: String = task_row.try_get("status").map_err(internal_error)?;
    let node_id: String = task_row.try_get("node_id").map_err(internal_error)?;

    if !is_valid_task_transition(&current_status, "succeeded") {
        return Err(bad_request(&format!(
            "cannot transition task from '{}' to 'succeeded'",
            current_status
        )));
    }

    // Update task
    sqlx::query(
        "UPDATE tasks SET status = 'succeeded', updated_at = now() WHERE task_id = $1",
    )
    .bind(&task_id)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Update node lifecycle
    let new_node_lifecycle = node_lifecycle_for_task_status("succeeded");
    sqlx::query(
        "UPDATE nodes SET lifecycle = $1, updated_at = now() WHERE node_id = $2",
    )
    .bind(new_node_lifecycle)
    .bind(&node_id)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Complete the running attempt (if any)
    sqlx::query(
        "UPDATE task_attempts SET status = 'succeeded', finished_at = now()
         WHERE task_id = $1 AND status = 'running'",
    )
    .bind(&task_id)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Store artifact refs
    let mut artifacts_stored = 0usize;
    for artifact in &req.artifacts {
        let artifact_ref_id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO artifact_refs (artifact_ref_id, task_id, artifact_kind, artifact_uri, metadata)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(&artifact_ref_id)
        .bind(&task_id)
        .bind(&artifact.artifact_kind)
        .bind(&artifact.artifact_uri)
        .bind(&artifact.metadata)
        .execute(&mut *tx)
        .await
        .map_err(internal_error)?;
        artifacts_stored += 1;
    }

    // Emit event
    let event_id = Uuid::now_v7().to_string();
    let idempotency_key = format!("task-complete-{}", task_id);
    let payload = serde_json::json!({
        "task_id": task_id,
        "node_id": node_id,
        "from_status": current_status,
        "to_status": "succeeded",
        "output": req.output,
        "artifacts_stored": artifacts_stored,
        "trigger": "api_complete"
    });

    sqlx::query(
        "INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
         VALUES ($1, 'task', $2, 'task_completed', $3, $4, now())
         ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
    )
    .bind(&event_id)
    .bind(&task_id)
    .bind(&idempotency_key)
    .bind(&payload)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    // After task completion: check for newly unblocked dependent tasks
    if let Err(e) = unblock_dependent_tasks(&state.pool, &node_id).await {
        tracing::warn!(
            node_id,
            error = %e,
            "Dependency unblock check failed in complete_task (non-fatal)"
        );
    }

    Ok(Json(TaskLifecycleResponse {
        task_id,
        status: "succeeded".to_string(),
        previous_status: current_status,
        node_id,
        node_lifecycle: new_node_lifecycle.to_string(),
        artifacts_stored,
    }))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct FailTaskRequest {
    /// Human-readable error message.
    pub error_message: Option<String>,
    /// Machine-readable error code.
    pub error_code: Option<String>,
}

#[utoipa::path(
    post,
    path = "/api/tasks/{id}/fail",
    params(("id" = String, Path, description = "Task ID")),
    request_body = FailTaskRequest,
    responses(
        (status = 200, description = "Failed task", body = TaskLifecycleResponse)
    )
)]
pub async fn fail_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(req): Json<FailTaskRequest>,
) -> ApiResult<TaskLifecycleResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;

    // Fetch current task
    let task_row = sqlx::query(
        "SELECT task_id, node_id, status FROM tasks WHERE task_id = $1 FOR UPDATE",
    )
    .bind(&task_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    let Some(task_row) = task_row else {
        return Err(not_found("task not found"));
    };

    let current_status: String = task_row.try_get("status").map_err(internal_error)?;
    let node_id: String = task_row.try_get("node_id").map_err(internal_error)?;

    if !is_valid_task_transition(&current_status, "failed") {
        return Err(bad_request(&format!(
            "cannot transition task from '{}' to 'failed'",
            current_status
        )));
    }

    // Update task
    sqlx::query(
        "UPDATE tasks SET status = 'failed', updated_at = now() WHERE task_id = $1",
    )
    .bind(&task_id)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Update node lifecycle
    let new_node_lifecycle = node_lifecycle_for_task_status("failed");
    sqlx::query(
        "UPDATE nodes SET lifecycle = $1, updated_at = now() WHERE node_id = $2",
    )
    .bind(new_node_lifecycle)
    .bind(&node_id)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Fail the running attempt (if any)
    sqlx::query(
        "UPDATE task_attempts SET status = 'failed', finished_at = now()
         WHERE task_id = $1 AND status = 'running'",
    )
    .bind(&task_id)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Emit event
    let event_id = Uuid::now_v7().to_string();
    let idempotency_key = format!("task-fail-{}", task_id);
    let payload = serde_json::json!({
        "task_id": task_id,
        "node_id": node_id,
        "from_status": current_status,
        "to_status": "failed",
        "error_message": req.error_message,
        "error_code": req.error_code,
        "trigger": "api_fail"
    });

    sqlx::query(
        "INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
         VALUES ($1, 'task', $2, 'task_failed', $3, $4, now())
         ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
    )
    .bind(&event_id)
    .bind(&task_id)
    .bind(&idempotency_key)
    .bind(&payload)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    Ok(Json(TaskLifecycleResponse {
        task_id,
        status: "failed".to_string(),
        previous_status: current_status,
        node_id,
        node_lifecycle: new_node_lifecycle.to_string(),
        artifacts_stored: 0,
    }))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PatchTaskRequest {
    /// Target status to transition to.
    pub status: String,
    /// Optional output / reason.
    pub output: Option<String>,
    /// Optional artifact references.
    #[serde(default)]
    pub artifacts: Vec<ArtifactEntry>,
}

#[utoipa::path(
    patch,
    path = "/api/tasks/{id}",
    params(("id" = String, Path, description = "Task ID")),
    request_body = PatchTaskRequest,
    responses(
        (status = 200, description = "Patched task", body = TaskLifecycleResponse)
    )
)]
pub async fn patch_task(
    State(state): State<AppState>,
    Path(task_id): Path<String>,
    Json(req): Json<PatchTaskRequest>,
) -> ApiResult<TaskLifecycleResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;

    // Fetch current task
    let task_row = sqlx::query(
        "SELECT task_id, node_id, status FROM tasks WHERE task_id = $1 FOR UPDATE",
    )
    .bind(&task_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    let Some(task_row) = task_row else {
        return Err(not_found("task not found"));
    };

    let current_status: String = task_row.try_get("status").map_err(internal_error)?;
    let node_id: String = task_row.try_get("node_id").map_err(internal_error)?;

    if !is_valid_task_transition(&current_status, &req.status) {
        return Err(bad_request(&format!(
            "cannot transition task from '{}' to '{}'",
            current_status, req.status
        )));
    }

    // Update task status
    sqlx::query(
        "UPDATE tasks SET status = $1, updated_at = now() WHERE task_id = $2",
    )
    .bind(&req.status)
    .bind(&task_id)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Update node lifecycle
    let new_node_lifecycle = node_lifecycle_for_task_status(&req.status).to_string();
    sqlx::query(
        "UPDATE nodes SET lifecycle = $1, updated_at = now() WHERE node_id = $2",
    )
    .bind(&new_node_lifecycle)
    .bind(&node_id)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    // If task is terminal, finish the running attempt
    if req.status == "succeeded" || req.status == "failed" || req.status == "cancelled" {
        sqlx::query(
            "UPDATE task_attempts SET status = $1, finished_at = now()
             WHERE task_id = $2 AND status = 'running'",
        )
        .bind(&req.status)
        .bind(&task_id)
        .execute(&mut *tx)
        .await
        .map_err(internal_error)?;
    }

    // Store artifact refs
    let mut artifacts_stored = 0usize;
    for artifact in &req.artifacts {
        let artifact_ref_id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO artifact_refs (artifact_ref_id, task_id, artifact_kind, artifact_uri, metadata)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(&artifact_ref_id)
        .bind(&task_id)
        .bind(&artifact.artifact_kind)
        .bind(&artifact.artifact_uri)
        .bind(&artifact.metadata)
        .execute(&mut *tx)
        .await
        .map_err(internal_error)?;
        artifacts_stored += 1;
    }

    // Emit event
    let event_id = Uuid::now_v7().to_string();
    let idempotency_key = format!("task-patch-{}-{}", task_id, req.status);
    let payload = serde_json::json!({
        "task_id": task_id,
        "node_id": node_id,
        "from_status": current_status,
        "to_status": req.status,
        "output": req.output,
        "artifacts_stored": artifacts_stored,
        "trigger": "api_patch"
    });

    sqlx::query(
        "INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
         VALUES ($1, 'task', $2, 'task_status_changed', $3, $4, now())
         ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
    )
    .bind(&event_id)
    .bind(&task_id)
    .bind(&idempotency_key)
    .bind(&payload)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    // After task succeeds: check for newly unblocked dependent tasks
    if req.status == "succeeded" {
        if let Err(e) = unblock_dependent_tasks(&state.pool, &node_id).await {
            tracing::warn!(
                node_id,
                error = %e,
                "Dependency unblock check failed in patch_task (non-fatal)"
            );
        }
    }

    Ok(Json(TaskLifecycleResponse {
        task_id,
        status: req.status,
        previous_status: current_status,
        node_id,
        node_lifecycle: new_node_lifecycle,
        artifacts_stored,
    }))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CompleteAttemptRequest {
    /// Final status for the attempt: "succeeded" or "failed".
    pub status: String,
    /// Freeform output summary.
    pub output: Option<String>,
    /// Artifact references produced during this attempt.
    #[serde(default)]
    pub artifacts: Vec<ArtifactEntry>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct AttemptLifecycleResponse {
    pub task_attempt_id: String,
    pub task_id: String,
    pub attempt_status: String,
    pub previous_attempt_status: String,
    pub task_status: String,
    pub node_lifecycle: String,
    pub artifacts_stored: usize,
}

#[utoipa::path(
    post,
    path = "/api/task-attempts/{attempt_id}/complete",
    params(("attempt_id" = String, Path, description = "Task attempt ID")),
    request_body = CompleteAttemptRequest,
    responses(
        (status = 200, description = "Completed attempt", body = AttemptLifecycleResponse)
    )
)]
pub async fn complete_attempt(
    State(state): State<AppState>,
    Path(attempt_id): Path<String>,
    Json(req): Json<CompleteAttemptRequest>,
) -> ApiResult<AttemptLifecycleResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;

    // Fetch current attempt
    let attempt_row = sqlx::query(
        "SELECT task_attempt_id, task_id, status FROM task_attempts WHERE task_attempt_id = $1 FOR UPDATE",
    )
    .bind(&attempt_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    let Some(attempt_row) = attempt_row else {
        return Err(not_found("task attempt not found"));
    };

    let current_attempt_status: String =
        attempt_row.try_get("status").map_err(internal_error)?;
    let task_id: String = attempt_row.try_get("task_id").map_err(internal_error)?;

    if !is_valid_attempt_transition(&current_attempt_status, &req.status) {
        return Err(bad_request(&format!(
            "cannot transition attempt from '{}' to '{}'",
            current_attempt_status, req.status
        )));
    }

    // Update the attempt
    sqlx::query(
        "UPDATE task_attempts SET status = $1, finished_at = now()
         WHERE task_attempt_id = $2",
    )
    .bind(&req.status)
    .bind(&attempt_id)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Propagate the attempt result to the parent task
    let task_row = sqlx::query(
        "SELECT node_id, status FROM tasks WHERE task_id = $1 FOR UPDATE",
    )
    .bind(&task_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    let task_status: String = task_row.try_get("status").map_err(internal_error)?;
    let node_id: String = task_row.try_get("node_id").map_err(internal_error)?;

    // Determine new task status: propagate attempt result if task is running
    let new_task_status = if task_status == "running" {
        req.status.clone()
    } else {
        task_status.clone()
    };

    if new_task_status != task_status {
        sqlx::query(
            "UPDATE tasks SET status = $1, updated_at = now() WHERE task_id = $2",
        )
        .bind(&new_task_status)
        .bind(&task_id)
        .execute(&mut *tx)
        .await
        .map_err(internal_error)?;

        // Update node lifecycle
        let new_node_lifecycle = node_lifecycle_for_task_status(&new_task_status);
        sqlx::query(
            "UPDATE nodes SET lifecycle = $1, updated_at = now() WHERE node_id = $2",
        )
        .bind(new_node_lifecycle)
        .bind(&node_id)
        .execute(&mut *tx)
        .await
        .map_err(internal_error)?;
    }

    // Store artifacts
    let mut artifacts_stored = 0usize;
    for artifact in &req.artifacts {
        let artifact_ref_id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO artifact_refs (artifact_ref_id, task_id, artifact_kind, artifact_uri, metadata)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(&artifact_ref_id)
        .bind(&task_id)
        .bind(&artifact.artifact_kind)
        .bind(&artifact.artifact_uri)
        .bind(&artifact.metadata)
        .execute(&mut *tx)
        .await
        .map_err(internal_error)?;
        artifacts_stored += 1;
    }

    // Emit event
    let event_id = Uuid::now_v7().to_string();
    let idempotency_key = format!("attempt-complete-{}", attempt_id);
    let payload = serde_json::json!({
        "task_attempt_id": attempt_id,
        "task_id": task_id,
        "node_id": node_id,
        "from_attempt_status": current_attempt_status,
        "to_attempt_status": req.status,
        "task_status": new_task_status,
        "output": req.output,
        "artifacts_stored": artifacts_stored,
        "trigger": "api_attempt_complete"
    });

    sqlx::query(
        "INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
         VALUES ($1, 'task_attempt', $2, 'task_attempt_completed', $3, $4, now())
         ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
    )
    .bind(&event_id)
    .bind(&attempt_id)
    .bind(&idempotency_key)
    .bind(&payload)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    // After task succeeds via attempt completion: check for newly unblocked dependent tasks
    if new_task_status == "succeeded" {
        if let Err(e) = unblock_dependent_tasks(&state.pool, &node_id).await {
            tracing::warn!(
                node_id,
                error = %e,
                "Dependency unblock check failed in complete_attempt (non-fatal)"
            );
        }
    }

    let node_lifecycle = node_lifecycle_for_task_status(&new_task_status).to_string();

    Ok(Json(AttemptLifecycleResponse {
        task_attempt_id: attempt_id,
        task_id,
        attempt_status: req.status,
        previous_attempt_status: current_attempt_status,
        task_status: new_task_status,
        node_lifecycle,
        artifacts_stored,
    }))
}

/// After a node completes, find dependent tasks that are now unblocked
/// (all their predecessor nodes are done) and set them to 'running'
/// so worker-dispatch picks them up.
async fn unblock_dependent_tasks(
    pool: &sqlx::PgPool,
    completed_node_id: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let unblocked = sqlx::query(
        r#"
        SELECT DISTINCT t.task_id
        FROM node_edges ne
        JOIN nodes blocked_node ON ne.to_node_id = blocked_node.node_id
        JOIN tasks t ON t.node_id = blocked_node.node_id
        WHERE ne.from_node_id = $1
          AND t.status = 'queued'
          AND NOT EXISTS (
              SELECT 1 FROM node_edges ne2
              JOIN nodes pred ON ne2.from_node_id = pred.node_id
              WHERE ne2.to_node_id = blocked_node.node_id
                AND ne2.edge_kind IN ('depends_on', 'blocks')
                AND pred.lifecycle NOT IN ('admitted', 'done', 'completed')
          )
        "#,
    )
    .bind(completed_node_id)
    .fetch_all(pool)
    .await?;

    for row in unblocked {
        let unblocked_task_id: String = row.try_get("task_id").map_err(|e| {
            Box::new(e) as Box<dyn std::error::Error>
        })?;

        sqlx::query("UPDATE tasks SET status = 'running', updated_at = now() WHERE task_id = $1")
            .bind(&unblocked_task_id)
            .execute(pool)
            .await?;

        // Also update the node lifecycle to running
        sqlx::query(
            "UPDATE nodes SET lifecycle = 'running', updated_at = now() \
             WHERE node_id = (SELECT node_id FROM tasks WHERE task_id = $1) \
               AND lifecycle IN ('proposed', 'queued')",
        )
        .bind(&unblocked_task_id)
        .execute(pool)
        .await?;

        tracing::info!(task_id = %unblocked_task_id, "Unblocked dependent task");
    }

    Ok(())
}
