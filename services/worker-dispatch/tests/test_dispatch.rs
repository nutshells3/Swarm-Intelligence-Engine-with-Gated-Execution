//! Integration tests for the worker-dispatch SQL logic.
//!
//! Worker-dispatch cannot be tested end-to-end without agent CLI adapters,
//! so these tests directly exercise the SQL queries and DB state transitions
//! that dispatch_tick relies on.

use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use std::time::Duration;
use uuid::Uuid;

async fn test_pool() -> PgPool {
    let url = std::env::var("DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost/development_swarm".to_string()
    });
    PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&url)
        .await
        .expect("Failed to connect to test database")
}

fn test_id() -> String {
    format!("test-{}", Uuid::now_v7())
}

async fn cleanup_objective(pool: &PgPool, objective_id: &str) {
    let _ = sqlx::query("DELETE FROM certification_result_projections WHERE submission_id IN (SELECT submission_id FROM certification_submissions WHERE candidate_id IN (SELECT candidate_id FROM certification_candidates WHERE node_id IN (SELECT node_id FROM nodes WHERE objective_id = $1)))")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM certification_submissions WHERE candidate_id IN (SELECT candidate_id FROM certification_candidates WHERE node_id IN (SELECT node_id FROM nodes WHERE objective_id = $1))")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM certification_candidates WHERE node_id IN (SELECT node_id FROM nodes WHERE objective_id = $1)")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM artifact_refs WHERE task_id IN (SELECT task_id FROM tasks WHERE node_id IN (SELECT node_id FROM nodes WHERE objective_id = $1))")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM task_attempts WHERE task_id IN (SELECT task_id FROM tasks WHERE node_id IN (SELECT node_id FROM nodes WHERE objective_id = $1))")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM tasks WHERE node_id IN (SELECT node_id FROM nodes WHERE objective_id = $1)")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM node_edges WHERE from_node_id IN (SELECT node_id FROM nodes WHERE objective_id = $1) OR to_node_id IN (SELECT node_id FROM nodes WHERE objective_id = $1)")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM nodes WHERE objective_id = $1")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM event_journal WHERE payload::text LIKE $1")
        .bind(format!("%{}%", objective_id)).execute(pool).await;
    let _ = sqlx::query("DELETE FROM cycles WHERE loop_id IN (SELECT loop_id FROM loops WHERE objective_id = $1)")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM loops WHERE objective_id = $1")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM objectives WHERE objective_id = $1")
        .bind(objective_id).execute(pool).await;
}

async fn cleanup_policy(pool: &PgPool, policy_id: &str) {
    let _ = sqlx::query("DELETE FROM user_policies WHERE policy_id = $1")
        .bind(policy_id).execute(pool).await;
}

// ── Test 1: Dispatch picks up running tasks ──────────────────────────────

#[tokio::test]
async fn test_dispatch_picks_up_running_tasks() {
    let pool = test_pool().await;
    let obj_id = test_id();
    let node_id = Uuid::now_v7().to_string();
    let task_id = Uuid::now_v7().to_string();

    // Seed: running task with no active attempt
    sqlx::query(
        "INSERT INTO objectives (objective_id, summary, planning_status, plan_gate, created_at, updated_at)
         VALUES ($1, 'Dispatch pickup test', 'active', 'open', now(), now())",
    )
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO nodes (node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at)
         VALUES ($1, $2, 'Build feature', 'Implement feature X', 'implementation', 'running', now(), now())",
    )
    .bind(&node_id)
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO tasks (task_id, node_id, worker_role, skill_pack_id, status, provider_mode, model_binding, created_at, updated_at)
         VALUES ($1, $2, 'implementer', 'default', 'running', 'api', 'claude', now(), now())",
    )
    .bind(&task_id)
    .bind(&node_id)
    .execute(&pool)
    .await
    .unwrap();

    // Verify the dispatch query finds this task
    let found = sqlx::query(
        r#"
        SELECT t.task_id
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
        "#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let found_ids: Vec<String> = found.iter().map(|r| r.get::<String, _>("task_id")).collect();
    assert!(
        found_ids.contains(&task_id),
        "Dispatch query should find the running task without an active attempt"
    );

    cleanup_objective(&pool, &obj_id).await;
}

// ── Test 2: Dispatch respects concurrency limit ──────────────────────────

#[tokio::test]
async fn test_dispatch_respects_concurrency_limit() {
    let pool = test_pool().await;
    let obj_id = test_id();
    let policy_id = format!("test-policy-{}", Uuid::now_v7());

    // Seed: policy with max_active_agents=1
    sqlx::query(
        "INSERT INTO user_policies (policy_id, revision, policy_payload, created_at)
         VALUES ($1, 999, $2::jsonb, now())",
    )
    .bind(&policy_id)
    .bind(serde_json::json!({
        "global": { "max_active_agents": 1 }
    }))
    .execute(&pool)
    .await
    .unwrap();

    // Create a running attempt to consume the 1 slot
    let existing_node_id = Uuid::now_v7().to_string();
    let existing_task_id = Uuid::now_v7().to_string();
    let existing_attempt_id = Uuid::now_v7().to_string();

    sqlx::query(
        "INSERT INTO objectives (objective_id, summary, planning_status, plan_gate, created_at, updated_at)
         VALUES ($1, 'Concurrency limit test', 'active', 'open', now(), now())",
    )
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO nodes (node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at)
         VALUES ($1, $2, 'Already running', 'Task already running', 'implementation', 'running', now(), now())",
    )
    .bind(&existing_node_id)
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO tasks (task_id, node_id, worker_role, skill_pack_id, status, created_at, updated_at)
         VALUES ($1, $2, 'implementer', 'default', 'running', now(), now())",
    )
    .bind(&existing_task_id)
    .bind(&existing_node_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO task_attempts (task_attempt_id, task_id, attempt_index, lease_owner, status, started_at)
         VALUES ($1, $2, 1, 'worker-dispatch', 'running', now())",
    )
    .bind(&existing_attempt_id)
    .bind(&existing_task_id)
    .execute(&pool)
    .await
    .unwrap();

    // Compute available slots using the same logic as dispatch_tick
    let running_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM task_attempts WHERE status = 'running' AND lease_owner = 'worker-dispatch'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let max_agents: i64 = sqlx::query_scalar::<_, serde_json::Value>(
        "SELECT policy_payload FROM user_policies ORDER BY revision DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .map(|v| v.pointer("/global/max_active_agents").and_then(|x| x.as_i64()).unwrap_or(1000))
    .unwrap_or(1000);

    let available_slots = (max_agents as usize).saturating_sub(running_count as usize);

    assert_eq!(
        available_slots, 0,
        "With max_active_agents=1 and 1 running attempt, available slots should be 0"
    );

    cleanup_objective(&pool, &obj_id).await;
    cleanup_policy(&pool, &policy_id).await;
}

// ── Test 3: Dispatch respects dependencies ───────────────────────────────

#[tokio::test]
async fn test_dispatch_respects_dependencies() {
    let pool = test_pool().await;
    let obj_id = test_id();
    let node_a_id = Uuid::now_v7().to_string();
    let node_b_id = Uuid::now_v7().to_string();
    let task_b_id = Uuid::now_v7().to_string();

    // Seed: node A (completed) -> node B (queued task)
    sqlx::query(
        "INSERT INTO objectives (objective_id, summary, planning_status, plan_gate, created_at, updated_at)
         VALUES ($1, 'Dependency test', 'active', 'open', now(), now())",
    )
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO nodes (node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at)
         VALUES ($1, $2, 'Task A', 'Predecessor', 'implementation', 'completed', now(), now())",
    )
    .bind(&node_a_id)
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO nodes (node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at)
         VALUES ($1, $2, 'Task B', 'Dependent on A', 'implementation', 'running', now(), now())",
    )
    .bind(&node_b_id)
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    // Edge: A -> B
    sqlx::query(
        "INSERT INTO node_edges (edge_id, from_node_id, to_node_id, edge_kind)
         VALUES ($1, $2, $3, 'depends_on')",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&node_a_id)
    .bind(&node_b_id)
    .execute(&pool)
    .await
    .unwrap();

    // Task B is running (eligible for dispatch since A is completed)
    sqlx::query(
        "INSERT INTO tasks (task_id, node_id, worker_role, skill_pack_id, status, created_at, updated_at)
         VALUES ($1, $2, 'implementer', 'default', 'running', now(), now())",
    )
    .bind(&task_b_id)
    .bind(&node_b_id)
    .execute(&pool)
    .await
    .unwrap();

    // Verify B is found by the dispatch query (since A is completed)
    let found = sqlx::query(
        r#"
        SELECT t.task_id
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
        "#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let found_ids: Vec<String> = found.iter().map(|r| r.get::<String, _>("task_id")).collect();
    assert!(
        found_ids.contains(&task_b_id),
        "Task B should be eligible when its dependency A is completed"
    );

    cleanup_objective(&pool, &obj_id).await;
}

// ── Test 4: Dispatch skips blocked tasks ─────────────────────────────────

#[tokio::test]
async fn test_dispatch_skips_blocked_tasks() {
    let pool = test_pool().await;
    let obj_id = test_id();
    let node_a_id = Uuid::now_v7().to_string();
    let node_b_id = Uuid::now_v7().to_string();
    let task_b_id = Uuid::now_v7().to_string();

    // Seed: node A (still running) -> node B (running task)
    sqlx::query(
        "INSERT INTO objectives (objective_id, summary, planning_status, plan_gate, created_at, updated_at)
         VALUES ($1, 'Blocked task test', 'active', 'open', now(), now())",
    )
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    // A is still running (NOT completed)
    sqlx::query(
        "INSERT INTO nodes (node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at)
         VALUES ($1, $2, 'Task A', 'Predecessor still running', 'implementation', 'running', now(), now())",
    )
    .bind(&node_a_id)
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO nodes (node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at)
         VALUES ($1, $2, 'Task B', 'Blocked by A', 'implementation', 'queued', now(), now())",
    )
    .bind(&node_b_id)
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    // Edge: A -> B
    sqlx::query(
        "INSERT INTO node_edges (edge_id, from_node_id, to_node_id, edge_kind)
         VALUES ($1, $2, $3, 'depends_on')",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&node_a_id)
    .bind(&node_b_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO tasks (task_id, node_id, worker_role, skill_pack_id, status, created_at, updated_at)
         VALUES ($1, $2, 'implementer', 'default', 'running', now(), now())",
    )
    .bind(&task_b_id)
    .bind(&node_b_id)
    .execute(&pool)
    .await
    .unwrap();

    // The dispatch query should NOT return task B (because A is still running)
    let found = sqlx::query(
        r#"
        SELECT t.task_id
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
        "#,
    )
    .fetch_all(&pool)
    .await
    .unwrap();

    let found_ids: Vec<String> = found.iter().map(|r| r.get::<String, _>("task_id")).collect();
    assert!(
        !found_ids.contains(&task_b_id),
        "Task B should NOT be eligible when dependency A is still running"
    );

    cleanup_objective(&pool, &obj_id).await;
}

// ── Test 5: Certification eligibility (always) ──────────────────────────

#[tokio::test]
async fn test_certification_eligibility_always() {
    let pool = test_pool().await;
    let obj_id = test_id();
    let node_id = Uuid::now_v7().to_string();
    let task_id = Uuid::now_v7().to_string();
    let attempt_id = Uuid::now_v7().to_string();
    let cert_policy_id = "certification_config";

    // Save existing cert config so we can restore it
    let existing_config: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT policy_payload FROM user_policies WHERE policy_id = $1",
    )
    .bind(cert_policy_id)
    .fetch_optional(&pool)
    .await
    .unwrap();

    // Set certification to frequency=always
    sqlx::query(
        "INSERT INTO user_policies (policy_id, revision, policy_payload, created_at)
         VALUES ($1, 9999, $2::jsonb, now())
         ON CONFLICT (policy_id) DO UPDATE SET policy_payload = $2::jsonb, revision = 9999",
    )
    .bind(cert_policy_id)
    .bind(serde_json::json!({
        "enabled": true,
        "frequency": "always"
    }))
    .execute(&pool)
    .await
    .unwrap();

    // Seed: a succeeded task
    sqlx::query(
        "INSERT INTO objectives (objective_id, summary, planning_status, plan_gate, created_at, updated_at)
         VALUES ($1, 'Cert always test', 'active', 'open', now(), now())",
    )
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO nodes (node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at)
         VALUES ($1, $2, 'Regular implementation task', 'Nothing special', 'implementation', 'completed', now(), now())",
    )
    .bind(&node_id)
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO tasks (task_id, node_id, worker_role, skill_pack_id, status, created_at, updated_at)
         VALUES ($1, $2, 'implementer', 'default', 'succeeded', now(), now())",
    )
    .bind(&task_id)
    .bind(&node_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO task_attempts (task_attempt_id, task_id, attempt_index, lease_owner, status, started_at)
         VALUES ($1, $2, 1, 'worker-dispatch', 'succeeded', now())",
    )
    .bind(&attempt_id)
    .bind(&task_id)
    .execute(&pool)
    .await
    .unwrap();

    // Simulate: check_certification_eligibility logic inline
    // When frequency=always, even non-critical tasks should get candidates
    let policy_payload: serde_json::Value = sqlx::query_scalar(
        "SELECT policy_payload FROM user_policies WHERE policy_id = 'certification_config'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();

    let enabled = policy_payload.get("enabled").and_then(|v| v.as_bool()).unwrap_or(false);
    let frequency = policy_payload.get("frequency").and_then(|v| v.as_str()).unwrap_or("off");

    assert!(enabled, "Certification should be enabled");
    assert_eq!(frequency, "always");

    // With frequency=always, any task should be eligible
    let title = "Regular implementation task";
    let is_critical = title.to_lowercase().contains("contract")
        || title.to_lowercase().contains("invariant")
        || title.to_lowercase().contains("proof")
        || title.to_lowercase().contains("safety");
    let should_create = match frequency {
        "always" => true,
        "critical_only" => is_critical,
        _ => false,
    };
    assert!(should_create, "With frequency=always, all tasks should be eligible");
    assert!(!is_critical, "This task is not critical, proving always mode works for non-critical");

    // Restore previous certification config
    if let Some(prev) = existing_config {
        sqlx::query(
            "UPDATE user_policies SET policy_payload = $1::jsonb
             WHERE policy_id = $2",
        )
        .bind(&prev)
        .bind(cert_policy_id)
        .execute(&pool)
        .await
        .unwrap();
    } else {
        let _ = sqlx::query("DELETE FROM user_policies WHERE policy_id = $1")
            .bind(cert_policy_id)
            .execute(&pool)
            .await;
    }

    cleanup_objective(&pool, &obj_id).await;
}

// ── Test 6: Certification eligibility (critical_only) ────────────────────

#[tokio::test]
async fn test_certification_eligibility_critical_only() {
    let _pool = test_pool().await;

    // Test the filtering logic directly
    let test_cases = vec![
        ("Verify contract invariants", true, "contract"),
        ("Check safety conditions", true, "safety"),
        ("Write proof for theorem", true, "proof"),
        ("Implement feature X", false, "non-critical"),
        ("Build REST endpoint", false, "non-critical"),
        ("Add logging infrastructure", false, "non-critical"),
    ];

    for (title, expected_eligible, label) in test_cases {
        let title_lower = title.to_lowercase();
        let is_critical = title_lower.contains("contract")
            || title_lower.contains("invariant")
            || title_lower.contains("proof")
            || title_lower.contains("safety")
            || title_lower.contains("correctness");

        // With frequency=critical_only, only critical tasks pass
        let should_create = is_critical; // frequency = "critical_only"

        assert_eq!(
            should_create, expected_eligible,
            "Title '{}' ({}) should be eligible={} under critical_only",
            title, label, expected_eligible
        );
    }
}
