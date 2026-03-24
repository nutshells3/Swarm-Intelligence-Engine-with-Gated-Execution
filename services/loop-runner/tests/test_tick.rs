//! Integration tests for the orchestration tick logic.
//!
//! These tests run against the real PostgreSQL database (development_swarm).
//! Each test uses unique IDs and cleans up after itself.

use scaling::ScalingContext;
use sqlx::postgres::PgPoolOptions;
use sqlx::{PgPool, Row};
use std::path::PathBuf;
use std::time::Duration;
use uuid::Uuid;

/// Connect to the test database. Reuses the development_swarm DB.
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

/// Build a standalone-tier ScalingContext for tests.
async fn test_scaling_ctx(pool: &PgPool) -> ScalingContext {
    let repo_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    ScalingContext::from_config(
        scaling::ScalingConfig::default(),
        pool.clone(),
        repo_root,
    )
    .await
    .expect("Failed to build test ScalingContext")
}

/// Generate a unique test ID prefix to avoid collisions between tests.
fn test_id() -> String {
    format!("test-{}", Uuid::now_v7())
}

/// Clean up test data by objective_id. Cascades through loops, cycles, nodes, tasks, etc.
async fn cleanup_objective(pool: &PgPool, objective_id: &str) {
    // Delete in reverse dependency order
    let _ = sqlx::query("DELETE FROM certification_result_projections WHERE submission_id IN (SELECT submission_id FROM certification_submissions WHERE candidate_id IN (SELECT candidate_id FROM certification_candidates WHERE node_id IN (SELECT node_id FROM nodes WHERE objective_id = $1)))")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM certification_submissions WHERE candidate_id IN (SELECT candidate_id FROM certification_candidates WHERE node_id IN (SELECT node_id FROM nodes WHERE objective_id = $1))")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM certification_candidates WHERE node_id IN (SELECT node_id FROM nodes WHERE objective_id = $1)")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM conflict_artifacts WHERE conflict_id IN (SELECT conflict_id FROM conflicts WHERE node_id IN (SELECT node_id FROM nodes WHERE objective_id = $1))")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM conflicts WHERE node_id IN (SELECT node_id FROM nodes WHERE objective_id = $1)")
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
    let _ = sqlx::query("DELETE FROM certification_refs WHERE node_id IN (SELECT node_id FROM nodes WHERE objective_id = $1)")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM review_artifacts WHERE target_ref = $1")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM plan_gates WHERE plan_id IN (SELECT plan_id FROM plans WHERE objective_id = $1)")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM plan_invariants WHERE objective_id = $1")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM risk_register WHERE objective_id = $1")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM unresolved_questions WHERE objective_id = $1")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM plans WHERE objective_id = $1")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM milestone_nodes WHERE tree_id IN (SELECT tree_id FROM milestone_trees WHERE objective_id = $1)")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM milestone_trees WHERE objective_id = $1")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM acceptance_criteria WHERE owner_id = $1")
        .bind(objective_id).execute(pool).await;
    // Clean event journal entries for test loops/cycles
    let _ = sqlx::query("DELETE FROM event_journal WHERE payload::text LIKE $1")
        .bind(format!("%{}%", objective_id)).execute(pool).await;
    let _ = sqlx::query("DELETE FROM cycles WHERE loop_id IN (SELECT loop_id FROM loops WHERE objective_id = $1)")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM loops WHERE objective_id = $1")
        .bind(objective_id).execute(pool).await;
    let _ = sqlx::query("DELETE FROM objectives WHERE objective_id = $1")
        .bind(objective_id).execute(pool).await;
}

// ── Test 1: Create loops for new objectives ──────────────────────────────

#[tokio::test]
async fn test_create_loops_for_new_objectives() {
    let pool = test_pool().await;
    let scaling_ctx = test_scaling_ctx(&pool).await;
    let obj_id = test_id();

    // Seed: insert an objective with no loop
    sqlx::query(
        "INSERT INTO objectives (objective_id, summary, planning_status, plan_gate, created_at, updated_at)
         VALUES ($1, 'Test objective for loop creation', 'active', 'open', now(), now())",
    )
    .bind(&obj_id)
    .execute(&pool)
    .await
    .expect("Failed to insert objective");

    // Verify no loop exists yet
    let before: Option<String> = sqlx::query_scalar(
        "SELECT loop_id FROM loops WHERE objective_id = $1",
    )
    .bind(&obj_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(before.is_none(), "Loop should not exist before tick");

    // Run tick
    let actions = loop_runner::tick::tick(&pool, &scaling_ctx).await.expect("tick failed");
    assert!(actions > 0, "Tick should have created at least one action");

    // Verify loop was created
    let after: Option<String> = sqlx::query_scalar(
        "SELECT loop_id FROM loops WHERE objective_id = $1",
    )
    .bind(&obj_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(after.is_some(), "Loop should exist after tick");

    // Verify event was recorded
    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM event_journal
         WHERE aggregate_kind = 'loop'
           AND idempotency_key = $1",
    )
    .bind(format!("auto-loop-for-{}", obj_id))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(event_count, 1, "Exactly one loop_created event expected");

    cleanup_objective(&pool, &obj_id).await;
}

// ── Test 2: Create cycles for active loops ───────────────────────────────

#[tokio::test]
async fn test_create_cycles_for_active_loops() {
    let pool = test_pool().await;
    let scaling_ctx = test_scaling_ctx(&pool).await;
    let obj_id = test_id();
    let loop_id = Uuid::now_v7().to_string();

    // Seed: objective + loop with no cycles
    sqlx::query(
        "INSERT INTO objectives (objective_id, summary, planning_status, plan_gate, created_at, updated_at)
         VALUES ($1, 'Test for cycle creation', 'active', 'open', now(), now())",
    )
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO loops (loop_id, objective_id, cycle_index, active_track, created_at, updated_at)
         VALUES ($1, $2, 0, 'main', now(), now())",
    )
    .bind(&loop_id)
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    // Verify no cycle exists
    let before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM cycles WHERE loop_id = $1",
    )
    .bind(&loop_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(before, 0);

    // Run tick
    loop_runner::tick::tick(&pool, &scaling_ctx).await.expect("tick failed");

    // Verify cycle was created in intake (then quickly advanced to plan_elaboration)
    let cycle_row = sqlx::query(
        "SELECT cycle_id, phase FROM cycles WHERE loop_id = $1",
    )
    .bind(&loop_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(cycle_row.is_some(), "Cycle should exist after tick");

    let cycle = cycle_row.unwrap();
    let phase: String = cycle.get("phase");
    // Tick creates cycle in intake, then same tick advances it to plan_elaboration
    assert!(
        phase == "intake" || phase == "plan_elaboration" || phase == "decomposition",
        "Cycle should be in intake, plan_elaboration, or decomposition, got: {}",
        phase
    );

    // Verify loop cycle_index was updated
    let updated_idx: i32 = sqlx::query_scalar(
        "SELECT cycle_index FROM loops WHERE loop_id = $1",
    )
    .bind(&loop_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(updated_idx, 1, "cycle_index should be 1 after first cycle");

    cleanup_objective(&pool, &obj_id).await;
}

// ── Test 3: Plan gate blocks dispatch when conditions not met ────────────

#[tokio::test]
async fn test_plan_gate_blocks_dispatch() {
    let pool = test_pool().await;
    let scaling_ctx = test_scaling_ctx(&pool).await;
    let obj_id = test_id();
    let loop_id = Uuid::now_v7().to_string();
    let cycle_id = Uuid::now_v7().to_string();

    // Seed: objective with empty summary (gate condition will fail)
    sqlx::query(
        "INSERT INTO objectives (objective_id, summary, planning_status, plan_gate, created_at, updated_at)
         VALUES ($1, '', 'active', 'open', now(), now())",
    )
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO loops (loop_id, objective_id, cycle_index, active_track, created_at, updated_at)
         VALUES ($1, $2, 1, 'main', now(), now())",
    )
    .bind(&loop_id)
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO cycles (cycle_id, loop_id, phase, policy_snapshot, created_at, updated_at)
         VALUES ($1, $2, 'plan_elaboration', '{}'::jsonb, now(), now())",
    )
    .bind(&cycle_id)
    .bind(&loop_id)
    .execute(&pool)
    .await
    .unwrap();

    // Run tick -- gate should NOT be satisfied (no summary, no arch, no milestones, etc.)
    loop_runner::tick::tick(&pool, &scaling_ctx).await.expect("tick failed");

    // Verify cycle is still in plan_elaboration (not advanced to decomposition)
    let phase: String = sqlx::query_scalar(
        "SELECT phase FROM cycles WHERE cycle_id = $1",
    )
    .bind(&cycle_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        phase, "plan_elaboration",
        "Cycle should remain in plan_elaboration when gate is not satisfied"
    );

    // Verify plan gate was created and is still open
    let gate_status: Option<String> = sqlx::query_scalar(
        "SELECT pg.current_status FROM plan_gates pg
         JOIN plans p ON p.plan_id = pg.plan_id
         WHERE p.objective_id = $1",
    )
    .bind(&obj_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert_eq!(gate_status.as_deref(), Some("open"), "Gate should be open");

    cleanup_objective(&pool, &obj_id).await;
}

// ── Test 4: Plan gate allows dispatch when conditions met ────────────────

#[tokio::test]
async fn test_plan_gate_allows_dispatch() {
    let pool = test_pool().await;
    let scaling_ctx = test_scaling_ctx(&pool).await;
    let obj_id = test_id();
    let loop_id = Uuid::now_v7().to_string();
    let cycle_id = Uuid::now_v7().to_string();
    let node_id = Uuid::now_v7().to_string();

    // Seed: objective with full data to satisfy gate
    sqlx::query(
        "INSERT INTO objectives (objective_id, summary, planning_status, plan_gate, architecture_summary, created_at, updated_at)
         VALUES ($1, 'Implement feature X with comprehensive tests', 'active', 'open', 'Microservice arch with REST API', now(), now())",
    )
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO loops (loop_id, objective_id, cycle_index, active_track, created_at, updated_at)
         VALUES ($1, $2, 1, 'main', now(), now())",
    )
    .bind(&loop_id)
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO cycles (cycle_id, loop_id, phase, policy_snapshot, created_at, updated_at)
         VALUES ($1, $2, 'plan_elaboration', '{}'::jsonb, now(), now())",
    )
    .bind(&cycle_id)
    .bind(&loop_id)
    .execute(&pool)
    .await
    .unwrap();

    // Satisfy gate conditions: create a node (has_nodes triggers gate pass)
    sqlx::query(
        "INSERT INTO nodes (node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at)
         VALUES ($1, $2, 'Implement REST endpoints', 'Create CRUD endpoints', 'implementation', 'proposed', now(), now())",
    )
    .bind(&node_id)
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    // Run tick
    loop_runner::tick::tick(&pool, &scaling_ctx).await.expect("tick failed");

    // Verify cycle advanced past plan_elaboration
    let phase: String = sqlx::query_scalar(
        "SELECT phase FROM cycles WHERE cycle_id = $1",
    )
    .bind(&cycle_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_ne!(
        phase, "plan_elaboration",
        "Cycle should have advanced past plan_elaboration (got: {})",
        phase
    );

    cleanup_objective(&pool, &obj_id).await;
}

// ── Test 5: Decompose creates tasks ──────────────────────────────────────

#[tokio::test]
async fn test_decompose_creates_tasks() {
    let pool = test_pool().await;
    let scaling_ctx = test_scaling_ctx(&pool).await;
    let obj_id = test_id();
    let loop_id = Uuid::now_v7().to_string();
    let cycle_id = Uuid::now_v7().to_string();
    let node_id = Uuid::now_v7().to_string();

    // Seed: objective + loop + cycle in decomposition + a node without a task
    sqlx::query(
        "INSERT INTO objectives (objective_id, summary, planning_status, plan_gate, created_at, updated_at)
         VALUES ($1, 'Test decomposition', 'active', 'open', now(), now())",
    )
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO loops (loop_id, objective_id, cycle_index, active_track, created_at, updated_at)
         VALUES ($1, $2, 1, 'main', now(), now())",
    )
    .bind(&loop_id)
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO cycles (cycle_id, loop_id, phase, policy_snapshot, created_at, updated_at)
         VALUES ($1, $2, 'decomposition', '{}'::jsonb, now(), now())",
    )
    .bind(&cycle_id)
    .bind(&loop_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO nodes (node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at)
         VALUES ($1, $2, 'Write unit tests', 'Create tests for module X', 'verification', 'proposed', now(), now())",
    )
    .bind(&node_id)
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    // Verify no tasks exist for this node
    let before: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tasks WHERE node_id = $1",
    )
    .bind(&node_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(before, 0);

    // Run tick
    loop_runner::tick::tick(&pool, &scaling_ctx).await.expect("tick failed");

    // Verify task was created for the node
    let task_row = sqlx::query(
        "SELECT task_id, worker_role, status FROM tasks WHERE node_id = $1",
    )
    .bind(&node_id)
    .fetch_optional(&pool)
    .await
    .unwrap();
    assert!(task_row.is_some(), "Task should have been created for the node");

    let task = task_row.unwrap();
    let worker_role: String = task.get("worker_role");
    assert_eq!(worker_role, "reviewer", "verification lane should map to reviewer role");

    // Status should be running (no dependencies)
    let status: String = task.get("status");
    assert_eq!(status, "running", "Task with no deps should start as running");

    cleanup_objective(&pool, &obj_id).await;
}

// ── Test 6: Execution completion advances to integration ─────────────────

#[tokio::test]
async fn test_execution_completion() {
    let pool = test_pool().await;
    let scaling_ctx = test_scaling_ctx(&pool).await;
    let obj_id = test_id();
    let loop_id = Uuid::now_v7().to_string();
    let cycle_id = Uuid::now_v7().to_string();
    let node_id = Uuid::now_v7().to_string();
    let task_id = Uuid::now_v7().to_string();

    // Seed: full chain with cycle in execution, one succeeded task
    sqlx::query(
        "INSERT INTO objectives (objective_id, summary, planning_status, plan_gate, created_at, updated_at)
         VALUES ($1, 'Test execution completion', 'active', 'open', now(), now())",
    )
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO loops (loop_id, objective_id, cycle_index, active_track, created_at, updated_at)
         VALUES ($1, $2, 1, 'main', now(), now())",
    )
    .bind(&loop_id)
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO cycles (cycle_id, loop_id, phase, policy_snapshot, created_at, updated_at)
         VALUES ($1, $2, 'execution', '{}'::jsonb, now(), now())",
    )
    .bind(&cycle_id)
    .bind(&loop_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO nodes (node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at)
         VALUES ($1, $2, 'Build module', 'Implement core module', 'implementation', 'completed', now(), now())",
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

    // Run tick
    loop_runner::tick::tick(&pool, &scaling_ctx).await.expect("tick failed");

    // Verify cycle advanced past execution
    let phase: String = sqlx::query_scalar(
        "SELECT phase FROM cycles WHERE cycle_id = $1",
    )
    .bind(&cycle_id)
    .fetch_one(&pool)
    .await
    .unwrap();

    assert!(
        phase == "integration" || phase == "state_update" || phase == "next_cycle_ready",
        "Cycle should have advanced past execution to integration or beyond, got: {}",
        phase
    );

    cleanup_objective(&pool, &obj_id).await;
}

// ── Test 7: Conflict detection ───────────────────────────────────────────

#[tokio::test]
async fn test_conflict_detection() {
    let pool = test_pool().await;
    let scaling_ctx = test_scaling_ctx(&pool).await;
    let obj_id = test_id();
    let node_id = Uuid::now_v7().to_string();
    let task1_id = Uuid::now_v7().to_string();
    let task2_id = Uuid::now_v7().to_string();

    // Seed: objective + node + TWO succeeded tasks (triggers divergence conflict)
    sqlx::query(
        "INSERT INTO objectives (objective_id, summary, planning_status, plan_gate, created_at, updated_at)
         VALUES ($1, 'Test conflict detection', 'active', 'open', now(), now())",
    )
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO nodes (node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at)
         VALUES ($1, $2, 'Shared module', 'Build shared code', 'implementation', 'completed', now(), now())",
    )
    .bind(&node_id)
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    // Two tasks for same node, both succeeded
    sqlx::query(
        "INSERT INTO tasks (task_id, node_id, worker_role, skill_pack_id, status, created_at, updated_at)
         VALUES ($1, $2, 'implementer', 'default', 'succeeded', now(), now())",
    )
    .bind(&task1_id)
    .bind(&node_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO tasks (task_id, node_id, worker_role, skill_pack_id, status, created_at, updated_at)
         VALUES ($1, $2, 'implementer', 'default', 'succeeded', now(), now())",
    )
    .bind(&task2_id)
    .bind(&node_id)
    .execute(&pool)
    .await
    .unwrap();

    // Run tick
    loop_runner::tick::tick(&pool, &scaling_ctx).await.expect("tick failed");

    // Verify conflict was created
    let conflict_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM conflicts WHERE node_id = $1 AND conflict_kind = 'divergence'",
    )
    .bind(&node_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(conflict_count > 0, "Divergence conflict should have been created");

    cleanup_objective(&pool, &obj_id).await;
}

// ── Test 8: Drift detection ─────────────────────────────────────────────

#[tokio::test]
async fn test_drift_detection() {
    let pool = test_pool().await;
    let scaling_ctx = test_scaling_ctx(&pool).await;
    let obj_id = test_id();
    let upstream_node_id = Uuid::now_v7().to_string();
    let downstream_node_id = Uuid::now_v7().to_string();
    let cert_ref_id = Uuid::now_v7().to_string();

    // Seed: two nodes with an edge, downstream has a valid certification,
    // then upstream is modified after certification
    sqlx::query(
        "INSERT INTO objectives (objective_id, summary, planning_status, plan_gate, created_at, updated_at)
         VALUES ($1, 'Test drift detection', 'active', 'open', now(), now())",
    )
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO nodes (node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at)
         VALUES ($1, $2, 'Upstream node', 'Defines API', 'planning', 'completed', now() - interval '10 minutes', now())",
    )
    .bind(&upstream_node_id)
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO nodes (node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at)
         VALUES ($1, $2, 'Downstream node', 'Implements API', 'implementation', 'completed', now() - interval '10 minutes', now())",
    )
    .bind(&downstream_node_id)
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    // Edge: upstream -> downstream
    sqlx::query(
        "INSERT INTO node_edges (edge_id, from_node_id, to_node_id, edge_kind)
         VALUES ($1, $2, $3, 'depends_on')",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&upstream_node_id)
    .bind(&downstream_node_id)
    .execute(&pool)
    .await
    .unwrap();

    // Certification on downstream created 5 minutes ago
    sqlx::query(
        "INSERT INTO certification_refs (certification_ref_id, node_id, external_system, external_ref, gate, status, created_at)
         VALUES ($1, $2, 'test', 'test-ref', 'audited', 'valid', now() - interval '5 minutes')",
    )
    .bind(&cert_ref_id)
    .bind(&downstream_node_id)
    .execute(&pool)
    .await
    .unwrap();

    // Now update upstream AFTER the certification (simulates change)
    sqlx::query(
        "UPDATE nodes SET updated_at = now() WHERE node_id = $1",
    )
    .bind(&upstream_node_id)
    .execute(&pool)
    .await
    .unwrap();

    // Run tick
    loop_runner::tick::tick(&pool, &scaling_ctx).await.expect("tick failed");

    // Verify certification is now stale
    let cert_status: String = sqlx::query_scalar(
        "SELECT status FROM certification_refs WHERE certification_ref_id = $1",
    )
    .bind(&cert_ref_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(cert_status, "stale", "Certification should be marked stale after upstream change");

    // Verify drift event was emitted
    let drift_events: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM event_journal
         WHERE aggregate_kind = 'drift'
           AND aggregate_id = $1
           AND event_kind = 'drift_detected'",
    )
    .bind(&downstream_node_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(drift_events > 0, "Drift event should have been emitted");

    // Cleanup
    let _ = sqlx::query("DELETE FROM event_journal WHERE aggregate_kind = 'drift' AND aggregate_id = $1")
        .bind(&downstream_node_id).execute(&pool).await;
    let _ = sqlx::query("DELETE FROM certification_refs WHERE certification_ref_id = $1")
        .bind(&cert_ref_id).execute(&pool).await;
    let _ = sqlx::query("DELETE FROM node_edges WHERE from_node_id = $1 OR to_node_id = $1")
        .bind(&upstream_node_id).execute(&pool).await;
    cleanup_objective(&pool, &obj_id).await;
}

// ── Test 9: Certification candidate selection ────────────────────────────

#[tokio::test]
async fn test_certification_candidate_selection() {
    let pool = test_pool().await;
    let scaling_ctx = test_scaling_ctx(&pool).await;
    let obj_id = test_id();
    let node_id = Uuid::now_v7().to_string();
    let task_id = Uuid::now_v7().to_string();

    // Seed: succeeded task whose node title contains "contract" (triggers candidate)
    sqlx::query(
        "INSERT INTO objectives (objective_id, summary, planning_status, plan_gate, created_at, updated_at)
         VALUES ($1, 'Test cert candidate selection', 'active', 'open', now(), now())",
    )
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO nodes (node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at)
         VALUES ($1, $2, 'Verify contract invariants', 'Check all contract conditions', 'verification', 'completed', now(), now())",
    )
    .bind(&node_id)
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO tasks (task_id, node_id, worker_role, skill_pack_id, status, created_at, updated_at)
         VALUES ($1, $2, 'reviewer', 'default', 'succeeded', now(), now())",
    )
    .bind(&task_id)
    .bind(&node_id)
    .execute(&pool)
    .await
    .unwrap();

    // Run tick
    loop_runner::tick::tick(&pool, &scaling_ctx).await.expect("tick failed");

    // Verify certification candidate was created
    let candidate_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM certification_candidates WHERE task_id = $1",
    )
    .bind(&task_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(candidate_count > 0, "Certification candidate should have been created for contract-related task");

    cleanup_objective(&pool, &obj_id).await;
}

// ── Test 10: Idempotent tick ─────────────────────────────────────────────

#[tokio::test]
async fn test_idempotent_tick() {
    let pool = test_pool().await;
    let scaling_ctx = test_scaling_ctx(&pool).await;
    let obj_id = test_id();

    // Seed: objective with no loop
    sqlx::query(
        "INSERT INTO objectives (objective_id, summary, planning_status, plan_gate, created_at, updated_at)
         VALUES ($1, 'Test idempotency', 'active', 'open', now(), now())",
    )
    .bind(&obj_id)
    .execute(&pool)
    .await
    .unwrap();

    // Run tick TWICE
    loop_runner::tick::tick(&pool, &scaling_ctx).await.expect("first tick failed");
    loop_runner::tick::tick(&pool, &scaling_ctx).await.expect("second tick failed");

    // Verify exactly ONE loop was created (not two)
    let loop_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM loops WHERE objective_id = $1",
    )
    .bind(&obj_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(loop_count, 1, "Idempotent tick should not create duplicate loops");

    // Verify exactly one loop_created event
    let event_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM event_journal
         WHERE aggregate_kind = 'loop'
           AND idempotency_key = $1",
    )
    .bind(format!("auto-loop-for-{}", obj_id))
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(event_count, 1, "Should have exactly one loop_created event");

    cleanup_objective(&pool, &obj_id).await;
}
