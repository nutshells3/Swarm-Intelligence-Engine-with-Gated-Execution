//! Runtime command executor (CTL-001~006, CTL-010~013, CTL-015).
//!
//! This module provides concrete SQL-backed implementations for the 11
//! command types defined in [`crate::commands`].  Each function follows
//! the same discipline used by `tick.rs` and the API route handlers:
//!
//!   1. Check idempotency via `event_journal` (BND-010).
//!   2. Execute the authoritative INSERT / UPDATE.
//!   3. Append an `event_journal` entry.
//!   4. Return [`CommandOutcome`] (success / idempotent-skip) or
//!      [`CommandRejection`] (precondition failure).
//!
//! All functions accept a `&mut sqlx::Transaction` so the caller owns
//! the transaction boundary.  This allows tick.rs and the API routes
//! to compose multiple commands inside a single commit.
//!
//! # Feature gate
//!
//! This module is compiled only when the `runtime` feature is active:
//!
//! ```toml
//! control-plane = { path = "../../packages/control-plane", features = ["runtime"] }
//! ```

use chrono::Utc;
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::commands::{
    CommandOutcome, CommandRejection, CommandRejectionReason, CommandResult,
    CreateCycleCommand, CreateLoopCommand, CreateNodeFromPlanCommand,
    CreateObjectiveCommand, CreateTaskFromNodeCommand, DispatchSchedulerCommand,
    FailureIngestionCommand, NextCycleGenerationCommand, RetrySchedulingCommand,
    TaskCompletionCommand, TimeoutIngestionCommand,
};

/// Check the scoped idempotency registry.
///
/// Returns `Some(aggregate_id)` when a matching event already exists.
async fn idempotency_check(
    tx: &mut Transaction<'_, Postgres>,
    aggregate_kind: &str,
    idempotency_key: &str,
) -> Result<Option<String>, sqlx::Error> {
    sqlx::query_scalar(
        "SELECT aggregate_id FROM event_journal \
         WHERE aggregate_kind = $1 AND idempotency_key = $2 LIMIT 1",
    )
    .bind(aggregate_kind)
    .bind(idempotency_key)
    .fetch_optional(tx.as_mut())
    .await
}

/// Append an event to the journal (ON CONFLICT guards races).
async fn append_event(
    tx: &mut Transaction<'_, Postgres>,
    aggregate_kind: &str,
    aggregate_id: &str,
    event_kind: &str,
    idempotency_key: &str,
    payload: &serde_json::Value,
) -> Result<String, sqlx::Error> {
    let event_id = Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO event_journal \
         (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
         VALUES ($1, $2, $3, $4, $5, $6, now()) \
         ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
    )
    .bind(&event_id)
    .bind(aggregate_kind)
    .bind(aggregate_id)
    .bind(event_kind)
    .bind(idempotency_key)
    .bind(payload)
    .execute(tx.as_mut())
    .await?;
    Ok(event_id)
}

/// Build a successful [`CommandOutcome`].
fn outcome_applied(event_id: String, message: impl Into<String>) -> CommandOutcome {
    CommandOutcome {
        applied: true,
        idempotent_skip: false,
        message: message.into(),
        event_ids: vec![event_id],
        executed_at: Utc::now(),
    }
}

/// Build an idempotent-skip [`CommandOutcome`].
fn outcome_skip(message: impl Into<String>) -> CommandOutcome {
    CommandOutcome {
        applied: false,
        idempotent_skip: true,
        message: message.into(),
        event_ids: vec![],
        executed_at: Utc::now(),
    }
}

/// Build a rejection.
fn reject(reason: CommandRejectionReason, detail: impl Into<String>) -> CommandRejection {
    CommandRejection {
        reason,
        detail: detail.into(),
    }
}


/// Execute [`CreateObjectiveCommand`].
///
/// Mirrors the SQL in `orchestration-api/src/routes/objectives.rs::create_objective`.
pub async fn execute_create_objective(
    tx: &mut Transaction<'_, Postgres>,
    cmd: &CreateObjectiveCommand,
) -> CommandResult {
    // Validation: non-empty summary
    if cmd.summary.trim().is_empty() {
        return Err(reject(
            CommandRejectionReason::ValidationError,
            "summary must not be empty",
        ));
    }
    if cmd.desired_outcome.trim().is_empty() {
        return Err(reject(
            CommandRejectionReason::ValidationError,
            "desired_outcome must not be empty",
        ));
    }

    // Idempotency check
    if let Some(_existing) = idempotency_check(tx, "objective", &cmd.idempotency_key).await
        .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?
    {
        return Ok(outcome_skip("objective already created with this idempotency key"));
    }

    let objective_id = Uuid::now_v7().to_string();

    sqlx::query(
        "INSERT INTO objectives \
         (objective_id, summary, planning_status, plan_gate, created_at, updated_at) \
         VALUES ($1, $2, 'planning', 'draft', now(), now())",
    )
    .bind(&objective_id)
    .bind(&cmd.summary)
    .execute(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    let event_id = append_event(
        tx,
        "objective",
        &objective_id,
        "objective_created",
        &cmd.idempotency_key,
        &serde_json::json!({
            "objective_id": objective_id,
            "summary": cmd.summary,
            "desired_outcome": cmd.desired_outcome,
            "source_conversation_id": cmd.source_conversation_id,
        }),
    )
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    tracing::info!(objective_id, "CTL-001: objective created");
    Ok(outcome_applied(event_id, format!("objective {} created", objective_id)))
}


/// Execute [`CreateLoopCommand`].
///
/// Mirrors the SQL in `tick.rs::create_loops_for_new_objectives`.
pub async fn execute_create_loop(
    tx: &mut Transaction<'_, Postgres>,
    cmd: &CreateLoopCommand,
) -> CommandResult {
    // Idempotency check
    if let Some(_existing) = idempotency_check(tx, "loop", &cmd.idempotency_key).await
        .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?
    {
        return Ok(outcome_skip("loop already created with this idempotency key"));
    }

    // Precondition: objective must exist
    let obj_exists: Option<String> = sqlx::query_scalar(
        "SELECT objective_id FROM objectives WHERE objective_id = $1",
    )
    .bind(&cmd.objective_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    if obj_exists.is_none() {
        return Err(reject(
            CommandRejectionReason::EntityNotFound,
            format!("objective {} not found", cmd.objective_id),
        ));
    }

    let loop_id = Uuid::now_v7().to_string();

    sqlx::query(
        "INSERT INTO loops \
         (loop_id, objective_id, cycle_index, active_track, created_at, updated_at) \
         VALUES ($1, $2, 0, $3, now(), now())",
    )
    .bind(&loop_id)
    .bind(&cmd.objective_id)
    .bind(&cmd.initial_track)
    .execute(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    let event_id = append_event(
        tx,
        "loop",
        &loop_id,
        "loop_created",
        &cmd.idempotency_key,
        &serde_json::json!({
            "loop_id": loop_id,
            "objective_id": cmd.objective_id,
            "active_track": cmd.initial_track,
            "trigger": "command",
        }),
    )
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    tracing::info!(loop_id, objective_id = %cmd.objective_id, "CTL-002: loop created");
    Ok(outcome_applied(event_id, format!("loop {} created", loop_id)))
}


/// Execute [`CreateCycleCommand`].
///
/// Mirrors the SQL in `tick.rs::create_cycles_for_active_loops`.
pub async fn execute_create_cycle(
    tx: &mut Transaction<'_, Postgres>,
    cmd: &CreateCycleCommand,
) -> CommandResult {
    // Idempotency check
    if let Some(_existing) = idempotency_check(tx, "cycle", &cmd.idempotency_key).await
        .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?
    {
        return Ok(outcome_skip("cycle already created with this idempotency key"));
    }

    // Precondition: loop must exist
    let loop_exists: Option<String> = sqlx::query_scalar(
        "SELECT loop_id FROM loops WHERE loop_id = $1",
    )
    .bind(&cmd.loop_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    if loop_exists.is_none() {
        return Err(reject(
            CommandRejectionReason::EntityNotFound,
            format!("loop {} not found", cmd.loop_id),
        ));
    }

    let cycle_id = Uuid::now_v7().to_string();

    sqlx::query(
        "INSERT INTO cycles \
         (cycle_id, loop_id, cycle_index, phase, policy_snapshot_id, created_at, updated_at) \
         VALUES ($1, $2, $3, 'intake', $4, now(), now())",
    )
    .bind(&cycle_id)
    .bind(&cmd.loop_id)
    .bind(cmd.cycle_index)
    .bind(&cmd.policy_snapshot_id)
    .execute(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    // Bump cycle_index on the parent loop
    sqlx::query(
        "UPDATE loops SET cycle_index = $1, updated_at = now() WHERE loop_id = $2",
    )
    .bind(cmd.cycle_index)
    .bind(&cmd.loop_id)
    .execute(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    let event_id = append_event(
        tx,
        "cycle",
        &cycle_id,
        "cycle_created",
        &cmd.idempotency_key,
        &serde_json::json!({
            "cycle_id": cycle_id,
            "loop_id": cmd.loop_id,
            "cycle_index": cmd.cycle_index,
            "policy_snapshot_id": cmd.policy_snapshot_id,
            "initial_phase": "intake",
        }),
    )
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    tracing::info!(cycle_id, loop_id = %cmd.loop_id, "CTL-003: cycle created");
    Ok(outcome_applied(event_id, format!("cycle {} created", cycle_id)))
}


/// Execute [`CreateNodeFromPlanCommand`].
///
/// Mirrors the SQL in `tick.rs::bridge_milestones_to_nodes`.
pub async fn execute_create_node_from_plan(
    tx: &mut Transaction<'_, Postgres>,
    cmd: &CreateNodeFromPlanCommand,
) -> CommandResult {
    let idempotency_key_node = format!("milestone-bridge-{}", cmd.milestone_id);

    // Idempotency check
    if let Some(_existing) = idempotency_check(tx, "node", &idempotency_key_node).await
        .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?
    {
        return Ok(outcome_skip("node already bridged for this milestone"));
    }

    // Precondition: objective must exist
    let obj_exists: Option<String> = sqlx::query_scalar(
        "SELECT objective_id FROM objectives WHERE objective_id = $1",
    )
    .bind(&cmd.objective_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    if obj_exists.is_none() {
        return Err(reject(
            CommandRejectionReason::EntityNotFound,
            format!("objective {} not found", cmd.objective_id),
        ));
    }

    let node_id = Uuid::now_v7().to_string();
    let lane_str = serde_json::to_string(&cmd.initial_lane)
        .unwrap_or_else(|_| "\"implementation\"".to_string());
    let lane_str = lane_str.trim_matches('"');
    let lifecycle_str = serde_json::to_string(&cmd.initial_lifecycle)
        .unwrap_or_else(|_| "\"proposed\"".to_string());
    let lifecycle_str = lifecycle_str.trim_matches('"');

    let statement = format!("[milestone:{}] {}", cmd.milestone_id, cmd.statement);

    sqlx::query(
        "INSERT INTO nodes \
         (node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at, revision) \
         VALUES ($1, $2, $3, $4, $5, $6, now(), now(), 1) \
         ON CONFLICT DO NOTHING",
    )
    .bind(&node_id)
    .bind(&cmd.objective_id)
    .bind(&cmd.title)
    .bind(&statement)
    .bind(lane_str)
    .bind(lifecycle_str)
    .execute(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    let event_id = append_event(
        tx,
        "node",
        &node_id,
        "milestone_bridged",
        &idempotency_key_node,
        &serde_json::json!({
            "milestone_id": cmd.milestone_id,
            "objective_id": cmd.objective_id,
            "node_id": node_id,
            "bridge": "CTL-004",
        }),
    )
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    tracing::info!(node_id, milestone_id = %cmd.milestone_id, "CTL-004: node bridged from milestone");
    Ok(outcome_applied(event_id, format!("node {} created from milestone {}", node_id, cmd.milestone_id)))
}


/// Execute [`CreateTaskFromNodeCommand`].
///
/// Mirrors the SQL in `tick.rs::create_tasks_for_objective`.
pub async fn execute_create_task_from_node(
    tx: &mut Transaction<'_, Postgres>,
    cmd: &CreateTaskFromNodeCommand,
) -> CommandResult {
    // Idempotency check
    if let Some(_existing) = idempotency_check(tx, "task", &cmd.idempotency_key).await
        .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?
    {
        return Ok(outcome_skip("task already created with this idempotency key"));
    }

    // Precondition: node must exist
    let node_exists: Option<String> = sqlx::query_scalar(
        "SELECT node_id FROM nodes WHERE node_id = $1",
    )
    .bind(&cmd.node_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    if node_exists.is_none() {
        return Err(reject(
            CommandRejectionReason::EntityNotFound,
            format!("node {} not found", cmd.node_id),
        ));
    }

    let task_id = Uuid::now_v7().to_string();
    let status_str = serde_json::to_string(&cmd.initial_status)
        .unwrap_or_else(|_| "\"queued\"".to_string());
    let status_str = status_str.trim_matches('"');

    sqlx::query(
        "INSERT INTO tasks \
         (task_id, node_id, worker_role, skill_pack_id, status, created_at, updated_at) \
         VALUES ($1, $2, $3, $4, $5, now(), now())",
    )
    .bind(&task_id)
    .bind(&cmd.node_id)
    .bind(&cmd.worker_role)
    .bind(&cmd.skill_pack_id)
    .bind(status_str)
    .execute(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    let event_id = append_event(
        tx,
        "task",
        &task_id,
        "task_created",
        &cmd.idempotency_key,
        &serde_json::json!({
            "task_id": task_id,
            "node_id": cmd.node_id,
            "worker_role": cmd.worker_role,
            "skill_pack_id": cmd.skill_pack_id,
            "status": status_str,
            "trigger": "command",
        }),
    )
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    tracing::info!(task_id, node_id = %cmd.node_id, "CTL-005: task created");
    Ok(outcome_applied(event_id, format!("task {} created", task_id)))
}


/// Execute [`DispatchSchedulerCommand`].
///
/// Mirrors the SQL in `tick.rs::dispatch_queued_tasks`.
/// Dispatches up to `max_dispatches` queued tasks in one round.
pub async fn execute_dispatch_scheduler(
    tx: &mut Transaction<'_, Postgres>,
    cmd: &DispatchSchedulerCommand,
) -> CommandResult {
    // Idempotency check
    if let Some(_existing) = idempotency_check(tx, "dispatch", &cmd.idempotency_key).await
        .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?
    {
        return Ok(outcome_skip("dispatch round already executed with this idempotency key"));
    }

    // Find queued tasks, optionally scoped to a cycle
    let rows = if let Some(ref cycle_id) = cmd.cycle_id {
        sqlx::query(
            "SELECT t.task_id, t.node_id, t.worker_role \
             FROM tasks t \
             JOIN nodes n ON n.node_id = t.node_id \
             JOIN loops l ON l.objective_id = n.objective_id \
             JOIN cycles c ON c.loop_id = l.loop_id \
             WHERE t.status = 'queued' AND c.cycle_id = $1 \
             ORDER BY t.created_at ASC \
             LIMIT $2",
        )
        .bind(cycle_id)
        .bind(cmd.max_dispatches as i64)
        .fetch_all(tx.as_mut())
        .await
        .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?
    } else {
        sqlx::query(
            "SELECT t.task_id, t.node_id, t.worker_role \
             FROM tasks t \
             WHERE t.status = 'queued' \
             ORDER BY t.created_at ASC \
             LIMIT $1",
        )
        .bind(cmd.max_dispatches as i64)
        .fetch_all(tx.as_mut())
        .await
        .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?
    };

    let mut dispatched_count = 0u32;
    let mut event_ids = Vec::new();

    for row in &rows {
        let task_id: &str = row.get("task_id");
        let node_id: &str = row.get("node_id");
        let worker_role: &str = row.get("worker_role");

        // Mark task as running (optimistic locking on status)
        let result = sqlx::query(
            "UPDATE tasks SET status = 'running', updated_at = now() \
             WHERE task_id = $1 AND status = 'queued'",
        )
        .bind(task_id)
        .execute(tx.as_mut())
        .await
        .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

        if result.rows_affected() == 0 {
            continue; // race: already dispatched
        }

        // Determine next attempt index
        let max_attempt: Option<i32> = sqlx::query_scalar(
            "SELECT MAX(attempt_index) FROM task_attempts WHERE task_id = $1",
        )
        .bind(task_id)
        .fetch_one(tx.as_mut())
        .await
        .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

        let attempt_index = max_attempt.map_or(1, |m| m + 1);
        let attempt_id = Uuid::now_v7().to_string();

        sqlx::query(
            "INSERT INTO task_attempts \
             (task_attempt_id, task_id, attempt_index, lease_owner, status, started_at) \
             VALUES ($1, $2, $3, 'loop-runner', 'running', now())",
        )
        .bind(&attempt_id)
        .bind(task_id)
        .bind(attempt_index)
        .execute(tx.as_mut())
        .await
        .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

        // Update node lifecycle
        sqlx::query(
            "UPDATE nodes SET lifecycle = 'running', updated_at = now() \
             WHERE node_id = $1 AND lifecycle IN ('proposed', 'queued')",
        )
        .bind(node_id)
        .execute(tx.as_mut())
        .await
        .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

        let eid = append_event(
            tx,
            "task",
            task_id,
            "task_status_changed",
            &format!("dispatch-{}", task_id),
            &serde_json::json!({
                "task_id": task_id,
                "node_id": node_id,
                "attempt_id": attempt_id,
                "attempt_index": attempt_index,
                "worker_role": worker_role,
                "status": "running",
                "trigger": "command_dispatch",
            }),
        )
        .await
        .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

        event_ids.push(eid);
        dispatched_count += 1;
    }

    // Record the dispatch round itself
    let round_event_id = append_event(
        tx,
        "dispatch",
        &Uuid::now_v7().to_string(),
        "dispatch_round_completed",
        &cmd.idempotency_key,
        &serde_json::json!({
            "dispatched_count": dispatched_count,
            "cycle_id": cmd.cycle_id,
            "max_dispatches": cmd.max_dispatches,
        }),
    )
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    event_ids.push(round_event_id);

    tracing::info!(dispatched_count, "CTL-006: dispatch round completed");

    Ok(CommandOutcome {
        applied: dispatched_count > 0,
        idempotent_skip: false,
        message: format!("{} tasks dispatched", dispatched_count),
        event_ids,
        executed_at: Utc::now(),
    })
}


/// Execute [`TaskCompletionCommand`].
///
/// Mirrors the SQL in `orchestration-api/src/routes/task_lifecycle.rs::complete_task`.
pub async fn execute_task_completion(
    tx: &mut Transaction<'_, Postgres>,
    cmd: &TaskCompletionCommand,
) -> CommandResult {
    // Idempotency check
    if let Some(_existing) = idempotency_check(tx, "task", &cmd.idempotency_key).await
        .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?
    {
        return Ok(outcome_skip("task completion already recorded"));
    }

    // Precondition: task must exist and be in a non-terminal state
    let task_row = sqlx::query(
        "SELECT task_id, node_id, status FROM tasks WHERE task_id = $1",
    )
    .bind(&cmd.task_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    let task_row = task_row.ok_or_else(|| {
        reject(CommandRejectionReason::EntityNotFound, format!("task {} not found", cmd.task_id))
    })?;

    let current_status: String = task_row.get("status");
    let node_id: String = task_row.get("node_id");

    if current_status == "succeeded" || current_status == "failed" || current_status == "cancelled" {
        return Err(reject(
            CommandRejectionReason::IllegalTransition,
            format!("task {} is already terminal ({})", cmd.task_id, current_status),
        ));
    }

    // Serialize final_status for SQL
    let final_status_str = serde_json::to_string(&cmd.final_status)
        .unwrap_or_else(|_| "\"succeeded\"".to_string());
    let final_status_str = final_status_str.trim_matches('"');

    // Update task status
    sqlx::query(
        "UPDATE tasks SET status = $1, updated_at = now() WHERE task_id = $2",
    )
    .bind(final_status_str)
    .bind(&cmd.task_id)
    .execute(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    // Finish the running attempt
    sqlx::query(
        "UPDATE task_attempts SET status = $1, finished_at = now() \
         WHERE task_id = $2 AND status = 'running'",
    )
    .bind(final_status_str)
    .bind(&cmd.task_id)
    .execute(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    // Update node lifecycle based on task outcome
    let node_lifecycle = if final_status_str == "succeeded" {
        "completed"
    } else {
        "failed"
    };
    sqlx::query(
        "UPDATE nodes SET lifecycle = $1, updated_at = now() WHERE node_id = $2",
    )
    .bind(node_lifecycle)
    .bind(&node_id)
    .execute(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    let event_id = append_event(
        tx,
        "task",
        &cmd.task_id,
        "task_completed",
        &cmd.idempotency_key,
        &serde_json::json!({
            "task_id": cmd.task_id,
            "node_id": node_id,
            "final_status": final_status_str,
            "worker_id": cmd.worker_id,
            "artifact_ref": cmd.artifact_ref,
            "completed_at": cmd.completed_at.to_rfc3339(),
        }),
    )
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    tracing::info!(task_id = %cmd.task_id, final_status = final_status_str, "CTL-010: task completed");
    Ok(outcome_applied(event_id, format!("task {} completed as {}", cmd.task_id, final_status_str)))
}


/// Execute [`FailureIngestionCommand`].
///
/// Mirrors the SQL in `orchestration-api/src/routes/task_lifecycle.rs::fail_task`.
pub async fn execute_failure_ingestion(
    tx: &mut Transaction<'_, Postgres>,
    cmd: &FailureIngestionCommand,
) -> CommandResult {
    // Idempotency check
    if let Some(_existing) = idempotency_check(tx, "task", &cmd.idempotency_key).await
        .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?
    {
        return Ok(outcome_skip("failure already recorded"));
    }

    // Precondition: task must exist
    let task_row = sqlx::query(
        "SELECT task_id, node_id, status FROM tasks WHERE task_id = $1",
    )
    .bind(&cmd.task_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    let task_row = task_row.ok_or_else(|| {
        reject(CommandRejectionReason::EntityNotFound, format!("task {} not found", cmd.task_id))
    })?;

    let node_id: String = task_row.get("node_id");

    // Update task to failed
    sqlx::query(
        "UPDATE tasks SET status = 'failed', updated_at = now() WHERE task_id = $1",
    )
    .bind(&cmd.task_id)
    .execute(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    // Finish the running attempt
    sqlx::query(
        "UPDATE task_attempts SET status = 'failed', finished_at = now() \
         WHERE task_id = $1 AND status = 'running'",
    )
    .bind(&cmd.task_id)
    .execute(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    // Update node lifecycle
    sqlx::query(
        "UPDATE nodes SET lifecycle = 'failed', updated_at = now() WHERE node_id = $1",
    )
    .bind(&node_id)
    .execute(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    let failure_kind_str = serde_json::to_string(&cmd.failure_kind)
        .unwrap_or_else(|_| "\"transient\"".to_string());
    let failure_kind_str = failure_kind_str.trim_matches('"');

    let event_id = append_event(
        tx,
        "task",
        &cmd.task_id,
        "task_failed",
        &cmd.idempotency_key,
        &serde_json::json!({
            "task_id": cmd.task_id,
            "node_id": node_id,
            "worker_id": cmd.worker_id,
            "failure_kind": failure_kind_str,
            "error_message": cmd.error_message,
            "attempt_number": cmd.attempt_number,
            "failed_at": cmd.failed_at.to_rfc3339(),
        }),
    )
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    tracing::info!(task_id = %cmd.task_id, failure_kind = failure_kind_str, "CTL-011: failure ingested");
    Ok(outcome_applied(event_id, format!("task {} failure recorded", cmd.task_id)))
}


/// Execute [`TimeoutIngestionCommand`].
///
/// No dedicated runtime equivalent exists yet; this provides the canonical
/// implementation.
pub async fn execute_timeout_ingestion(
    tx: &mut Transaction<'_, Postgres>,
    cmd: &TimeoutIngestionCommand,
) -> CommandResult {
    // Idempotency check
    if let Some(_existing) = idempotency_check(tx, "task", &cmd.idempotency_key).await
        .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?
    {
        return Ok(outcome_skip("timeout already recorded"));
    }

    // Precondition: task must exist and be running
    let task_row = sqlx::query(
        "SELECT task_id, node_id, status FROM tasks WHERE task_id = $1",
    )
    .bind(&cmd.task_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    let task_row = task_row.ok_or_else(|| {
        reject(CommandRejectionReason::EntityNotFound, format!("task {} not found", cmd.task_id))
    })?;

    let current_status: String = task_row.get("status");
    let node_id: String = task_row.get("node_id");

    if current_status != "running" {
        return Err(reject(
            CommandRejectionReason::IllegalTransition,
            format!("task {} is {} (expected running for timeout)", cmd.task_id, current_status),
        ));
    }

    // Mark task as failed (timeout is a failure type)
    sqlx::query(
        "UPDATE tasks SET status = 'failed', updated_at = now() WHERE task_id = $1",
    )
    .bind(&cmd.task_id)
    .execute(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    // Finish the running attempt
    sqlx::query(
        "UPDATE task_attempts SET status = 'timed_out', finished_at = now() \
         WHERE task_id = $1 AND status = 'running'",
    )
    .bind(&cmd.task_id)
    .execute(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    let event_id = append_event(
        tx,
        "task",
        &cmd.task_id,
        "task_timed_out",
        &cmd.idempotency_key,
        &serde_json::json!({
            "task_id": cmd.task_id,
            "node_id": node_id,
            "worker_id": cmd.worker_id,
            "elapsed_seconds": cmd.elapsed_seconds,
            "timeout_threshold_seconds": cmd.timeout_threshold_seconds,
            "attempt_number": cmd.attempt_number,
            "timed_out_at": cmd.timed_out_at.to_rfc3339(),
        }),
    )
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    tracing::info!(task_id = %cmd.task_id, elapsed = cmd.elapsed_seconds, "CTL-012: timeout ingested");
    Ok(outcome_applied(event_id, format!("task {} timeout recorded", cmd.task_id)))
}


/// Execute [`RetrySchedulingCommand`].
///
/// Transitions a failed task back to queued for retry.
/// Mirrors the `failed -> queued` path in `task_lifecycle.rs::patch_task`.
pub async fn execute_retry_scheduling(
    tx: &mut Transaction<'_, Postgres>,
    cmd: &RetrySchedulingCommand,
) -> CommandResult {
    // Idempotency check
    if let Some(_existing) = idempotency_check(tx, "task", &cmd.idempotency_key).await
        .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?
    {
        return Ok(outcome_skip("retry already scheduled"));
    }

    // Precondition: task must exist and be failed
    let task_row = sqlx::query(
        "SELECT task_id, node_id, status, retry_budget FROM tasks WHERE task_id = $1",
    )
    .bind(&cmd.task_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    let task_row = task_row.ok_or_else(|| {
        reject(CommandRejectionReason::EntityNotFound, format!("task {} not found", cmd.task_id))
    })?;

    let current_status: String = task_row.get("status");
    let node_id: String = task_row.get("node_id");
    let retry_budget: Option<i32> = task_row.try_get("retry_budget").ok();

    if current_status != "failed" {
        return Err(reject(
            CommandRejectionReason::IllegalTransition,
            format!("task {} is {} (expected failed for retry)", cmd.task_id, current_status),
        ));
    }

    // Check retry budget
    if let Some(budget) = retry_budget {
        let attempt_count: Option<i64> = sqlx::query_scalar(
            "SELECT COUNT(*) FROM task_attempts WHERE task_id = $1",
        )
        .bind(&cmd.task_id)
        .fetch_one(tx.as_mut())
        .await
        .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

        if attempt_count.unwrap_or(0) >= budget as i64 {
            return Err(reject(
                CommandRejectionReason::PreconditionFailed,
                format!(
                    "task {} retry budget exhausted ({}/{})",
                    cmd.task_id,
                    attempt_count.unwrap_or(0),
                    budget
                ),
            ));
        }
    }

    // Re-queue the task
    sqlx::query(
        "UPDATE tasks SET status = 'queued', updated_at = now() WHERE task_id = $1",
    )
    .bind(&cmd.task_id)
    .execute(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    // Update node lifecycle back to queued
    sqlx::query(
        "UPDATE nodes SET lifecycle = 'queued', updated_at = now() WHERE node_id = $1",
    )
    .bind(&node_id)
    .execute(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    let event_id = append_event(
        tx,
        "task",
        &cmd.task_id,
        "task_retry_scheduled",
        &cmd.idempotency_key,
        &serde_json::json!({
            "task_id": cmd.task_id,
            "node_id": node_id,
            "next_attempt_number": cmd.next_attempt_number,
            "delay_ms": cmd.delay_ms,
            "reason": cmd.reason,
            "reassign_worker": cmd.reassign_worker,
        }),
    )
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    tracing::info!(task_id = %cmd.task_id, next_attempt = cmd.next_attempt_number, "CTL-013: retry scheduled");
    Ok(outcome_applied(event_id, format!("task {} retry scheduled (attempt {})", cmd.task_id, cmd.next_attempt_number)))
}


/// Execute [`NextCycleGenerationCommand`].
///
/// Mirrors the SQL in `tick.rs::create_cycles_for_active_loops` triggered
/// when the current cycle reaches `next_cycle_ready`.
pub async fn execute_next_cycle_generation(
    tx: &mut Transaction<'_, Postgres>,
    cmd: &NextCycleGenerationCommand,
) -> CommandResult {
    // Idempotency check
    if let Some(_existing) = idempotency_check(tx, "cycle", &cmd.idempotency_key).await
        .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?
    {
        return Ok(outcome_skip("next cycle already generated"));
    }

    // Precondition: current cycle must be in next_cycle_ready
    let cycle_row = sqlx::query(
        "SELECT cycle_id, loop_id, phase FROM cycles WHERE cycle_id = $1",
    )
    .bind(&cmd.current_cycle_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    let cycle_row = cycle_row.ok_or_else(|| {
        reject(
            CommandRejectionReason::EntityNotFound,
            format!("cycle {} not found", cmd.current_cycle_id),
        )
    })?;

    let current_phase: String = cycle_row.get("phase");
    if current_phase != "next_cycle_ready" {
        return Err(reject(
            CommandRejectionReason::PreconditionFailed,
            format!(
                "cycle {} is in phase {} (expected next_cycle_ready)",
                cmd.current_cycle_id, current_phase
            ),
        ));
    }

    // Determine next cycle index
    let current_index: Option<i32> = sqlx::query_scalar(
        "SELECT cycle_index FROM loops WHERE loop_id = $1",
    )
    .bind(&cmd.loop_id)
    .fetch_optional(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    let next_index = current_index.map_or(1, |i| i + 1);
    let cycle_id = Uuid::now_v7().to_string();

    // Create the new cycle
    sqlx::query(
        "INSERT INTO cycles \
         (cycle_id, loop_id, cycle_index, phase, policy_snapshot_id, created_at, updated_at) \
         VALUES ($1, $2, $3, 'intake', $4, now(), now())",
    )
    .bind(&cycle_id)
    .bind(&cmd.loop_id)
    .bind(next_index)
    .bind(&cmd.next_policy_snapshot_id)
    .execute(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    // Bump cycle_index on the parent loop
    sqlx::query(
        "UPDATE loops SET cycle_index = $1, updated_at = now() WHERE loop_id = $2",
    )
    .bind(next_index)
    .bind(&cmd.loop_id)
    .execute(tx.as_mut())
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    // Handle carry-forward nodes
    for carry_node_id in &cmd.carry_forward_node_ids {
        // Mark carried nodes as queued in the new cycle context
        sqlx::query(
            "UPDATE nodes SET lifecycle = 'queued', updated_at = now() \
             WHERE node_id = $1 AND lifecycle IN ('failed', 'completed')",
        )
        .bind(carry_node_id)
        .execute(tx.as_mut())
        .await
        .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;
    }

    let event_id = append_event(
        tx,
        "cycle",
        &cycle_id,
        "cycle_created",
        &cmd.idempotency_key,
        &serde_json::json!({
            "cycle_id": cycle_id,
            "loop_id": cmd.loop_id,
            "cycle_index": next_index,
            "policy_snapshot_id": cmd.next_policy_snapshot_id,
            "previous_cycle_id": cmd.current_cycle_id,
            "carry_forward_node_ids": cmd.carry_forward_node_ids,
            "initial_phase": "intake",
            "trigger": "next_cycle_generation",
        }),
    )
    .await
    .map_err(|e| reject(CommandRejectionReason::PreconditionFailed, e.to_string()))?;

    tracing::info!(
        cycle_id,
        loop_id = %cmd.loop_id,
        previous_cycle = %cmd.current_cycle_id,
        "CTL-015: next cycle generated"
    );
    Ok(outcome_applied(event_id, format!("cycle {} generated (next after {})", cycle_id, cmd.current_cycle_id)))
}
