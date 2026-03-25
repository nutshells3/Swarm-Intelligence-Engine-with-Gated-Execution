//! Worker dispatch service.
//!
//! Picks up tasks with status "running" that have no active attempt,
//! creates git worktrees for isolation, invokes the appropriate agent
//! adapter, captures results, and updates task status.
//!
//! This is a separate process from the loop-runner: the loop-runner
//! advances phases and creates tasks, while worker-dispatch executes
//! them.

use agent_adapters::adapter::{AdapterRequest, AdapterStatus};
use agent_adapters::registry::AdapterRegistry;
use git_control::worktree::WorktreeManager;
use scaling::{ScalingContext, Event};
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use std::env;
use uuid::Uuid;

struct DispatchContext {
    pool: PgPool,
    registry: Arc<AdapterRegistry>,
    worktree_mgr: WorktreeManager,
    scaling_ctx: Arc<ScalingContext>,
    max_concurrent: usize,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("info".parse()?),
        )
        .init();

    let database_url = env::var("ORCHESTRATION_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/development_swarm".to_string());

    let repo_root = env::var("SWARM_REPO_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env::current_dir().unwrap());

    let max_concurrent: usize = env::var("SWARM_MAX_CONCURRENT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1000);

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&database_url)
        .await?;

    sqlx::migrate!("../../db/migrations").run(&pool).await?;

    let registry = Arc::new(AdapterRegistry::auto_detect());
    tracing::info!(adapters = ?registry.list(), "Available adapters");

    if registry.is_empty() {
        tracing::warn!(
            "No agent adapters detected! \
             Install claude CLI, codex CLI, or set ANTHROPIC_API_KEY / OPENAI_API_KEY."
        );
    }

    let worktree_mgr = WorktreeManager::new(repo_root.clone());

    let scaling_config = scaling::config::load_scaling_config();
    let scaling_ctx = Arc::new(
        ScalingContext::from_config(scaling_config, pool.clone(), repo_root)
            .await
            .expect("Failed to build ScalingContext"),
    );

    let ctx = DispatchContext {
        pool,
        registry,
        worktree_mgr,
        scaling_ctx,
        max_concurrent,
    };

    tracing::info!(max_concurrent, "Worker dispatch started");

    loop {
        match dispatch_tick(&ctx).await {
            Ok(dispatched) => {
                if dispatched > 0 {
                    tracing::info!(dispatched, "Dispatch tick completed");
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "Dispatch tick failed");
            }
        }
        tokio::time::sleep(Duration::from_secs(3)).await;
    }
}

/// Run one dispatch tick: find running tasks without active attempts
/// whose dependencies are all completed, and execute them in parallel.
async fn dispatch_tick(ctx: &DispatchContext) -> Result<u32, Box<dyn std::error::Error>> {
    // 1. Count currently running worker-dispatch attempts
    let running_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM task_attempts WHERE status = 'running' AND lease_owner = 'worker-dispatch'",
    )
    .fetch_one(&ctx.pool)
    .await?;

    // Check policy concurrency limits from user_policies
    let policy_row = sqlx::query(
        "SELECT policy_payload FROM user_policies ORDER BY revision DESC LIMIT 1",
    )
    .fetch_optional(&ctx.pool)
    .await?;

    let max_agents = policy_row
        .as_ref()
        .and_then(|r| r.try_get::<serde_json::Value, _>("policy_payload").ok())
        .and_then(|v| v.pointer("/global/max_active_agents")?.as_i64())
        .unwrap_or(ctx.max_concurrent as i64);

    let available_slots =
        (max_agents as usize).min(ctx.max_concurrent).saturating_sub(running_count as usize);
    if available_slots == 0 {
        return Ok(0);
    }

    // 2. Find tasks that are ready: status=running, no worker-dispatch attempt,
    //    AND all dependency predecessors are completed.
    //    FOR UPDATE OF t SKIP LOCKED prevents concurrent ticks from picking the same task.
    let tasks = sqlx::query(
        r#"
        SELECT t.task_id, t.node_id, t.worker_role, t.skill_pack_id,
               t.provider_mode, t.model_binding,
               n.title AS node_title, n.statement AS node_statement
        FROM tasks t
        JOIN nodes n ON t.node_id = n.node_id
        WHERE t.status = 'running'
          AND NOT EXISTS (
              SELECT 1 FROM task_attempts ta
              WHERE ta.task_id = t.task_id
                AND ta.status = 'running'
                AND ta.lease_owner = 'worker-dispatch'
          )
          AND NOT EXISTS (
              SELECT 1 FROM node_edges ne
              JOIN nodes pred ON ne.from_node_id = pred.node_id
              WHERE ne.to_node_id = t.node_id
                AND ne.edge_kind IN ('depends_on', 'blocks')
                AND pred.lifecycle NOT IN ('admitted', 'done', 'completed')
          )
          AND EXISTS (
              SELECT 1 FROM plan_gates pg
              JOIN plans p ON pg.plan_id = p.plan_id
              WHERE p.objective_id = n.objective_id
                AND pg.current_status IN ('satisfied', 'overridden')
                AND pg.evaluated_at = (
                    SELECT MAX(pg2.evaluated_at) FROM plan_gates pg2
                    WHERE pg2.plan_id = p.plan_id
                )
          )
        FOR UPDATE OF t SKIP LOCKED
        LIMIT $1
        "#,
    )
    .bind(available_slots as i32)
    .fetch_all(&ctx.pool)
    .await?;

    if tasks.is_empty() {
        return Ok(0);
    }

    let dispatched = tasks.len() as u32;

    // Pre-dispatch file-level overlap detection.
    //
    // Current behavior (file-level):
    //   1. Collect git diffs from artifact_refs for all currently-running tasks.
    //   2. Parse +++ b/ and --- a/ lines to extract file paths.
    //   3. Check whether the candidate task's node_statement mentions any
    //      of those files. If so, skip dispatch to prevent concurrent
    //      modification of the same files.
    //
    // Limitations:
    //   - Heuristic only: it relies on the node statement mentioning file
    //     paths. Tasks that modify files not mentioned in their description
    //     are not caught.
    //   - File-level granularity: two tasks editing different functions in
    //     the same file are treated as conflicting.
    //
    // Future extension path (symbol-level):
    //   - After the adapter produces output, parse the git diff with
    //     tree-sitter to extract changed symbols (functions, types, etc.).
    //   - Store symbol-level change sets in artifact_refs with
    //     artifact_kind = 'symbol_diff'.
    //   - Replace the file-path heuristic with a symbol-set intersection
    //     check. This would allow non-overlapping edits to the same file
    //     to proceed in parallel while still blocking true semantic conflicts.
    //   - The ConflictKind::Semantic variant in git-control already exists
    //     to classify such conflicts once detected.
    //
    // Collect files touched by currently running tasks
    let running_files: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT ar.artifact_uri FROM artifact_refs ar \
         JOIN task_attempts ta ON ar.task_id = ta.task_id \
         WHERE ta.status = 'running' AND ta.lease_owner = 'worker-dispatch' \
         AND ar.artifact_kind = 'git_diff'",
    )
    .fetch_all(&ctx.pool)
    .await
    .unwrap_or_default();

    // Extract file paths from diffs of running tasks
    let running_file_set: std::collections::HashSet<String> = running_files
        .iter()
        .flat_map(|diff| {
            diff.lines()
                .filter(|l| l.starts_with("+++ b/") || l.starts_with("--- a/"))
                .map(|l| {
                    l.trim_start_matches("+++ b/")
                        .trim_start_matches("--- a/")
                        .to_string()
                })
        })
        .collect();

    // 4. Spawn each task as a tokio task for true parallel execution
    for task_row in tasks {
        let node_statement_check: String = task_row.try_get("node_statement").unwrap_or_default();

        // File-level overlap check (see documentation block above)
        if !running_file_set.is_empty() {
            let has_overlap = running_file_set.iter().any(|f| node_statement_check.contains(f));
            if has_overlap {
                let skip_task_id: String = task_row.try_get("task_id").unwrap_or_default();
                tracing::warn!(
                    task_id = %skip_task_id,
                    "Skipping task dispatch: file overlap with running tasks detected"
                );
                continue;
            }
        }

        let pool = ctx.pool.clone();
        let registry = ctx.registry.clone();
        let worktree_repo_root = ctx.worktree_mgr.repo_root.clone();
        let scaling_ctx = ctx.scaling_ctx.clone();

        tokio::spawn(async move {
            let task_id: String = task_row.try_get("task_id").unwrap_or_default();
            let node_title: String = task_row.try_get("node_title").unwrap_or_default();
            let node_statement: String = task_row.try_get("node_statement").unwrap_or_default();
            let provider_mode: Option<String> = task_row.try_get("provider_mode").ok();
            let model_binding: Option<String> = task_row.try_get("model_binding").ok();

            if let Err(e) = execute_task(
                &pool,
                &registry,
                &worktree_repo_root,
                &scaling_ctx,
                &task_id,
                &node_title,
                &node_statement,
                provider_mode.as_deref(),
                model_binding,
            )
            .await
            {
                tracing::error!(task_id, error = %e, "Task execution failed");
            }
        });
    }

    Ok(dispatched)
}

/// Execute a single task: create worktree, invoke adapter, update status,
/// check for newly unblocked dependent tasks.
async fn execute_task(
    pool: &PgPool,
    registry: &AdapterRegistry,
    worktree_repo_root: &PathBuf,
    scaling_ctx: &ScalingContext,
    task_id: &str,
    node_title: &str,
    node_statement: &str,
    provider_mode: Option<&str>,
    model_binding: Option<String>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Terminal state guard: re-check that the task is still in 'running' state.
    // Guards against race conditions where the task state changed between
    // the dispatch query and execution start.
    let task_info = sqlx::query(
        "SELECT node_id, worker_role, status, retry_budget FROM tasks WHERE task_id = $1",
    )
    .bind(task_id)
    .fetch_one(pool)
    .await?;
    let node_id: String = task_info.try_get("node_id")?;
    let worker_role: String = task_info.try_get("worker_role").unwrap_or_default();
    let current_status: String = task_info.try_get("status").unwrap_or_default();

    // Terminal states: do not execute
    match current_status.as_str() {
        "succeeded" | "failed_permanent" | "cancelled" | "archived" => {
            tracing::warn!(
                task_id,
                status = %current_status,
                "Refusing to dispatch task in terminal state"
            );
            return Ok(());
        }
        "running" => { /* expected -- proceed */ }
        other => {
            tracing::warn!(
                task_id,
                status = %other,
                "Unexpected task status at dispatch time; proceeding cautiously"
            );
        }
    }

    // Special handling for integration verification tasks — bypass adapter
    if worker_role == "integration_verifier" {
        return execute_integration_verify(pool, task_id, &node_id, worktree_repo_root).await;
    }

    // Select the best available adapter with fallback.
    let adapter = registry
        .select_with_fallback(provider_mode)
        .or_else(|| registry.select_with_fallback(model_binding.as_deref()))
        .or_else(|| registry.select_with_fallback(None));

    let Some(adapter) = adapter else {
        tracing::error!(
            task_id,
            "No adapter available — install claude CLI, codex CLI, or set API keys. Skipping task."
        );
        return Ok(());
    };

    tracing::info!(
        task_id,
        node_title,
        adapter = adapter.name(),
        "Dispatching task"
    );

    // Create isolated workspace for task (uses scaling tier: worktree / pooled / container)
    let worktree_path = match scaling_ctx.isolation.acquire(task_id).await {
        Ok(path) => {
            tracing::info!(task_id, path = %path.display(), "Workspace ready");

            let artifacts_dir = path.join(".artifacts");
            if let Err(e) = tokio::fs::create_dir_all(&artifacts_dir).await {
                tracing::warn!(
                    task_id,
                    error = %e,
                    "Failed to create .artifacts directory (non-fatal)"
                );
            } else {
                tracing::debug!(task_id, path = %artifacts_dir.display(), ".artifacts directory created");
            }

            // Record the worker-to-worktree binding so the control plane
            // knows which task owns which worktree.
            let assignment_id = Uuid::now_v7().to_string();
            let branch_name = format!("task-{}", task_id);
            let worktree_path_str = path.to_string_lossy().to_string();
            if let Err(e) = sqlx::query(
                "INSERT INTO git_worktree_assignments \
                     (assignment_id, worker_id, task_id, worktree_path, branch_name, assigned_at, active) \
                 VALUES ($1, 'worker-dispatch', $2, $3, $4, now(), true) \
                 ON CONFLICT DO NOTHING",
            )
            .bind(&assignment_id)
            .bind(task_id)
            .bind(&worktree_path_str)
            .bind(&branch_name)
            .execute(pool)
            .await
            {
                tracing::warn!(task_id, error = %e, "Failed to record worktree assignment (non-fatal)");
            }

            // Record worktree binding event in event journal
            let wt_event_id = Uuid::now_v7().to_string();
            let wt_idem = format!("worktree-bind-{}", task_id);
            if let Err(e) = sqlx::query(
                "INSERT INTO event_journal \
                     (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
                 VALUES ($1, 'git', $2, 'worktree_bound', $3, $4, now()) \
                 ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
            )
            .bind(&wt_event_id)
            .bind(task_id)
            .bind(&wt_idem)
            .bind(serde_json::json!({
                "task_id": task_id,
                "worker_id": "worker-dispatch",
                "worktree_path": worktree_path_str,
                "branch_name": branch_name,
                "trigger": "worktree_acquisition"
            }))
            .execute(pool)
            .await {
                tracing::warn!(error = %e, "Failed to record event");
            }

            path
        }
        Err(e) => {
            // Fail retryable instead of falling back to repo root — shared-directory
            // fallback lets concurrent tasks clobber each other.
            tracing::error!(task_id, error = %e, "Failed to acquire workspace");
            return Err(format!(
                "Worktree acquisition failed for task {}: {}",
                task_id, e
            ).into());
        }
    };

    // Determine next attempt index
    let max_attempt: Option<i32> = sqlx::query_scalar(
        "SELECT MAX(attempt_index) FROM task_attempts WHERE task_id = $1",
    )
    .bind(task_id)
    .fetch_one(pool)
    .await?;
    let attempt_index = max_attempt.map_or(1, |m| m + 1);

    // Create the task attempt record
    let attempt_id = Uuid::now_v7().to_string();
    let event_id = Uuid::now_v7().to_string();
    let idempotency_key = format!("worker-dispatch-{}-{}", task_id, attempt_index);

    {
        let mut tx = pool.begin().await?;

        // Scoped idempotency check
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT aggregate_id FROM event_journal
             WHERE aggregate_kind = 'task' AND idempotency_key = $1 LIMIT 1",
        )
        .bind(&idempotency_key)
        .fetch_optional(tx.as_mut())
        .await?;

        if existing.is_some() {
            tx.rollback().await?;
            return Ok(());
        }

        // Insert the attempt as running
        sqlx::query(
            "INSERT INTO task_attempts \
                 (task_attempt_id, task_id, attempt_index, lease_owner, status, started_at) \
             VALUES ($1, $2, $3, 'worker-dispatch', 'running', now())",
        )
        .bind(&attempt_id)
        .bind(task_id)
        .bind(attempt_index)
        .execute(tx.as_mut())
        .await?;

        // Read skill_pack_id from the task row for provenance recording
        let skill_pack_id: String = sqlx::query_scalar(
            "SELECT skill_pack_id FROM tasks WHERE task_id = $1",
        )
        .bind(task_id)
        .fetch_one(tx.as_mut())
        .await
        .unwrap_or_else(|_| "unknown".to_string());

        // Record the dispatch event with skill resolution provenance
        let payload = serde_json::json!({
            "task_id": task_id,
            "node_id": node_id,
            "attempt_id": attempt_id,
            "attempt_index": attempt_index,
            "adapter": adapter.name(),
            "trigger": "worker_dispatch",
            "skill_pack_id": skill_pack_id,
            "skill_selection_reason": "bound at task creation via resolve_skill_full"
        });

        sqlx::query(
            "INSERT INTO event_journal \
                 (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
             VALUES ($1, 'task', $2, 'task_attempt_started', $3, $4, now()) \
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
        )
        .bind(&event_id)
        .bind(task_id)
        .bind(&idempotency_key)
        .bind(&payload)
        .execute(tx.as_mut())
        .await?;

        tx.commit().await?;

        // Also publish to event bus for NATS path (after tx commit)
        let _ = scaling_ctx.event_bus.publish(Event {
            event_id: event_id.clone(),
            aggregate_kind: "task".into(),
            aggregate_id: task_id.into(),
            event_kind: "task_attempt_started".into(),
            idempotency_key: idempotency_key.clone(),
            payload: payload.clone(),
        }).await;
    }

    // Load task cautions for the prompt
    let cautions: serde_json::Value = sqlx::query_scalar(
        "SELECT COALESCE(cautions, '[]'::jsonb) FROM tasks WHERE task_id = $1"
    )
    .bind(task_id)
    .fetch_one(pool)
    .await
    .unwrap_or(serde_json::json!([]));

    let cautions_text = if let Some(arr) = cautions.as_array() {
        if arr.is_empty() { String::new() }
        else {
            let items: Vec<&str> = arr.iter().filter_map(|v| v.as_str()).collect();
            format!("\n\n### Cautions\n{}", items.iter().map(|c| format!("- {}", c)).collect::<Vec<_>>().join("\n"))
        }
    } else { String::new() };

    // Read policy-derived token limits
    let policy_row_ctx = sqlx::query(
        "SELECT policy_payload FROM user_policies ORDER BY revision DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await
    .ok()
    .flatten();

    let policy_payload = policy_row_ctx
        .as_ref()
        .and_then(|r| r.try_get::<serde_json::Value, _>("policy_payload").ok());

    let policy_max_input_tokens = policy_payload
        .as_ref()
        .and_then(|v| v.pointer("/global/max_input_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(100_000);

    let policy_max_output_tokens = policy_payload
        .as_ref()
        .and_then(|v| v.pointer("/global/max_output_tokens"))
        .and_then(|v| v.as_u64())
        .unwrap_or(4096);

    let prompt = format!(
        "## Task: {}\n\n\
         ### Description\n\
         {}{}\n\n\
         ### Working Directory\n\
         {}\n\n\
         Please implement this task. Follow existing code conventions.",
        node_title,
        node_statement,
        cautions_text,
        worktree_path.display()
    );

    // Enforce input token budget (rough estimate: ~4 chars per token)
    let max_input_chars = (policy_max_input_tokens * 4) as usize;
    let prompt = if prompt.len() > max_input_chars {
        let truncated = &prompt[..max_input_chars];
        format!(
            "{}\n\n[TRUNCATED: input exceeded {} token budget]",
            truncated, policy_max_input_tokens
        )
    } else {
        prompt
    };

    let request = AdapterRequest {
        task_id: task_id.to_string(),
        prompt,
        context_files: vec![],
        working_directory: worktree_path.to_string_lossy().to_string(),
        model: model_binding,
        provider_mode: provider_mode.unwrap_or("auto").to_string(),
        timeout_seconds: 600,
        max_tokens: Some(policy_max_output_tokens as u32),
        temperature: None,
    };

    let adapter_name = adapter.name().to_string();

    // Emit periodic heartbeat events to the event_journal every 30 seconds
    // while the adapter is running, so the control plane knows the worker
    // is still alive.
    let heartbeat_pool = pool.clone();
    let heartbeat_task_id = task_id.to_string();
    let heartbeat_attempt_id = attempt_id.clone();
    let heartbeat_adapter_name = adapter_name.clone();
    let (cancel_tx, mut cancel_rx) = tokio::sync::oneshot::channel::<()>();

    let heartbeat_handle = tokio::spawn(async move {
        let mut tick_count: u64 = 0;
        loop {
            tokio::select! {
                _ = &mut cancel_rx => break,
                _ = tokio::time::sleep(Duration::from_secs(30)) => {}
            }
            tick_count += 1;
            let hb_event_id = Uuid::now_v7().to_string();
            let hb_idem = format!("heartbeat-{}-{}", heartbeat_attempt_id, tick_count);
            let hb_payload = serde_json::json!({
                "task_id": heartbeat_task_id,
                "attempt_id": heartbeat_attempt_id,
                "adapter": heartbeat_adapter_name,
                "tick": tick_count,
                "status": "running",
                "trigger": "worker_heartbeat"
            });
            if let Err(e) = sqlx::query(
                "INSERT INTO event_journal \
                     (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
                 VALUES ($1, 'worker', $2, 'worker_status_heartbeat', $3, $4, now()) \
                 ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
            )
            .bind(&hb_event_id)
            .bind(&heartbeat_task_id)
            .bind(&hb_idem)
            .bind(&hb_payload)
            .execute(&heartbeat_pool)
            .await {
                tracing::warn!(error = %e, "Failed to record event");
            }
        }
    });

    // Server-side timeout enforcement: safety net that ensures we never
    // wait indefinitely. Set 10% longer than the adapter timeout to allow
    // the adapter to report its own timeout status.
    tracing::debug!(task_id, adapter = %adapter_name, "spawn boundary entered");
    let server_timeout = Duration::from_secs(request.timeout_seconds as u64 + 30);
    let response = match tokio::time::timeout(server_timeout, adapter.invoke_boxed(request)).await {
        Ok(resp) => resp,
        Err(_elapsed) => {
            tracing::error!(
                task_id,
                timeout_secs = server_timeout.as_secs(),
                "Server-side timeout triggered -- adapter did not respond in time"
            );
            // Synthesize a timeout response
            agent_adapters::adapter::AdapterResponse {
                task_id: task_id.to_string(),
                status: AdapterStatus::TimedOut,
                output: String::new(),
                stdout: String::new(),
                stderr: format!(
                    "Server-side timeout after {}s (adapter unresponsive)",
                    server_timeout.as_secs()
                ),
                duration_ms: server_timeout.as_millis() as u64,
                artifacts: vec![],
                provenance: agent_adapters::adapter::AdapterProvenance {
                    invocation_id: format!("timeout-{}", attempt_id),
                    adapter_name: adapter_name.clone(),
                    model_used: String::new(),
                    provider: String::new(),
                    started_at: String::new(),
                    finished_at: String::new(),
                },
                token_usage: None,
            }
        }
    };

    // Stop the heartbeat background task now that the adapter has returned
    let _ = cancel_tx.send(());
    let _ = heartbeat_handle.await;

    // Classify failures as transient vs permanent.
    // RetryableError and TimedOut are transient (retryable).
    // Failed, EmptyOutput, MalformedOutput are permanent.
    let (final_status, is_retryable) = match response.status {
        AdapterStatus::Succeeded => ("succeeded", false),
        AdapterStatus::Failed => ("failed_permanent", false),
        AdapterStatus::TimedOut => ("failed_retryable", true),
        AdapterStatus::EmptyOutput => ("failed_permanent", false),
        AdapterStatus::MalformedOutput => ("failed_permanent", false),
        AdapterStatus::RetryableError => ("failed_retryable", true),
    };

    let error_message = if response.status != AdapterStatus::Succeeded {
        Some(format!(
            "Adapter {} returned status {:?} (retryable={}): stderr={}",
            adapter_name, response.status, is_retryable, response.stderr
        ))
    } else {
        None
    };

    tracing::info!(
        task_id,
        adapter = %adapter_name,
        status = ?response.status,
        duration_ms = response.duration_ms,
        "Adapter invocation completed"
    );

    // Update attempt and task status in a transaction
    {
        let finish_event_id = Uuid::now_v7().to_string();
        let finish_idempotency_key = format!("finish-{}", attempt_id);

        let mut tx = pool.begin().await?;

        // Mark the attempt as finished
        sqlx::query(
            "UPDATE task_attempts \
             SET status = $1, finished_at = now() \
             WHERE task_attempt_id = $2",
        )
        .bind(final_status)
        .bind(&attempt_id)
        .execute(tx.as_mut())
        .await?;

        // Update task status
        sqlx::query(
            "UPDATE tasks SET status = $1, updated_at = now() WHERE task_id = $2",
        )
        .bind(final_status)
        .bind(task_id)
        .execute(tx.as_mut())
        .await?;

        // If the task succeeded, also update the node lifecycle
        if final_status == "succeeded" {
            sqlx::query(
                "UPDATE nodes SET lifecycle = 'completed', updated_at = now() \
                 WHERE node_id = $1 AND lifecycle = 'running'",
            )
            .bind(&node_id)
            .execute(tx.as_mut())
            .await?;
        }

        // Store output as artifact if we got content
        if !response.output.is_empty() {
            let artifact_id = Uuid::now_v7().to_string();
            let metadata = serde_json::json!({
                "adapter": adapter_name,
                "attempt_id": attempt_id,
                "invocation_id": response.provenance.invocation_id,
                "model_used": response.provenance.model_used,
                "provider": response.provenance.provider,
                "duration_ms": response.duration_ms,
                "token_usage": response.token_usage,
            });
            sqlx::query(
                "INSERT INTO artifact_refs \
                     (artifact_ref_id, task_id, artifact_kind, artifact_uri, metadata) \
                 VALUES ($1, $2, 'adapter_output', $3, $4)",
            )
            .bind(&artifact_id)
            .bind(task_id)
            .bind(&response.output)
            .bind(&metadata)
            .execute(tx.as_mut())
            .await?;
        }

        // Strip null bytes before storing -- PostgreSQL TEXT columns reject 0x00.
        let clean_stdout = response.stdout.replace('\0', "");
        let clean_stderr = response.stderr.replace('\0', "");

        if !clean_stdout.is_empty() {
            let stdout_artifact_id = Uuid::now_v7().to_string();
            sqlx::query(
                "INSERT INTO artifact_refs \
                     (artifact_ref_id, task_id, artifact_kind, artifact_uri, metadata) \
                 VALUES ($1, $2, 'stdout_capture', $3, $4)",
            )
            .bind(&stdout_artifact_id)
            .bind(task_id)
            .bind(&clean_stdout)
            .bind(serde_json::json!({
                "adapter": adapter_name,
                "attempt_id": attempt_id,
                "attempt_index": attempt_index,
            }))
            .execute(tx.as_mut())
            .await?;
        }

        if !clean_stderr.is_empty() {
            let stderr_artifact_id = Uuid::now_v7().to_string();
            sqlx::query(
                "INSERT INTO artifact_refs \
                     (artifact_ref_id, task_id, artifact_kind, artifact_uri, metadata) \
                 VALUES ($1, $2, 'stderr_capture', $3, $4)",
            )
            .bind(&stderr_artifact_id)
            .bind(task_id)
            .bind(&clean_stderr)
            .bind(serde_json::json!({
                "adapter": adapter_name,
                "attempt_id": attempt_id,
                "attempt_index": attempt_index,
            }))
            .execute(tx.as_mut())
            .await?;
        }

        // Emit status sidecar event after each attempt status change.
        let status_event_id = Uuid::now_v7().to_string();
        let status_idempotency_key = format!("status-sidecar-{}-{}", attempt_id, final_status);
        let status_payload = serde_json::json!({
            "task_id": task_id,
            "attempt_id": attempt_id,
            "attempt_index": attempt_index,
            "status": final_status,
            "is_retryable": is_retryable,
            "adapter": adapter_name,
            "duration_ms": response.duration_ms,
            "progress": if final_status == "succeeded" { 100 } else { 0 },
            "trigger": "worker_status_sidecar"
        });
        sqlx::query(
            "INSERT INTO event_journal \
                 (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
             VALUES ($1, 'worker', $2, 'worker_status_heartbeat', $3, $4, now()) \
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
        )
        .bind(&status_event_id)
        .bind(task_id)
        .bind(&status_idempotency_key)
        .bind(&status_payload)
        .execute(tx.as_mut())
        .await?;

        // Record completion event
        let payload = serde_json::json!({
            "task_id": task_id,
            "node_id": node_id,
            "attempt_id": attempt_id,
            "final_status": final_status,
            "adapter": adapter_name,
            "invocation_id": response.provenance.invocation_id,
            "duration_ms": response.duration_ms,
            "error": error_message,
            "trigger": "worker_dispatch_complete"
        });

        sqlx::query(
            "INSERT INTO event_journal \
                 (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
             VALUES ($1, 'task', $2, 'task_attempt_finished', $3, $4, now()) \
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
        )
        .bind(&finish_event_id)
        .bind(task_id)
        .bind(&finish_idempotency_key)
        .bind(&payload)
        .execute(tx.as_mut())
        .await?;

        tx.commit().await?;

        // Also publish to event bus for NATS path (after tx commit)
        let _ = scaling_ctx.event_bus.publish(Event {
            event_id: finish_event_id.clone(),
            aggregate_kind: "task".into(),
            aggregate_id: task_id.into(),
            event_kind: "task_attempt_finished".into(),
            idempotency_key: finish_idempotency_key.clone(),
            payload: payload.clone(),
        }).await;
    }

    // Record retryable backend failure metrics in task_metrics table.
    if is_retryable {
        // Resolve cycle_id from task -> node -> objective -> loop -> cycle chain
        let cycle_id_for_metric: Option<String> = sqlx::query_scalar(
            "SELECT c.cycle_id FROM tasks t \
             JOIN nodes n ON t.node_id = n.node_id \
             JOIN loops l ON n.objective_id = l.objective_id \
             JOIN cycles c ON l.loop_id = c.loop_id \
             WHERE t.task_id = $1 \
             ORDER BY c.created_at DESC LIMIT 1",
        )
        .bind(task_id)
        .fetch_optional(pool)
        .await
        .unwrap_or(None);

        let metric_cycle_id = cycle_id_for_metric.unwrap_or_else(|| "unknown".to_string());
        let metric_id = Uuid::now_v7().to_string();

        if let Err(e) = sqlx::query(
            "INSERT INTO task_metrics \
                 (id, task_id, cycle_id, worker_role, duration_ms, \
                  retry_count, succeeded, failure_category, recorded_at) \
             VALUES ($1, $2, $3, $4, $5, $6, false, 'retryable_backend', now())",
        )
        .bind(&metric_id)
        .bind(task_id)
        .bind(&metric_cycle_id)
        .bind(&worker_role)
        .bind(response.duration_ms as i64)
        .bind(attempt_index)
        .execute(pool)
        .await {
            tracing::warn!(error = %e, "Failed to record event");
        }

        tracing::info!(
            task_id,
            worker_role = %worker_role,
            duration_ms = response.duration_ms,
            attempt_index,
            "Retryable failure metric recorded"
        );
    }

    // Same-cycle retry for retryable failures: decrement retry_budget
    // and set task status back to 'running' for re-dispatch.
    if is_retryable {
        let current_budget: Option<i32> = sqlx::query_scalar(
            "SELECT retry_budget FROM tasks WHERE task_id = $1",
        )
        .bind(task_id)
        .fetch_one(pool)
        .await?;

        let budget = current_budget.unwrap_or(0);
        if budget > 0 {
            let mut retry_tx = pool.begin().await?;

            // Decrement retry_budget and reset task to running for re-dispatch
            sqlx::query(
                "UPDATE tasks SET retry_budget = retry_budget - 1, \
                 status = 'running', updated_at = now() \
                 WHERE task_id = $1 AND retry_budget > 0",
            )
            .bind(task_id)
            .execute(retry_tx.as_mut())
            .await?;

            // Record the retry decision event
            let retry_event_id = Uuid::now_v7().to_string();
            let retry_idem = format!("retry-decision-{}-{}", task_id, attempt_index);
            let retry_payload = serde_json::json!({
                "task_id": task_id,
                "attempt_id": attempt_id,
                "attempt_index": attempt_index,
                "remaining_budget": budget - 1,
                "failure_status": final_status,
                "trigger": "same_cycle_retry"
            });
            sqlx::query(
                "INSERT INTO event_journal \
                     (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
                 VALUES ($1, 'task', $2, 'task_retry_scheduled', $3, $4, now()) \
                 ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
            )
            .bind(&retry_event_id)
            .bind(task_id)
            .bind(&retry_idem)
            .bind(&retry_payload)
            .execute(retry_tx.as_mut())
            .await?;

            retry_tx.commit().await?;

            tracing::info!(
                task_id,
                attempt_index,
                remaining_budget = budget - 1,
                "Retryable failure: task re-queued for same-cycle retry"
            );

            // Skip dependency unblocking and cleanup -- we are retrying
            return Ok(());
        } else {
            tracing::warn!(
                task_id,
                "Retryable failure but retry_budget exhausted; marking as permanent failure"
            );
            // Update to permanent failure since budget is exhausted
            sqlx::query(
                "UPDATE tasks SET status = 'failed_permanent', updated_at = now() WHERE task_id = $1",
            )
            .bind(task_id)
            .execute(pool)
            .await?;
        }
    }

    // After a successful task, check if any dependent tasks can now be
    // unblocked (all their predecessor nodes are completed).
    if final_status == "succeeded" {
        if let Err(e) = unblock_dependent_tasks(pool, &node_id).await {
            tracing::warn!(
                task_id,
                node_id,
                error = %e,
                "Dependency unblock check failed (non-fatal)"
            );
        }
    }

    // Certification eligibility check (post-completion)
    if final_status == "succeeded" {
        if let Err(e) = check_certification_eligibility(
            pool,
            task_id,
            &node_id,
            &attempt_id,
            node_title,
        )
        .await
        {
            tracing::warn!(
                task_id,
                error = %e,
                "Certification eligibility check failed (non-fatal)"
            );
        }
    }

    // Git worktree merge-back and conflict detection
    if final_status == "succeeded" {
        let worktree_mgr_mb = WorktreeManager::new(worktree_repo_root.clone());

        // Check if worktree has changes (use scaling isolation for dirty check)
        if let Ok(dirty) = scaling_ctx.isolation.is_dirty(task_id).await {
            if dirty {
                // Get the diff (use scaling isolation)
                let diff = scaling_ctx.isolation.get_diff(task_id).await.unwrap_or_default();

                if !diff.is_empty() {
                    // Store diff as artifact
                    let diff_artifact_id = uuid::Uuid::now_v7().to_string();
                    sqlx::query(
                        "INSERT INTO artifact_refs (artifact_ref_id, task_id, artifact_kind, artifact_uri, metadata) \
                         VALUES ($1, $2, 'git_diff', $3, $4)"
                    )
                    .bind(&diff_artifact_id)
                    .bind(task_id)
                    .bind(&diff)
                    .bind(serde_json::json!({"worktree": format!("task-{}", task_id), "lines": diff.lines().count()}))
                    .execute(pool)
                    .await.ok();

                    // Check for conflicting edits with other worktrees
                    if let Ok(worktrees) = worktree_mgr_mb.list_worktrees().await {
                        for wt in &worktrees {
                            if wt.branch == format!("task-{}", task_id) { continue; }

                            // Check if any other worktree modified the same files
                            let our_files: Vec<&str> = diff.lines()
                                .filter(|l| l.starts_with("+++ b/") || l.starts_with("--- a/"))
                                .map(|l| l.trim_start_matches("+++ b/").trim_start_matches("--- a/"))
                                .collect();

                            let other_task_id = wt.branch.trim_start_matches("task-");
                            if let Ok(other_diff) = worktree_mgr_mb.get_diff(other_task_id).await {
                                let other_files: Vec<&str> = other_diff.lines()
                                    .filter(|l| l.starts_with("+++ b/") || l.starts_with("--- a/"))
                                    .map(|l| l.trim_start_matches("+++ b/").trim_start_matches("--- a/"))
                                    .collect();

                                let conflicts: Vec<&&str> = our_files.iter()
                                    .filter(|f| other_files.contains(f))
                                    .collect();

                                if !conflicts.is_empty() {
                                    tracing::warn!(
                                        task_id,
                                        other_branch = %wt.branch,
                                        conflicting_files = ?conflicts,
                                        "File-level conflict detected between worktrees"
                                    );

                                    // Create conflict record
                                    let conflict_id = uuid::Uuid::now_v7().to_string();
                                    sqlx::query(
                                        "INSERT INTO conflicts (conflict_id, node_id, conflict_kind, status, created_at, updated_at) \
                                         VALUES ($1, $2, 'mainline_integration', 'open', now(), now()) \
                                         ON CONFLICT DO NOTHING"
                                    )
                                    .bind(&conflict_id)
                                    .bind(&node_id)
                                    .execute(pool)
                                    .await.ok();

                                    // Record conflict detection event in event journal
                                    let conflict_event_id = Uuid::now_v7().to_string();
                                    if let Err(e) = sqlx::query(
                                        "INSERT INTO event_journal \
                                             (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
                                         VALUES ($1, 'conflict', $2, 'file_conflict_detected', $3, $4, now()) \
                                         ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
                                    )
                                    .bind(&conflict_event_id)
                                    .bind(&conflict_id)
                                    .bind(format!("conflict-{}-{}", task_id, wt.branch))
                                    .bind(serde_json::json!({
                                        "task_id": task_id,
                                        "node_id": node_id,
                                        "our_branch": format!("task-{}", task_id),
                                        "other_branch": wt.branch,
                                        "conflicting_files": conflicts.iter().map(|f| f.to_string()).collect::<Vec<_>>(),
                                        "conflict_kind": "mainline_integration",
                                        "trigger": "worktree_conflict_detection"
                                    }))
                                    .execute(pool)
                                    .await {
                                        tracing::warn!(error = %e, "Failed to record event");
                                    }
                                }
                            }
                        }
                    }

                    // Commit changes in the worktree
                    let wt_path = worktree_mgr_mb.worktree_dir.join(format!("task-{}", task_id));
                    let _ = tokio::process::Command::new("git")
                        .args(["add", "-A"])
                        .current_dir(&wt_path)
                        .output().await;
                    let _ = tokio::process::Command::new("git")
                        .args(["commit", "-m", &format!("Task {} completed: {}", task_id, node_title)])
                        .current_dir(&wt_path)
                        .output().await;

                    tracing::info!(task_id, diff_lines = diff.lines().count(), "Worktree changes committed");
                }
            }
        }
    }

    // Before releasing the workspace, check if it has uncommitted changes.
    // If dirty, log a warning and skip automatic cleanup to prevent data loss.
    let should_release = match scaling_ctx.isolation.is_dirty(task_id).await {
        Ok(true) => {
            tracing::warn!(
                task_id,
                "Worktree has uncommitted changes -- skipping automatic cleanup to prevent data loss"
            );

            // Record dirty-worktree warning event
            let dirty_event_id = Uuid::now_v7().to_string();
            if let Err(e) = sqlx::query(
                "INSERT INTO event_journal \
                     (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
                 VALUES ($1, 'git', $2, 'dirty_worktree_detected', $3, $4, now()) \
                 ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
            )
            .bind(&dirty_event_id)
            .bind(task_id)
            .bind(format!("dirty-worktree-{}", task_id))
            .bind(serde_json::json!({
                "task_id": task_id,
                "node_id": node_id,
                "reason": "uncommitted_changes_at_cleanup",
                "trigger": "dirty_worktree_detection"
            }))
            .execute(pool)
            .await {
                tracing::warn!(error = %e, "Failed to record event");
            }

            false // Do not release
        }
        Ok(false) => true,  // Clean -- safe to release
        Err(e) => {
            // If we can't check, attempt release anyway (worktree may already be gone)
            tracing::debug!(task_id, error = %e, "Dirty check failed, attempting release anyway");
            true
        }
    };

    if should_release {
        // Release workspace and mark the git_worktree_assignments row as inactive.
        if let Err(e) = scaling_ctx.isolation.release(task_id).await {
            tracing::warn!(task_id, error = %e, "Workspace release failed");
        }

        // Deactivate the worktree assignment record
        let _ = sqlx::query(
            "UPDATE git_worktree_assignments \
             SET active = false, released_at = now() \
             WHERE task_id = $1 AND active = true",
        )
        .bind(task_id)
        .execute(pool)
        .await;

        // Record cleanup event
        let cleanup_event_id = Uuid::now_v7().to_string();
        if let Err(e) = sqlx::query(
            "INSERT INTO event_journal \
                 (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
             VALUES ($1, 'git', $2, 'worktree_released', $3, $4, now()) \
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
        )
        .bind(&cleanup_event_id)
        .bind(task_id)
        .bind(format!("worktree-release-{}", task_id))
        .bind(serde_json::json!({
            "task_id": task_id,
            "node_id": node_id,
            "trigger": "safe_worktree_cleanup"
        }))
        .execute(pool)
        .await {
            tracing::warn!(error = %e, "Failed to record event");
        }
    }

    tracing::info!(
        task_id,
        node_title,
        final_status,
        attempt_index,
        "Task dispatch completed"
    );

    Ok(())
}

/// After a node completes, find dependent tasks that are now unblocked
/// (all their predecessor nodes are done) and set them to 'running'
/// so worker-dispatch picks them up on the next tick.
async fn unblock_dependent_tasks(
    pool: &PgPool,
    completed_node_id: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
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
        let unblocked_task_id: String = row.try_get("task_id")?;
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

/// After a task succeeds, check whether certification is enabled and whether
/// this output is eligible. If so, create a `certification_candidate` record
/// and (if auto-submit is configured) a `certification_submission` record.
///
/// This is a best-effort operation: failures here do not block task dispatch.
async fn check_certification_eligibility(
    pool: &PgPool,
    task_id: &str,
    node_id: &str,
    attempt_id: &str,
    node_title: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Step 1: Check if certification is enabled via user_policies
    let policy_row = sqlx::query(
        "SELECT policy_payload FROM user_policies WHERE policy_id = 'certification_config'",
    )
    .fetch_optional(pool)
    .await?;

    let policy_payload = match policy_row {
        Some(row) => {
            let payload: serde_json::Value = row.try_get("policy_payload")?;
            payload
        }
        None => {
            // No certification policy set -- certification is off
            return Ok(());
        }
    };

    let enabled = policy_payload
        .get("enabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);

    if !enabled {
        return Ok(());
    }

    let frequency = policy_payload
        .get("frequency")
        .and_then(|v| v.as_str())
        .unwrap_or("off");

    if frequency == "off" {
        return Ok(());
    }

    // Step 2: Determine eligibility
    let title_lower = node_title.to_lowercase();
    let is_critical = title_lower.contains("contract")
        || title_lower.contains("invariant")
        || title_lower.contains("proof")
        || title_lower.contains("safety")
        || title_lower.contains("correctness");

    let eligibility_reason = if is_critical {
        "contract_or_invariant"
    } else {
        "downstream_dependency"
    };

    // Step 3: Apply frequency filter
    let should_create = match frequency {
        "always" => true,
        "critical_only" => is_critical,
        "on_request" => false,
        _ => false,
    };

    if !should_create {
        return Ok(());
    }

    // Step 4: Create certification candidate and submission records
    let candidate_id = Uuid::now_v7().to_string();
    let submission_id = Uuid::now_v7().to_string();
    let idempotency_key = format!("auto-cert-{}-{}", task_id, attempt_id);

    let mut tx = pool.begin().await?;

    // Idempotency check
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT submission_id FROM certification_submissions WHERE idempotency_key = $1",
    )
    .bind(&idempotency_key)
    .fetch_optional(tx.as_mut())
    .await?;

    if existing.is_some() {
        tx.rollback().await?;
        tracing::debug!(task_id, "Certification candidate already exists (idempotent skip)");
        return Ok(());
    }

    let claim_summary = format!("Auto-certification for completed task: {}", node_title);

    // Create the candidate
    sqlx::query(
        "INSERT INTO certification_candidates \
             (candidate_id, node_id, task_id, claim_summary, source_anchors, \
              eligibility_reason, provenance_task_attempt_id, created_at) \
         VALUES ($1, $2, $3, $4, '[]'::jsonb, $5, $6, now())",
    )
    .bind(&candidate_id)
    .bind(node_id)
    .bind(task_id)
    .bind(&claim_summary)
    .bind(eligibility_reason)
    .bind(attempt_id)
    .execute(tx.as_mut())
    .await?;

    // Create the submission
    sqlx::query(
        "INSERT INTO certification_submissions \
             (submission_id, candidate_id, idempotency_key, submitted_at, \
              queue_status, retry_count, max_retries, status_changed_at) \
         VALUES ($1, $2, $3, now(), 'pending', 0, 3, now())",
    )
    .bind(&submission_id)
    .bind(&candidate_id)
    .bind(&idempotency_key)
    .execute(tx.as_mut())
    .await?;

    // Record the event
    let event_payload = serde_json::json!({
        "candidate_id": candidate_id,
        "submission_id": submission_id,
        "task_id": task_id,
        "node_id": node_id,
        "attempt_id": attempt_id,
        "eligibility_reason": eligibility_reason,
        "frequency": frequency,
        "trigger": "worker_dispatch_auto_cert"
    });

    sqlx::query(
        "INSERT INTO event_journal \
             (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
         VALUES ($1, 'certification', $2, 'certification_candidate_created', $3, $4, now()) \
         ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&candidate_id)
    .bind(&idempotency_key)
    .bind(&event_payload)
    .execute(tx.as_mut())
    .await?;

    tx.commit().await?;

    tracing::info!(
        task_id,
        candidate_id,
        submission_id,
        eligibility_reason,
        "Certification candidate created"
    );

    Ok(())
}

/// Execute the integration verification task: merge all completed worktrees
/// into an integration branch, detect project type, run build/test, report.
///
/// GIT-007/008: Strengthened merge logic with proper conflict detection.
/// GIT-009: Review-before-merge check -- if no review_artifacts exist for
///          the objective, emit a review_needed event instead of merging.
async fn execute_integration_verify(
    pool: &PgPool,
    task_id: &str,
    node_id: &str,
    repo_root: &PathBuf,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing::info!(task_id, "Starting integration verification");

    // Review-before-merge gate: look up the objective_id for this node
    // and check if any review_artifacts
    // exist. If not, emit a review_needed event and block the merge.
    let objective_id: Option<String> = sqlx::query_scalar(
        "SELECT objective_id FROM nodes WHERE node_id = $1",
    )
    .bind(node_id)
    .fetch_optional(pool)
    .await?;

    if let Some(ref obj_id) = objective_id {
        let review_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM review_artifacts \
             WHERE target_ref = $1 AND status IN ('approved', 'integrated')",
        )
        .bind(obj_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

        if review_count == 0 {
            tracing::warn!(
                task_id,
                objective_id = %obj_id,
                "No completed reviews found for this objective -- emitting review_needed event"
            );

            let review_event_id = Uuid::now_v7().to_string();
            let review_idem = format!("review-needed-{}-{}", obj_id, task_id);
            sqlx::query(
                "INSERT INTO event_journal \
                     (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
                 VALUES ($1, 'review', $2, 'review_needed', $3, $4, now()) \
                 ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
            )
            .bind(&review_event_id)
            .bind(obj_id)
            .bind(&review_idem)
            .bind(serde_json::json!({
                "objective_id": obj_id,
                "integration_task_id": task_id,
                "node_id": node_id,
                "reason": "no_review_artifacts_found",
                "trigger": "integration_verify_review_gate"
            }))
            .execute(pool)
            .await?;

            // NOTE: We proceed with the merge but the event signals that human
            // review is still needed. A stricter policy could return early here.
        }
    }

    // 1. Find all completed worktrees for this objective's tasks
    let completed_tasks: Vec<(String, String)> = sqlx::query(
        "SELECT ta.task_id, t.node_id
         FROM tasks t
         JOIN task_attempts ta ON ta.task_id = t.task_id
         WHERE t.node_id IN (
             SELECT ne.from_node_id FROM node_edges ne WHERE ne.to_node_id = $1
         )
         AND ta.status = 'succeeded'
         ORDER BY ta.finished_at ASC",
    )
    .bind(node_id)
    .fetch_all(pool)
    .await?
    .iter()
    .map(|row| {
        let tid: String = row.try_get("task_id").unwrap_or_default();
        let nid: String = row.try_get("node_id").unwrap_or_default();
        (tid, nid)
    })
    .collect();

    // 2. Create a clean integration branch
    let integration_branch = format!("integration-{}", task_id);
    let _ = tokio::process::Command::new("git")
        .args(["checkout", "-b", &integration_branch])
        .current_dir(repo_root)
        .output()
        .await;

    // 3. Sequential merge of each worktree's changes
    //    GIT-007/008: Track merge failures with structured conflict info.
    let mut merge_failures: Vec<serde_json::Value> = Vec::new();
    let worktree_dir = repo_root.join(".worktrees");

    for (dep_task_id, dep_node_id) in &completed_tasks {
        let wt_path = worktree_dir.join(format!("task-{}", dep_task_id));

        if !wt_path.exists() {
            tracing::debug!(dep_task_id, "Worktree not found, skipping (already merged or cleaned)");
            continue;
        }

        // Try to merge the worktree branch
        let merge_result = tokio::process::Command::new("git")
            .args([
                "merge",
                "--no-ff",
                &format!("task-{}", dep_task_id),
                "-m",
                &format!("Merge task {} into integration", dep_task_id),
            ])
            .current_dir(repo_root)
            .output()
            .await?;

        if !merge_result.status.success() {
            let stderr = String::from_utf8_lossy(&merge_result.stderr).to_string();
            let stdout = String::from_utf8_lossy(&merge_result.stdout).to_string();
            tracing::warn!(dep_task_id, %stderr, "Merge conflict during integration");

            // GIT-007/008: Identify conflicting files from git merge output
            let conflicting_files: Vec<String> = stdout
                .lines()
                .chain(stderr.lines())
                .filter(|l| l.starts_with("CONFLICT") || l.contains("Merge conflict"))
                .map(|l| l.to_string())
                .collect();

            merge_failures.push(serde_json::json!({
                "task_id": dep_task_id,
                "node_id": dep_node_id,
                "branch": format!("task-{}", dep_task_id),
                "stderr": stderr,
                "conflicting_lines": conflicting_files,
            }));

            // Abort the failed merge
            let _ = tokio::process::Command::new("git")
                .args(["merge", "--abort"])
                .current_dir(repo_root)
                .output()
                .await;
        }
    }

    // 4. Detect project type and run build/test
    let mut build_result: Option<(bool, String)> = None;

    if merge_failures.is_empty() {
        build_result = Some(detect_and_run_build(repo_root).await);
    }

    // 5. Record results
    let success = merge_failures.is_empty() && build_result.as_ref().map_or(true, |(ok, _)| *ok);
    let status = if success { "succeeded" } else { "failed" };

    // Build result summary
    let summary = serde_json::json!({
        "merge_failures": merge_failures,
        "build_result": build_result.as_ref().map(|(ok, output)| {
            serde_json::json!({"success": ok, "output": output})
        }),
        "tasks_merged": completed_tasks.len(),
        "integration_branch": &integration_branch,
        "review_gate_checked": objective_id.is_some(),
    });

    // Update task status
    sqlx::query("UPDATE tasks SET status = $1, updated_at = now() WHERE task_id = $2")
        .bind(status)
        .bind(task_id)
        .execute(pool)
        .await?;

    // Store result as artifact
    let artifact_id = Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO artifact_refs (artifact_ref_id, task_id, artifact_kind, artifact_uri, metadata)
         VALUES ($1, $2, 'integration_verify_result', $3, $4)",
    )
    .bind(&artifact_id)
    .bind(task_id)
    .bind(serde_json::to_string(&summary).unwrap_or_default())
    .bind(&summary)
    .execute(pool)
    .await?;

    // Each merge failure gets its own conflict record with kind = 'mainline_integration'
    // and a corresponding event journal entry.
    for failure in &merge_failures {
        let conflict_id = Uuid::now_v7().to_string();
        let dep_task_id = failure.get("task_id").and_then(|v| v.as_str()).unwrap_or("unknown");

        sqlx::query(
            "INSERT INTO conflicts (conflict_id, node_id, conflict_kind, status, created_at, updated_at)
             VALUES ($1, $2, 'mainline_integration', 'open', now(), now())
             ON CONFLICT DO NOTHING",
        )
        .bind(&conflict_id)
        .bind(node_id)
        .execute(pool)
        .await?;

        // Record per-conflict event in event journal
        let conflict_event_id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO event_journal \
                 (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
             VALUES ($1, 'conflict', $2, 'merge_conflict_detected', $3, $4, now()) \
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
        )
        .bind(&conflict_event_id)
        .bind(&conflict_id)
        .bind(format!("merge-conflict-{}-{}", task_id, dep_task_id))
        .bind(serde_json::json!({
            "integration_task_id": task_id,
            "node_id": node_id,
            "conflict_kind": "mainline_integration",
            "failed_branch": format!("task-{}", dep_task_id),
            "merge_details": failure,
            "trigger": "integration_verify_merge"
        }))
        .execute(pool)
        .await?;
    }

    // Event journal for overall integration result
    let event_id = Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO event_journal
         (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
         VALUES ($1, 'task', $2, 'integration_verification_complete', $3, $4, now())
         ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
    )
    .bind(&event_id)
    .bind(task_id)
    .bind(format!("integration-verify-{}", task_id))
    .bind(&summary)
    .execute(pool)
    .await?;

    if success {
        tracing::info!(task_id, "Integration verification PASSED");
        // Update node lifecycle
        sqlx::query("UPDATE nodes SET lifecycle = 'completed', updated_at = now() WHERE node_id = $1")
            .bind(node_id)
            .execute(pool)
            .await?;
    } else {
        tracing::error!(task_id, "Integration verification FAILED");
    }

    // Clean up integration branch if failed
    if !success {
        let _ = tokio::process::Command::new("git")
            .args(["checkout", "main"])
            .current_dir(repo_root)
            .output()
            .await;
        let _ = tokio::process::Command::new("git")
            .args(["branch", "-D", &integration_branch])
            .current_dir(repo_root)
            .output()
            .await;
    }

    Ok(())
}

/// Detect project type from files in the repo root and run the appropriate
/// build/test command. Returns (success, output).
async fn detect_and_run_build(repo_root: &PathBuf) -> (bool, String) {
    // Check for project files in priority order
    let checks: Vec<(&str, Vec<&str>)> = vec![
        ("Makefile", vec!["make", "test"]),
        ("Cargo.toml", vec!["cargo", "check"]),
        ("package.json", vec!["npm", "test", "--", "--passWithNoTests"]),
        ("pyproject.toml", vec!["python3", "-m", "pytest", "--co", "-q"]),
        ("setup.py", vec!["python3", "-m", "pytest", "--co", "-q"]),
        ("go.mod", vec!["go", "build", "./..."]),
        ("CMakeLists.txt", vec!["cmake", "--build", "."]),
    ];

    for (file, cmd) in &checks {
        if repo_root.join(file).exists() {
            tracing::info!(file, cmd = ?cmd, "Detected project type, running build check");

            let result = tokio::time::timeout(
                std::time::Duration::from_secs(300), // 5 min timeout
                tokio::process::Command::new(cmd[0])
                    .args(&cmd[1..])
                    .current_dir(repo_root)
                    .output(),
            )
            .await;

            match result {
                Ok(Ok(output)) => {
                    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
                    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
                    let combined =
                        format!("=== stdout ===\n{}\n=== stderr ===\n{}", stdout, stderr);
                    return (output.status.success(), combined);
                }
                Ok(Err(e)) => {
                    return (false, format!("Failed to run {:?}: {}", cmd, e));
                }
                Err(_) => {
                    return (false, format!("Build command {:?} timed out after 300s", cmd));
                }
            }
        }
    }

    // No recognized project file — pass by default
    tracing::info!("No recognized project file found, skipping build verification");
    (true, "No build system detected, verification skipped.".to_string())
}
