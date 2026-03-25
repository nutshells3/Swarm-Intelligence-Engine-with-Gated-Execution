//! Metrics endpoints (Tier 3).
//!
//! Each endpoint computes metrics on-the-fly from existing tables using
//! SQL aggregations. No separate metrics store is maintained.

use axum::extract::State;
use axum::response::Json;
use serde::Serialize;
use sqlx::Row;

use crate::error::{ApiResult, internal_error};
use crate::state::AppState;

#[derive(Serialize, utoipa::ToSchema)]
pub struct CycleMetric {
    pub cycle_id: String,
    pub phase: String,
    pub created_at: String,
    pub updated_at: String,
    pub duration_ms: Option<i64>,
    pub tasks_completed: i64,
    pub tasks_failed: i64,
    pub tasks_queued: i64,
    pub tasks_running: i64,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TaskMetrics {
    pub total: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub queued: i64,
    pub running: i64,
    pub timed_out: i64,
    pub cancelled: i64,
    pub success_rate: f64,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CostMetric {
    pub total_invocations: i64,
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TokenMetrics {
    pub total_input_tokens: i64,
    pub total_output_tokens: i64,
    pub total_tokens: i64,
    pub average_input_per_attempt: f64,
    pub average_output_per_attempt: f64,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct WorkerMetric {
    pub worker_role: String,
    pub total_attempts: i64,
    pub succeeded: i64,
    pub failed: i64,
    pub success_rate: f64,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SaturationMetrics {
    pub queued_tasks: i64,
    pub running_tasks: i64,
    pub active_workers: i64,
    pub queue_pressure: f64,
    /// Worker success rate summary (from worker_success_rates table).
    /// Null when no data exists yet.
    pub worker_success_rate: Option<f64>,
    /// Total worker attempts across all roles. Zero when no data.
    pub worker_total_attempts: i64,
}

#[utoipa::path(
    get,
    path = "/api/metrics/cycles",
    responses(
        (status = 200, description = "Cycle metrics", body = Vec<CycleMetric>)
    )
)]
pub async fn cycle_metrics(
    State(state): State<AppState>,
) -> ApiResult<Vec<CycleMetric>> {
    let rows = sqlx::query(
        r#"SELECT c.cycle_id, c.phase, c.created_at, c.updated_at,
                  (EXTRACT(EPOCH FROM (c.updated_at - c.created_at)) * 1000)::bigint as duration_ms,
                  COUNT(t.task_id) FILTER (WHERE t.status = 'succeeded') as tasks_completed,
                  COUNT(t.task_id) FILTER (WHERE t.status = 'failed') as tasks_failed,
                  COUNT(t.task_id) FILTER (WHERE t.status = 'queued') as tasks_queued,
                  COUNT(t.task_id) FILTER (WHERE t.status = 'running') as tasks_running
           FROM cycles c
           LEFT JOIN loops l ON c.loop_id = l.loop_id
           LEFT JOIN nodes n ON l.objective_id = n.objective_id
           LEFT JOIN tasks t ON n.node_id = t.node_id
           GROUP BY c.cycle_id, c.phase, c.created_at, c.updated_at
           ORDER BY c.created_at DESC
           LIMIT 100"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut results = Vec::with_capacity(rows.len());
    for row in &rows {
        let created_at: chrono::DateTime<chrono::Utc> =
            row.try_get("created_at").map_err(internal_error)?;
        let updated_at: chrono::DateTime<chrono::Utc> =
            row.try_get("updated_at").map_err(internal_error)?;

        results.push(CycleMetric {
            cycle_id: row.try_get("cycle_id").map_err(internal_error)?,
            phase: row.try_get("phase").map_err(internal_error)?,
            created_at: created_at.to_rfc3339(),
            updated_at: updated_at.to_rfc3339(),
            duration_ms: row.try_get("duration_ms").map_err(internal_error)?,
            tasks_completed: row.try_get("tasks_completed").map_err(internal_error)?,
            tasks_failed: row.try_get("tasks_failed").map_err(internal_error)?,
            tasks_queued: row.try_get("tasks_queued").map_err(internal_error)?,
            tasks_running: row.try_get("tasks_running").map_err(internal_error)?,
        });
    }

    Ok(Json(results))
}

#[utoipa::path(
    get,
    path = "/api/metrics/tasks",
    responses(
        (status = 200, description = "Task metrics", body = TaskMetrics)
    )
)]
pub async fn task_metrics(
    State(state): State<AppState>,
) -> ApiResult<TaskMetrics> {
    let row = sqlx::query(
        r#"SELECT
             COUNT(*) as total,
             COUNT(*) FILTER (WHERE status = 'succeeded') as succeeded,
             COUNT(*) FILTER (WHERE status = 'failed') as failed,
             COUNT(*) FILTER (WHERE status = 'queued') as queued,
             COUNT(*) FILTER (WHERE status = 'running') as running,
             COUNT(*) FILTER (WHERE status = 'timed_out') as timed_out,
             COUNT(*) FILTER (WHERE status = 'cancelled') as cancelled
           FROM tasks"#,
    )
    .fetch_one(&state.pool)
    .await
    .map_err(internal_error)?;

    let total: i64 = row.try_get("total").map_err(internal_error)?;
    let succeeded: i64 = row.try_get("succeeded").map_err(internal_error)?;
    let failed: i64 = row.try_get("failed").map_err(internal_error)?;
    let success_rate = if total > 0 {
        succeeded as f64 / total as f64
    } else {
        0.0
    };

    Ok(Json(TaskMetrics {
        total,
        succeeded,
        failed,
        queued: row.try_get("queued").map_err(internal_error)?,
        running: row.try_get("running").map_err(internal_error)?,
        timed_out: row.try_get("timed_out").map_err(internal_error)?,
        cancelled: row.try_get("cancelled").map_err(internal_error)?,
        success_rate,
    }))
}

#[utoipa::path(
    get,
    path = "/api/metrics/costs",
    responses(
        (status = 200, description = "Cost metrics", body = CostMetric)
    )
)]
pub async fn cost_metrics(
    State(state): State<AppState>,
) -> ApiResult<CostMetric> {
    let row = sqlx::query(
        r#"SELECT
             COUNT(*) as total_invocations,
             COALESCE(SUM(input_tokens), 0) as total_input_tokens,
             COALESCE(SUM(output_tokens), 0) as total_output_tokens
           FROM token_records"#,
    )
    .fetch_one(&state.pool)
    .await
    .map_err(internal_error)?;

    Ok(Json(CostMetric {
        total_invocations: row.try_get("total_invocations").map_err(internal_error)?,
        total_input_tokens: row.try_get("total_input_tokens").map_err(internal_error)?,
        total_output_tokens: row.try_get("total_output_tokens").map_err(internal_error)?,
    }))
}

#[utoipa::path(
    get,
    path = "/api/metrics/tokens",
    responses(
        (status = 200, description = "Token metrics", body = TokenMetrics)
    )
)]
pub async fn token_metrics(
    State(state): State<AppState>,
) -> ApiResult<TokenMetrics> {
    let row = sqlx::query(
        r#"SELECT
             COALESCE(SUM(input_tokens), 0) as total_input_tokens,
             COALESCE(SUM(output_tokens), 0) as total_output_tokens,
             COALESCE(SUM(input_tokens), 0) + COALESCE(SUM(output_tokens), 0) as total_tokens,
             CASE WHEN COUNT(*) > 0
                  THEN COALESCE(SUM(input_tokens), 0)::float / COUNT(*)
                  ELSE 0
             END as average_input_per_attempt,
             CASE WHEN COUNT(*) > 0
                  THEN COALESCE(SUM(output_tokens), 0)::float / COUNT(*)
                  ELSE 0
             END as average_output_per_attempt
           FROM token_records"#,
    )
    .fetch_one(&state.pool)
    .await
    .map_err(internal_error)?;

    Ok(Json(TokenMetrics {
        total_input_tokens: row.try_get("total_input_tokens").map_err(internal_error)?,
        total_output_tokens: row.try_get("total_output_tokens").map_err(internal_error)?,
        total_tokens: row.try_get("total_tokens").map_err(internal_error)?,
        average_input_per_attempt: row
            .try_get("average_input_per_attempt")
            .map_err(internal_error)?,
        average_output_per_attempt: row
            .try_get("average_output_per_attempt")
            .map_err(internal_error)?,
    }))
}

#[utoipa::path(
    get,
    path = "/api/metrics/workers",
    responses(
        (status = 200, description = "Worker metrics", body = Vec<WorkerMetric>)
    )
)]
pub async fn worker_metrics(
    State(state): State<AppState>,
) -> ApiResult<Vec<WorkerMetric>> {
    let rows = sqlx::query(
        r#"SELECT
             t.worker_role,
             COUNT(ta.task_attempt_id) as total_attempts,
             COUNT(ta.task_attempt_id) FILTER (WHERE ta.status = 'succeeded') as succeeded,
             COUNT(ta.task_attempt_id) FILTER (WHERE ta.status = 'failed') as failed
           FROM tasks t
           LEFT JOIN task_attempts ta ON t.task_id = ta.task_id
           GROUP BY t.worker_role
           ORDER BY t.worker_role"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut results = Vec::with_capacity(rows.len());
    for row in &rows {
        let total: i64 = row.try_get("total_attempts").map_err(internal_error)?;
        let succeeded: i64 = row.try_get("succeeded").map_err(internal_error)?;
        let success_rate = if total > 0 {
            succeeded as f64 / total as f64
        } else {
            0.0
        };

        results.push(WorkerMetric {
            worker_role: row.try_get("worker_role").map_err(internal_error)?,
            total_attempts: total,
            succeeded,
            failed: row.try_get("failed").map_err(internal_error)?,
            success_rate,
        });
    }

    Ok(Json(results))
}

#[utoipa::path(
    get,
    path = "/api/metrics/saturation",
    responses(
        (status = 200, description = "Saturation metrics", body = SaturationMetrics)
    )
)]
pub async fn saturation_metrics(
    State(state): State<AppState>,
) -> ApiResult<SaturationMetrics> {
    let task_row = sqlx::query(
        r#"SELECT
             COUNT(*) FILTER (WHERE status = 'queued') as queued_tasks,
             COUNT(*) FILTER (WHERE status = 'running') as running_tasks
           FROM tasks"#,
    )
    .fetch_one(&state.pool)
    .await
    .map_err(internal_error)?;

    let queued: i64 = task_row.try_get("queued_tasks").map_err(internal_error)?;
    let running: i64 = task_row.try_get("running_tasks").map_err(internal_error)?;

    let active_workers = running; // proxy: one worker per running task
    let queue_pressure = if running > 0 {
        queued as f64 / running as f64
    } else if queued > 0 {
        f64::INFINITY
    } else {
        0.0
    };

    let wsr_row = sqlx::query(
        r#"SELECT
             COALESCE(SUM(total_attempts), 0) as total_attempts,
             COALESCE(SUM(successes), 0) as successes,
             COALESCE(SUM(failures), 0) as failures
           FROM worker_success_rates"#,
    )
    .fetch_one(&state.pool)
    .await
    .map_err(internal_error)?;

    let wsr_total: i64 = wsr_row.try_get("total_attempts").map_err(internal_error)?;
    let wsr_successes: i64 = wsr_row.try_get("successes").map_err(internal_error)?;
    let (worker_success_rate, worker_total_attempts) = if wsr_total > 0 {
        (Some(wsr_successes as f64 / wsr_total as f64), wsr_total)
    } else {
        (None, 0)
    };

    Ok(Json(SaturationMetrics {
        queued_tasks: queued,
        running_tasks: running,
        active_workers,
        queue_pressure,
        worker_success_rate,
        worker_total_attempts,
    }))
}
