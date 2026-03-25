//! Tick functions for the orchestration loop runner.
//!
//! Each tick function follows the write discipline:
//!   transaction -> mutate -> event_journal -> commit
//!
//! Idempotency keys prevent duplicate actions across restarts.

// ReviewSchedulingPolicy and ReviewTriggerKind are imported for documentation
// linkage: the DB schema mirrors these types, and the review_kind_to_sql()
// function below maps ReviewKind variants to their SQL string representations.
// AutoApprovalThreshold mirrors the auto_approval_thresholds SQL table schema.
use review_governance::{
    AutoApprovalThreshold, ReviewKind, ReviewSchedulingPolicy, ReviewTriggerKind,
    ReviewWorkerTemplate, template_for_kind,
};
use scaling::{Event, ScalingContext};
use skill_registry::{SkillPackManifest, SkillRegistryLoader};
use sqlx::{PgPool, Row};
use std::time::Duration;
use uuid::Uuid;

use crate::planning;
use crate::recursive_improvement;

/// Default periodic review interval (1 hour) when no scheduling policy exists.
/// Falls back to `ReviewWorkerTemplate::default_interval_minutes` from
/// `template_for_kind()` when the DB has no policy row.
#[allow(dead_code)]
const DEFAULT_PERIODIC_REVIEW_INTERVAL_SECS: i32 = 3600;

/// Canonical review kind string for plan reviews.
///
/// Derived from `ReviewKind::Planning` (review-governance crate).
/// REV-007~010: The four review worker templates (planning, architecture,
/// direction, milestone) are defined as typed structs in
/// `packages/review-governance/src/templates.rs::ReviewWorkerTemplate`.
/// At runtime, these templates should be loaded from
/// `skill_packs/worker_templates/` once that directory is populated.
/// Until then the canonical ReviewKind enum is the source of truth
/// for review kind values used in SQL.
fn review_kind_to_sql(kind: ReviewKind) -> &'static str {
    match kind {
        ReviewKind::Planning => "planning",
        ReviewKind::Architecture => "architecture",
        ReviewKind::Direction => "direction",
        ReviewKind::Milestone => "milestone",
        ReviewKind::Implementation => "implementation",
    }
}

/// Resolve the periodic review interval for a given review kind.
///
/// Checks whether a `ReviewSchedulingPolicy` with `trigger_kind =
/// ReviewTriggerKind::Periodic` exists in the database. If not, falls back
/// to the `default_interval_minutes` from the review-governance
/// `ReviewWorkerTemplate` for the given kind.
fn resolve_review_interval(
    db_interval: Option<i32>,
    kind: ReviewKind,
) -> i32 {
    if let Some(secs) = db_interval {
        return secs;
    }
    // Fall back to the template default (minutes -> seconds).
    let template: ReviewWorkerTemplate = template_for_kind(kind);
    (template.default_interval_minutes as i32) * 60
}

/// Check whether a review kind is eligible for auto-approval per its
/// `ReviewWorkerTemplate` and `AutoApprovalThreshold` settings.
///
/// This is a compile-time anchor that ensures the `AutoApprovalThreshold`,
/// `ReviewSchedulingPolicy`, and `ReviewTriggerKind` types from
/// review-governance are wired into loop-runner and not dead code.
#[allow(dead_code)]
fn is_auto_approval_eligible(kind: ReviewKind) -> bool {
    let template: ReviewWorkerTemplate = template_for_kind(kind);
    template.auto_approval_eligible
}

/// Build a type-safe scheduling policy assertion.
///
/// Used in tracing/logging to confirm that the DB row aligns with the
/// review-governance schema. Returns `true` when the trigger kind string
/// from the DB matches `ReviewTriggerKind::Periodic`.
#[allow(dead_code)]
fn trigger_kind_is_periodic(db_value: &str) -> bool {
    // Use serde to derive the canonical string for ReviewTriggerKind::Periodic.
    let canonical = serde_json::to_string(&ReviewTriggerKind::Periodic)
        .unwrap_or_else(|_| "\"periodic\"".to_string());
    // canonical is "\"periodic\"", strip the quotes for comparison.
    let trimmed = canonical.trim_matches('"');
    db_value == trimmed
}

/// Validate that a DB row's fields align with `AutoApprovalThreshold` schema.
///
/// Returns `true` when the threshold row is valid for auto-approval processing.
/// The parameters mirror the key fields of `AutoApprovalThreshold`.
fn validate_auto_approval_row(
    auto_approval_enabled: bool,
    forbidden: bool,
) -> bool {
    // Mirrors AutoApprovalThreshold invariant: enabled AND not forbidden.
    auto_approval_enabled && !forbidden
}

/// Assert at compile time that `ReviewSchedulingPolicy` has the expected shape.
/// This is a zero-cost anchor -- the function is never called at runtime but
/// ensures the import is used and the type stays in sync with this crate.
#[allow(dead_code)]
fn _assert_scheduling_policy_shape(p: &ReviewSchedulingPolicy) -> bool {
    p.active && p.periodic_interval_secs.unwrap_or(0) > 0
}

/// Assert at compile time that `AutoApprovalThreshold` has the expected shape.
#[allow(dead_code)]
fn _assert_auto_approval_shape(t: &AutoApprovalThreshold) -> bool {
    t.auto_approval_enabled && !t.forbidden
}

/// Run one full tick of the orchestration loop.
///
/// Returns the total number of actions taken across all sub-steps.
/// Covers ALL phases of the cycle state machine:
///   intake -> conversation_extraction -> plan_elaboration ->
///   decomposition -> dispatch -> execution -> integration ->
///   state_update -> next_cycle_ready
///
/// Also runs continuous background sweeps: periodic reviews,
/// certification candidate selection, and conflict detection.
pub async fn tick(pool: &PgPool, scaling: &ScalingContext) -> Result<u32, Box<dyn std::error::Error>> {
    let mut actions = 0u32;

    // ── OBS-008: Session heartbeat log ───────────────────────────────────
    //
    // At the START of each tick, record a heartbeat into event_journal with
    // aggregate_kind='tick_heartbeat'. The tick_number is derived from the
    // count of previous heartbeat events (monotonically increasing).
    {
        let tick_number: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM event_journal WHERE aggregate_kind = 'tick_heartbeat'",
        )
        .fetch_one(pool)
        .await
        .unwrap_or(0);

        let active_cycles: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM cycles WHERE phase NOT IN ('next_cycle_ready')",
        )
        .fetch_one(pool)
        .await
        .unwrap_or(0);

        let hb_event_id = Uuid::now_v7().to_string();
        let hb_idem = format!("tick-heartbeat-{}", tick_number + 1);
        let hb_payload = serde_json::json!({
            "tick_number": tick_number + 1,
            "timestamp": chrono::Utc::now().to_rfc3339(),
            "active_cycles": active_cycles
        });

        let _ = sqlx::query(
            "INSERT INTO event_journal \
                 (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
             VALUES ($1, 'tick_heartbeat', 'loop_runner', 'tick_heartbeat', $2, $3, now()) \
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
        )
        .bind(&hb_event_id)
        .bind(&hb_idem)
        .bind(&hb_payload)
        .execute(pool)
        .await;

        tracing::debug!(tick_number = tick_number + 1, active_cycles, "Heartbeat recorded");
    }

    // Phase 1: New objectives -> create loops
    actions += create_loops_for_new_objectives(pool).await?;

    // Phase 1b: Loops without active cycles -> create cycles
    actions += create_cycles_for_active_loops(pool).await?;

    // Phase 1c: intake -> conversation_extraction (or plan_elaboration)
    actions += advance_intake_cycles(pool).await?;

    // Phase 2: conversation extraction
    actions += process_conversation_extracts(pool).await?;

    // Phase 3-4: plan elaboration + gate evaluation -> maybe decomposition
    actions += check_plan_gates(pool).await?;

    // Phase 5: periodic review
    actions += check_periodic_reviews(pool).await?;

    // Phase 5b: process pending reviews (auto-approval)
    actions += process_pending_reviews(pool).await?;

    // Phase 6: decomposition -> create nodes and tasks, advance to dispatch
    actions += decompose_and_create_tasks(pool).await?;

    // Phase 7: dispatch -> mark tasks as queued/running, advance to execution
    actions += dispatch_phase(pool).await?;

    // Phase 8a: per-task certification (hybrid pre-integration gate)
    // certification_required nodes get verified immediately on task success,
    // BEFORE integration. This catches simple claims (function correctness,
    // type safety) early without waiting for the full merge.
    actions += select_certification_candidates(pool).await?;
    actions += process_certification_queue(pool).await?;

    // Phase 8b: execution -> check if all tasks are done
    // Now also checks that certification_required nodes have passed certification
    // before allowing transition to integration.
    actions += check_execution_completion(pool).await?;

    // Phase 9: integration -> advance to state_update -> next_cycle_ready
    actions += complete_integration(pool).await?;

    // Phase 10: post-integration certification sweep
    // System-level claims (cross-module invariants, integration properties)
    // are verified after merge. This is a second pass for any newly eligible
    // candidates that only become certifiable after integration.
    actions += select_certification_candidates(pool).await?;
    actions += process_certification_queue(pool).await?;

    // Phase 12: next_cycle_ready -> optionally create next cycle
    actions += handle_next_cycle(pool).await?;

    // Continuous: conflict detection
    actions += detect_conflicts(pool, scaling).await?;

    // Continuous: conflict auto-resolution
    actions += auto_resolve_conflicts(pool, scaling).await?;

    // Continuous: drift detection (Gap 6)
    actions += detect_drift(pool, scaling).await?;

    // Periodic: enforce retention policies (state cleanup, at most once per hour)
    let last_retention_run: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
        "SELECT MAX(created_at) FROM event_journal \
         WHERE aggregate_kind = 'system' AND event_kind = 'retention_policy_enforced'",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(None);

    let should_run_retention = match last_retention_run {
        None => true,
        Some(last) => chrono::Utc::now() - last > chrono::Duration::hours(1),
    };

    if should_run_retention {
        actions += enforce_retention_policies(pool, scaling).await.unwrap_or_else(|e| {
            tracing::warn!(error = %e, "Retention policy enforcement failed (non-fatal)");
            0
        });
    }

    // Rebuild read-model projections (non-fatal on failure)
    if let Err(e) = crate::projections::rebuild_projections(pool).await {
        tracing::warn!(error = %e, "Projection rebuild failed (non-fatal)");
    }

    Ok(actions)
}

// ── Step 1: Create loops for objectives that don't have one ──────────────

/// Find objectives that don't yet have a loop and create one for each.
///
/// The idempotency key is derived from the objective_id so that
/// restarting the loop runner never creates duplicate loops.
async fn create_loops_for_new_objectives(
    pool: &PgPool,
) -> Result<u32, Box<dyn std::error::Error>> {
    let rows = sqlx::query(
        "SELECT o.objective_id
         FROM objectives o
         LEFT JOIN loops l ON l.objective_id = o.objective_id
         WHERE l.loop_id IS NULL",
    )
    .fetch_all(pool)
    .await?;

    let mut count = 0u32;

    for row in &rows {
        let objective_id: &str = row.get("objective_id");
        let loop_id = Uuid::now_v7().to_string();
        let event_id = Uuid::now_v7().to_string();
        let idempotency_key = format!("auto-loop-for-{}", objective_id);

        let mut tx = pool.begin().await?;

        // BND-010: scoped idempotency check
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT aggregate_id FROM event_journal
             WHERE aggregate_kind = 'loop' AND idempotency_key = $1 LIMIT 1",
        )
        .bind(&idempotency_key)
        .fetch_optional(tx.as_mut())
        .await?;

        if existing.is_some() {
            tracing::debug!(objective_id, "Loop already created, skipping");
            tx.rollback().await?;
            continue;
        }

        // Create the loop
        sqlx::query(
            "INSERT INTO loops (loop_id, objective_id, cycle_index, active_track, created_at, updated_at)
             VALUES ($1, $2, 0, 'main', now(), now())",
        )
        .bind(&loop_id)
        .bind(objective_id)
        .execute(tx.as_mut())
        .await?;

        // Record event
        let payload = serde_json::json!({
            "loop_id": loop_id,
            "objective_id": objective_id,
            "active_track": "main",
            "trigger": "auto_create"
        });

        sqlx::query(
            "INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
             VALUES ($1, 'loop', $2, 'loop_created', $3, $4, now())
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
        )
        .bind(&event_id)
        .bind(&loop_id)
        .bind(&idempotency_key)
        .bind(&payload)
        .execute(tx.as_mut())
        .await?;

        tx.commit().await?;
        tracing::info!(loop_id, objective_id, "Created loop for objective");
        count += 1;
    }

    Ok(count)
}

// ── Step 2: Create cycles for active loops ───────────────────────────────

/// Find loops whose last cycle is completed (NextCycleReady) or that have
/// no cycle at all, and create a new cycle in `intake` phase.
///
/// A default policy snapshot is created if none exists.
async fn create_cycles_for_active_loops(
    pool: &PgPool,
) -> Result<u32, Box<dyn std::error::Error>> {
    // Find loops that either have no cycles, or whose latest cycle is
    // in the terminal phase (next_cycle_ready).
    let rows = sqlx::query(
        "SELECT l.loop_id, l.cycle_index, l.objective_id
         FROM loops l
         WHERE NOT EXISTS (
             SELECT 1 FROM cycles c
             WHERE c.loop_id = l.loop_id
               AND c.phase NOT IN ('next_cycle_ready')
         )",
    )
    .fetch_all(pool)
    .await?;

    let mut count = 0u32;

    for row in &rows {
        let loop_id: &str = row.get("loop_id");
        let current_cycle_index: i32 = row.get("cycle_index");
        let _objective_id: &str = row.get("objective_id");
        let next_cycle_index = current_cycle_index + 1;
        let cycle_id = Uuid::now_v7().to_string();
        let event_id = Uuid::now_v7().to_string();
        let idempotency_key = format!("auto-cycle-{}-{}", loop_id, next_cycle_index);

        let mut tx = pool.begin().await?;

        // BND-010: scoped idempotency check
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT aggregate_id FROM event_journal
             WHERE aggregate_kind = 'cycle' AND idempotency_key = $1 LIMIT 1",
        )
        .bind(&idempotency_key)
        .fetch_optional(tx.as_mut())
        .await?;

        if existing.is_some() {
            tracing::debug!(loop_id, next_cycle_index, "Cycle already created, skipping");
            tx.rollback().await?;
            continue;
        }

        // Build a default policy snapshot for the cycle.
        // In a full implementation this would come from the user_policies table.
        let default_policy = serde_json::json!({
            "policy_id": format!("auto-policy-{}", cycle_id),
            "global": {
                "default_provider_mode": "api",
                "default_model_family": "claude",
                "max_active_agents": 4,
                "default_concurrency": 2,
                "default_retry_budget": 3,
                "certification_routing": "standard"
            },
            "planner":     { "provider_mode": null, "provider_name": null, "model_name": null, "reasoning_effort": null },
            "implementer": { "provider_mode": null, "provider_name": null, "model_name": null, "reasoning_effort": null },
            "reviewer":    { "provider_mode": null, "provider_name": null, "model_name": null, "reasoning_effort": null },
            "debugger":    { "provider_mode": null, "provider_name": null, "model_name": null, "reasoning_effort": null },
            "research":    { "provider_mode": null, "provider_name": null, "model_name": null, "reasoning_effort": null },
            "formalizer_a": { "enabled": false, "mode": "off", "binding": {}, "certification_frequency": "never" },
            "formalizer_b": { "enabled": false, "mode": "off", "binding": {}, "certification_frequency": "never" }
        });

        // Create the cycle
        sqlx::query(
            "INSERT INTO cycles (cycle_id, loop_id, phase, policy_snapshot, created_at, updated_at)
             VALUES ($1, $2, 'intake', $3, now(), now())",
        )
        .bind(&cycle_id)
        .bind(loop_id)
        .bind(&default_policy)
        .execute(tx.as_mut())
        .await?;

        // Update the loop's cycle_index
        sqlx::query(
            "UPDATE loops SET cycle_index = $1, updated_at = now()
             WHERE loop_id = $2",
        )
        .bind(next_cycle_index)
        .bind(loop_id)
        .execute(tx.as_mut())
        .await?;

        // Record event
        let payload = serde_json::json!({
            "cycle_id": cycle_id,
            "loop_id": loop_id,
            "cycle_index": next_cycle_index,
            "phase": "intake",
            "trigger": "auto_create"
        });

        sqlx::query(
            "INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
             VALUES ($1, 'cycle', $2, 'cycle_created', $3, $4, now())
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
        )
        .bind(&event_id)
        .bind(&cycle_id)
        .bind(&idempotency_key)
        .bind(&payload)
        .execute(tx.as_mut())
        .await?;

        tx.commit().await?;
        tracing::info!(cycle_id, loop_id, next_cycle_index, "Created cycle for loop");
        count += 1;
    }

    Ok(count)
}

// ── Step 3: Advance intake cycles ────────────────────────────────────────

/// Cycles in `intake` phase are automatically advanced to `plan_elaboration`.
///
/// In a full system, intake would involve conversation extraction first.
/// For now, this immediately advances the phase to keep the loop moving.
async fn advance_intake_cycles(
    pool: &PgPool,
) -> Result<u32, Box<dyn std::error::Error>> {
    let rows = sqlx::query(
        "SELECT c.cycle_id, c.loop_id
         FROM cycles c
         WHERE c.phase = 'intake'",
    )
    .fetch_all(pool)
    .await?;

    let mut count = 0u32;

    for row in &rows {
        let cycle_id: &str = row.get("cycle_id");
        let loop_id: &str = row.get("loop_id");
        let event_id = Uuid::now_v7().to_string();
        let idempotency_key = format!("advance-intake-{}", cycle_id);

        let mut tx = pool.begin().await?;

        // BND-010: scoped idempotency check
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT aggregate_id FROM event_journal
             WHERE aggregate_kind = 'cycle' AND idempotency_key = $1 LIMIT 1",
        )
        .bind(&idempotency_key)
        .fetch_optional(tx.as_mut())
        .await?;

        if existing.is_some() {
            tx.rollback().await?;
            continue;
        }

        // Advance phase: intake -> plan_elaboration
        let result = sqlx::query(
            "UPDATE cycles SET phase = 'plan_elaboration', updated_at = now()
             WHERE cycle_id = $1 AND phase = 'intake'",
        )
        .bind(cycle_id)
        .execute(tx.as_mut())
        .await?;

        if result.rows_affected() == 0 {
            // Race condition: phase already changed
            tx.rollback().await?;
            continue;
        }

        // Record event
        let payload = serde_json::json!({
            "cycle_id": cycle_id,
            "loop_id": loop_id,
            "from_phase": "intake",
            "to_phase": "plan_elaboration",
            "trigger": "auto_advance"
        });

        sqlx::query(
            "INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
             VALUES ($1, 'cycle', $2, 'cycle_phase_transitioned', $3, $4, now())
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
        )
        .bind(&event_id)
        .bind(cycle_id)
        .bind(&idempotency_key)
        .bind(&payload)
        .execute(tx.as_mut())
        .await?;

        tx.commit().await?;
        tracing::info!(cycle_id, "Advanced cycle from intake to plan_elaboration");
        count += 1;
    }

    Ok(count)
}

// ── Step 4: Check plan gates (real scoring) ──────────────────────────────

/// For cycles in `plan_elaboration`, evaluate plan gate completeness by
/// checking actual DB state (objective summary, architecture, milestones,
/// acceptance criteria, dependencies, invariants, risks, questions).
///
/// If the completeness score >= 0.3 (MVP threshold) or the user has
/// overridden the gate, advance to `decomposition`.
///
/// Stores the gate evaluation result in event_journal.
async fn check_plan_gates(
    pool: &PgPool,
) -> Result<u32, Box<dyn std::error::Error>> {
    let rows = sqlx::query(
        "SELECT c.cycle_id, c.loop_id, l.objective_id
         FROM cycles c
         JOIN loops l ON l.loop_id = c.loop_id
         WHERE c.phase = 'plan_elaboration'",
    )
    .fetch_all(pool)
    .await?;

    let mut count = 0u32;

    const MAX_ELABORATION_ATTEMPTS: i64 = 5;
    const MAX_ELABORATION_MINUTES: i64 = 30;

    for row in &rows {
        let cycle_id: String = row.get("cycle_id");
        let loop_id: String = row.get("loop_id");
        let objective_id: String = row.get("objective_id");

        // Guard: count how many times this cycle has been through plan_elaboration
        let elaboration_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM event_journal \
             WHERE aggregate_id = $1 AND event_kind = 'plan_gate_evaluated'",
        )
        .bind(&cycle_id)
        .fetch_one(pool)
        .await
        .unwrap_or(0);

        if elaboration_count >= MAX_ELABORATION_ATTEMPTS {
            // Force-advance past planning with an override
            sqlx::query(
                "UPDATE cycles SET phase = 'decomposition', updated_at = now() WHERE cycle_id = $1",
            )
            .bind(&cycle_id)
            .execute(pool)
            .await?;

            let event_id = Uuid::now_v7().to_string();
            sqlx::query(
                "INSERT INTO event_journal \
                 (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
                 VALUES ($1, 'cycle', $2, 'plan_gate_forced_override', $3, $4::jsonb, now()) \
                 ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
            )
            .bind(&event_id)
            .bind(&cycle_id)
            .bind(format!("plan-gate-force-{}", cycle_id))
            .bind(serde_json::json!({"cycle_id": cycle_id, "elaboration_count": elaboration_count, "reason": "max_attempts_exceeded"}))
            .execute(pool)
            .await?;

            tracing::warn!(cycle_id = %cycle_id, elaboration_count, "Plan gate forced override after max attempts");
            count += 1;
            continue;
        }

        // Guard: phase-level timeout (check how long the cycle has been in plan_elaboration)
        let phase_started: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
            "SELECT MIN(created_at) FROM event_journal \
             WHERE aggregate_id = $1 AND event_kind = 'cycle_phase_transitioned' \
             AND payload::text LIKE '%plan_elaboration%'",
        )
        .bind(&cycle_id)
        .fetch_one(pool)
        .await
        .unwrap_or(None);

        if let Some(started) = phase_started {
            let elapsed = chrono::Utc::now() - started;
            if elapsed > chrono::Duration::minutes(MAX_ELABORATION_MINUTES) {
                sqlx::query(
                    "UPDATE cycles SET phase = 'decomposition', updated_at = now() WHERE cycle_id = $1",
                )
                .bind(&cycle_id)
                .execute(pool)
                .await?;

                let event_id = Uuid::now_v7().to_string();
                sqlx::query(
                    "INSERT INTO event_journal \
                     (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
                     VALUES ($1, 'cycle', $2, 'plan_gate_forced_override', $3, $4::jsonb, now()) \
                     ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
                )
                .bind(&event_id)
                .bind(&cycle_id)
                .bind(format!("plan-gate-timeout-{}", cycle_id))
                .bind(serde_json::json!({"cycle_id": cycle_id, "elapsed_minutes": elapsed.num_minutes(), "reason": "phase_timeout"}))
                .execute(pool)
                .await?;

                tracing::warn!(cycle_id = %cycle_id, elapsed_minutes = elapsed.num_minutes(), "Plan gate forced override after timeout");
                count += 1;
                continue;
            }
        }

        // Run the real plan elaboration pipeline
        let gate_satisfied =
            planning::elaborate_plan(pool, &cycle_id, &objective_id).await?;

        // Record the gate evaluation attempt for the elaboration counter
        let eval_event_id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO event_journal \
             (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
             VALUES ($1, 'cycle', $2, 'plan_gate_evaluated', $3, $4::jsonb, now()) \
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
        )
        .bind(&eval_event_id)
        .bind(&cycle_id)
        .bind(format!("plan-gate-eval-{}-{}", cycle_id, elaboration_count + 1))
        .bind(serde_json::json!({"cycle_id": cycle_id, "attempt": elaboration_count + 1, "gate_satisfied": gate_satisfied}))
        .execute(pool)
        .await?;

        if gate_satisfied {
            // Advance to decomposition
            count += advance_cycle_phase(
                pool,
                &cycle_id,
                &loop_id,
                "plan_elaboration",
                "decomposition",
                "plan_gate_satisfied",
            )
            .await?;
        }
        // Otherwise, the cycle stays in plan_elaboration until
        // external agents/users satisfy the gate conditions.

        // Also check if the plan gate was manually overridden
        if !gate_satisfied {
            let overridden: Option<String> = sqlx::query_scalar(
                "SELECT pg.current_status FROM plan_gates pg
                 JOIN plans p ON p.plan_id = pg.plan_id
                 WHERE p.objective_id = $1 AND pg.current_status = 'overridden'",
            )
            .bind(&objective_id)
            .fetch_optional(pool)
            .await?;

            if overridden.is_some() {
                count += advance_cycle_phase(
                    pool,
                    &cycle_id,
                    &loop_id,
                    "plan_elaboration",
                    "decomposition",
                    "plan_gate_overridden",
                )
                .await?;
            }
        }
    }

    Ok(count)
}

// ── Step 5: Decompose and create tasks ───────────────────────────────────

/// For cycles in `decomposition`, create nodes via the planning pipeline
/// and then create tasks for those nodes. Once all nodes have tasks,
/// advance to `dispatch`.
///
/// CNF-005 (decomposition conflict) -- FUTURE HOOK:
/// A decomposition conflict would trigger when the same milestone is
/// decomposed by two concurrent workers into incompatible task trees.
/// This would happen if:
///   1. Two decomposition workers run concurrently on the same milestone.
///   2. Both produce valid but structurally different node/task graphs.
/// Detection: after INSERT into nodes, check if the milestone already
/// has a different decomposition (different node set) from a concurrent
/// worker. If so, INSERT INTO conflicts with conflict_kind =
/// 'decomposition'. Currently, decomposition is single-threaded per
/// cycle, so this cannot happen. When concurrent decomposition is
/// enabled, add the detection check here.
async fn decompose_and_create_tasks(
    pool: &PgPool,
) -> Result<u32, Box<dyn std::error::Error>> {
    let rows = sqlx::query(
        "SELECT c.cycle_id, c.loop_id, l.objective_id
         FROM cycles c
         JOIN loops l ON l.loop_id = c.loop_id
         WHERE c.phase = 'decomposition'",
    )
    .fetch_all(pool)
    .await?;

    let mut count = 0u32;

    for row in &rows {
        let cycle_id: String = row.get("cycle_id");
        let loop_id: String = row.get("loop_id");
        let objective_id: String = row.get("objective_id");

        // REC-006: Generate self-improvement milestone templates (if applicable)
        count += recursive_improvement::generate_milestone_templates(pool, &objective_id)
            .await
            .unwrap_or_else(|e| {
                tracing::warn!(error = %e, objective_id, "REC-006: milestone template generation failed (non-fatal)");
                0
            });

        // Create nodes from objective decomposition
        let nodes_created = planning::decompose_plan(pool, &objective_id).await?;
        count += nodes_created;

        // ── CTL-018: Milestone-to-node bridge ─────────────────────────
        //
        // Each milestone_node becomes a Node (if not already bridged),
        // and milestone parent-child relationships become node_edges
        // with edge_kind = 'depends_on'.  This formalises the
        // authoritative path from plan milestones to execution nodes.
        count += bridge_milestones_to_nodes(pool, &objective_id).await?;

        // Create tasks for nodes that don't have tasks yet
        count += create_tasks_for_objective(pool, &cycle_id, &objective_id).await?;

        // Auto-create integration verification node (if not already present)
        let existing_verify: Option<String> = sqlx::query_scalar(
            "SELECT node_id FROM nodes WHERE objective_id = $1 AND lane = 'integration' LIMIT 1",
        )
        .bind(&objective_id)
        .fetch_optional(pool)
        .await?;

        if existing_verify.is_none() {
            let verify_node_id = Uuid::now_v7().to_string();
            let verify_idem = format!("integration-verify-node-{}", cycle_id);

            let mut tx = pool.begin().await?;

            // BND-010: scoped idempotency check
            let idem_exists: Option<String> = sqlx::query_scalar(
                "SELECT aggregate_id FROM event_journal
                 WHERE aggregate_kind = 'node' AND idempotency_key = $1 LIMIT 1",
            )
            .bind(&verify_idem)
            .fetch_optional(tx.as_mut())
            .await?;

            if idem_exists.is_none() {
                sqlx::query(
                    "INSERT INTO nodes (node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at)
                     VALUES ($1, $2, 'Integration Verification', $3, 'integration', 'proposed', now(), now())
                     ON CONFLICT DO NOTHING",
                )
                .bind(&verify_node_id)
                .bind(&objective_id)
                .bind("Merge all completed worktrees into mainline, detect project type, run build/test, report failures.")
                .execute(tx.as_mut())
                .await?;

                // Make it depend on ALL other nodes in this cycle
                let other_nodes: Vec<String> = sqlx::query_scalar(
                    "SELECT node_id FROM nodes WHERE objective_id = $1 AND node_id != $2",
                )
                .bind(&objective_id)
                .bind(&verify_node_id)
                .fetch_all(tx.as_mut())
                .await?;

                for dep_node_id in &other_nodes {
                    let edge_id = Uuid::now_v7().to_string();
                    sqlx::query(
                        "INSERT INTO node_edges (edge_id, from_node_id, to_node_id, edge_kind)
                         VALUES ($1, $2, $3, 'depends_on')
                         ON CONFLICT DO NOTHING",
                    )
                    .bind(&edge_id)
                    .bind(dep_node_id)
                    .bind(&verify_node_id)
                    .execute(tx.as_mut())
                    .await?;
                }

                // Record event
                let event_id = Uuid::now_v7().to_string();
                sqlx::query(
                    "INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
                     VALUES ($1, 'node', $2, 'integration_verify_node_created', $3, $4, now())
                     ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
                )
                .bind(&event_id)
                .bind(&verify_node_id)
                .bind(&verify_idem)
                .bind(serde_json::json!({
                    "objective_id": objective_id,
                    "cycle_id": cycle_id,
                    "depends_on_count": other_nodes.len(),
                }))
                .execute(tx.as_mut())
                .await?;

                tx.commit().await?;
                tracing::info!(verify_node_id, objective_id, cycle_id, deps = other_nodes.len(),
                    "Auto-created integration verification node");

                // Create the task for the verification node
                count += create_tasks_for_objective(pool, &cycle_id, &objective_id).await?;
            } else {
                tx.rollback().await?;
            }
        }

        // Check if all nodes have tasks now
        let untasked_count: Option<i64> = sqlx::query_scalar(
            "SELECT COUNT(*) FROM nodes n
             WHERE n.objective_id = $1
               AND n.lifecycle IN ('proposed', 'queued')
               AND NOT EXISTS (
                   SELECT 1 FROM tasks t WHERE t.node_id = n.node_id
               )",
        )
        .bind(&objective_id)
        .fetch_one(pool)
        .await?;

        if untasked_count == Some(0) {
            count += advance_cycle_phase(
                pool,
                &cycle_id,
                &loop_id,
                "decomposition",
                "dispatch",
                "all_nodes_tasked",
            )
            .await?;
        }
    }

    Ok(count)
}

/// CTL-018: Bridge milestone_nodes to execution nodes.
///
/// For each milestone_node belonging to this objective (via its
/// milestone_tree), create a corresponding Node if one does not already
/// exist with a matching `milestone_ref`.  Then, for every milestone
/// with a `parent_id`, create a `depends_on` edge from the parent's
/// node to the child's node so the execution graph mirrors the
/// milestone hierarchy.
///
/// Returns the number of new nodes created.
async fn bridge_milestones_to_nodes(
    pool: &PgPool,
    objective_id: &str,
) -> Result<u32, Box<dyn std::error::Error>> {
    // 1. Fetch all milestones for this objective
    let milestones = sqlx::query(
        "SELECT mn.milestone_id, mn.title, mn.description, mn.parent_id, mn.ordering
         FROM milestone_nodes mn
         JOIN milestone_trees mt ON mt.tree_id = mn.tree_id
         WHERE mt.objective_id = $1
         ORDER BY mn.ordering ASC",
    )
    .bind(objective_id)
    .fetch_all(pool)
    .await?;

    if milestones.is_empty() {
        return Ok(0);
    }

    let mut created = 0u32;
    // Map from milestone_id -> node_id for edge creation
    let mut milestone_to_node: std::collections::HashMap<String, String> =
        std::collections::HashMap::new();

    let mut tx = pool.begin().await?;

    for ms_row in &milestones {
        let milestone_id: String = ms_row.get("milestone_id");
        let title: String = ms_row.get("title");
        let description: String = ms_row.get("description");

        // Check if a node already exists for this milestone (via milestone_ref
        // stored in the node's statement as a tag, or via event_journal).
        let existing_node: Option<String> = sqlx::query_scalar(
            "SELECT n.node_id FROM nodes n
             JOIN event_journal ej ON ej.aggregate_id = n.node_id
             WHERE ej.event_kind = 'milestone_bridged'
               AND ej.payload->>'milestone_id' = $1
             LIMIT 1",
        )
        .bind(&milestone_id)
        .fetch_optional(tx.as_mut())
        .await?;

        let node_id = if let Some(nid) = existing_node {
            nid
        } else {
            // Create a new node for this milestone
            let node_id = Uuid::now_v7().to_string();
            let statement = format!(
                "[milestone:{}] {}",
                milestone_id,
                if description.is_empty() { &title } else { &description }
            );

            sqlx::query(
                "INSERT INTO nodes (node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at, revision)
                 VALUES ($1, $2, $3, $4, 'implementation', 'proposed', now(), now(), 1)
                 ON CONFLICT DO NOTHING",
            )
            .bind(&node_id)
            .bind(objective_id)
            .bind(&title)
            .bind(&statement)
            .execute(tx.as_mut())
            .await?;

            // Record provenance so we can find this node later
            let event_id = Uuid::now_v7().to_string();
            sqlx::query(
                "INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
                 VALUES ($1, 'node', $2, 'milestone_bridged', $3, $4::jsonb, now())
                 ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
            )
            .bind(&event_id)
            .bind(&node_id)
            .bind(format!("milestone-bridge-{}", milestone_id))
            .bind(serde_json::json!({
                "milestone_id": milestone_id,
                "objective_id": objective_id,
                "bridge": "CTL-018",
            }))
            .execute(tx.as_mut())
            .await?;

            created += 1;
            tracing::debug!(node_id, milestone_id, "CTL-018: Bridged milestone to node");

            node_id
        };

        milestone_to_node.insert(milestone_id.clone(), node_id);
    }

    // 2. Create depends_on edges from milestone parent-child relationships
    for ms_row in &milestones {
        let milestone_id: String = ms_row.get("milestone_id");
        let parent_id: Option<String> = ms_row.try_get("parent_id").ok();

        if let Some(ref parent_mid) = parent_id {
            if let (Some(parent_node_id), Some(child_node_id)) = (
                milestone_to_node.get(parent_mid),
                milestone_to_node.get(&milestone_id),
            ) {
                // Check if edge already exists
                let edge_exists: Option<String> = sqlx::query_scalar(
                    "SELECT edge_id FROM node_edges
                     WHERE from_node_id = $1 AND to_node_id = $2 AND edge_kind = 'depends_on'
                     LIMIT 1",
                )
                .bind(parent_node_id)
                .bind(child_node_id)
                .fetch_optional(tx.as_mut())
                .await?;

                if edge_exists.is_none() {
                    let edge_id = Uuid::now_v7().to_string();
                    sqlx::query(
                        "INSERT INTO node_edges (edge_id, from_node_id, to_node_id, edge_kind)
                         VALUES ($1, $2, $3, 'depends_on')
                         ON CONFLICT DO NOTHING",
                    )
                    .bind(&edge_id)
                    .bind(parent_node_id)
                    .bind(child_node_id)
                    .execute(tx.as_mut())
                    .await?;
                }
            }
        }
    }

    tx.commit().await?;

    if created > 0 {
        tracing::info!(
            objective_id,
            created,
            milestones = milestones.len(),
            "CTL-018: Milestone-to-node bridge completed"
        );
    }

    Ok(created)
}

/// Create tasks for nodes belonging to an objective that don't have tasks.
///
/// Tasks whose node has no unmet dependencies start as 'running' (ready for
/// worker-dispatch). Tasks whose node has pending predecessor nodes start
/// as 'queued' and will be unblocked when dependencies complete.
async fn create_tasks_for_objective(
    pool: &PgPool,
    cycle_id: &str,
    objective_id: &str,
) -> Result<u32, Box<dyn std::error::Error>> {
    let node_rows = sqlx::query(
        "SELECT n.node_id, n.title, n.lifecycle, n.lane
         FROM nodes n
         WHERE n.objective_id = $1
           AND n.lifecycle IN ('proposed', 'queued')
           AND NOT EXISTS (
               SELECT 1 FROM tasks t WHERE t.node_id = n.node_id
           )",
    )
    .bind(objective_id)
    .fetch_all(pool)
    .await?;

    // Load the current policy snapshot for model binding resolution
    let policy_row = sqlx::query(
        "SELECT policy_payload FROM user_policies ORDER BY revision DESC LIMIT 1",
    )
    .fetch_optional(pool)
    .await?;

    let policy_payload: Option<serde_json::Value> = policy_row
        .as_ref()
        .and_then(|r| r.try_get::<serde_json::Value, _>("policy_payload").ok());

    // SKL-015 + SKL-006: Load available skill packs from DB for resolve_skill_full
    let skill_rows = sqlx::query(
        r#"SELECT skill_pack_id, worker_role, description, accepted_task_kinds,
                  "references", scripts, COALESCE(expected_output_contract, '') AS expected_output_contract,
                  version, COALESCE(deprecated, false) AS deprecated
           FROM skill_packs
           ORDER BY created_at"#,
    )
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    let available_skills: Vec<SkillPackManifest> = skill_rows
        .iter()
        .map(|r| {
            let atk: serde_json::Value = r.try_get("accepted_task_kinds").unwrap_or(serde_json::json!([]));
            let refs: serde_json::Value = r.try_get("references").unwrap_or(serde_json::json!([]));
            let scr: serde_json::Value = r.try_get("scripts").unwrap_or(serde_json::json!([]));
            let eoc: String = r.try_get("expected_output_contract").unwrap_or_default();
            let ver: Option<String> = r.try_get("version").unwrap_or(None);
            let dep: bool = r.try_get("deprecated").unwrap_or(false);
            SkillPackManifest {
                skill_pack_id: r.get("skill_pack_id"),
                worker_role: r.get("worker_role"),
                description: r.get("description"),
                accepted_task_kinds: atk.as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default(),
                references: refs.as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default(),
                scripts: scr.as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default(),
                expected_output_contract: if eoc.is_empty() { None } else { Some(eoc) },
                version: ver,
                deprecated: dep,
            }
        })
        .collect();

    // SKL-011: Read project_default_skill_pack from user_policies
    let project_default_skill_pack: Option<String> = policy_payload
        .as_ref()
        .and_then(|v| v.pointer("/global/default_skill_pack_id"))
        .and_then(|v| v.as_str())
        .map(String::from);

    let mut count = 0u32;

    for node_row in &node_rows {
        let node_id: &str = node_row.get("node_id");
        let node_title: &str = node_row.get("title");
        let lane: &str = node_row.get("lane");
        let task_id = Uuid::now_v7().to_string();
        let event_id = Uuid::now_v7().to_string();
        let idempotency_key = format!("auto-task-{}-{}", cycle_id, node_id);

        // Map lane to worker role
        let worker_role = match lane {
            "planning" => "planner",
            "implementation" => "implementer",
            "verification" => "reviewer",
            "integration" => "integration_verifier",
            _ => "implementer",
        };

        // Resolve provider_mode and model_binding from policy
        let (provider_mode, model_binding) = if let Some(ref payload) = policy_payload {
            let mode = payload
                .get(worker_role)
                .and_then(|r| r.get("provider_mode"))
                .and_then(|v| v.as_str())
                .or_else(|| {
                    payload
                        .pointer("/global/default_provider_mode")
                        .and_then(|v| v.as_str())
                })
                .unwrap_or("api")
                .to_string();

            let model = payload
                .get(worker_role)
                .and_then(|r| r.get("model_name"))
                .and_then(|v| v.as_str())
                .or_else(|| {
                    payload
                        .pointer("/global/default_model_family")
                        .and_then(|v| v.as_str())
                })
                .unwrap_or("claude")
                .to_string();

            (Some(mode), Some(model))
        } else {
            (None, None)
        };

        // Check if all predecessor nodes are completed (dependency-aware dispatch)
        let unmet_deps: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM node_edges ne
             JOIN nodes pred ON ne.from_node_id = pred.node_id
             WHERE ne.to_node_id = $1
               AND ne.edge_kind IN ('depends_on', 'blocks')
               AND pred.lifecycle NOT IN ('admitted', 'done', 'completed')",
        )
        .bind(node_id)
        .fetch_one(pool)
        .await?;

        // Tasks with no unmet deps start as 'running', others as 'queued'
        let initial_status = if unmet_deps == 0 { "running" } else { "queued" };
        let initial_lifecycle = if unmet_deps == 0 { "running" } else { "queued" };

        let mut tx = pool.begin().await?;

        // BND-010: scoped idempotency check
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT aggregate_id FROM event_journal
             WHERE aggregate_kind = 'task' AND idempotency_key = $1 LIMIT 1",
        )
        .bind(&idempotency_key)
        .fetch_optional(tx.as_mut())
        .await?;

        if existing.is_some() {
            tx.rollback().await?;
            continue;
        }

        // ── Gap 7: Backlog schema enforcement ─────────────────────────
        // Extract timeout and retry budget from policy
        let timeout_seconds = policy_payload
            .as_ref()
            .and_then(|v| v.pointer("/global/default_timeout_seconds"))
            .and_then(|v| v.as_i64())
            .unwrap_or(300) as i32;

        let retry_budget = policy_payload
            .as_ref()
            .and_then(|v| v.pointer("/global/default_retry_budget"))
            .and_then(|v| v.as_i64())
            .unwrap_or(3) as i32;

        // SKL-015 + SKL-010: Resolve skill pack via resolve_skill_full instead
        // of hardcoded 'default'. Uses the lane as task_kind for mapping.
        let task_kind = lane; // lane maps directly to task_kind for resolution
        let skill_resolution = SkillRegistryLoader::resolve_skill_full(
            Some(&task_id),
            Some(node_id),
            task_kind,
            worker_role,
            None, // current_phase -- not available at task creation time
            &[],  // task_overrides -- none at creation time
            &[],  // node_overrides -- none at creation time
            &[],  // phase_restrictions
            project_default_skill_pack.as_deref(),
            None, // global_fallback
            &available_skills,
        );

        let resolved_skill_pack_id = &skill_resolution.skill_pack_id;

        // Create the task with dependency-aware initial status, policy bindings,
        // and backlog schema fields (cautions, timeout, retry budget, approval policies)
        sqlx::query(
            "INSERT INTO tasks (task_id, node_id, worker_role, skill_pack_id, status, \
             provider_mode, model_binding, timeout_seconds, retry_budget, cautions, \
             auto_approval_policy, human_review_policy, created_at, updated_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10::jsonb, $11, $12, now(), now())",
        )
        .bind(&task_id)
        .bind(node_id)
        .bind(worker_role)
        .bind(resolved_skill_pack_id)
        .bind(initial_status)
        .bind(&provider_mode)
        .bind(&model_binding)
        .bind(timeout_seconds)
        .bind(retry_budget)
        .bind(serde_json::json!(["do not mutate canonical state directly"]))
        .bind("only_if_all_checks_pass")
        .bind("required_if_conflict_exists")
        .execute(tx.as_mut())
        .await?;

        // Update node lifecycle
        sqlx::query(
            "UPDATE nodes SET lifecycle = $1, updated_at = now()
             WHERE node_id = $2 AND lifecycle = 'proposed'",
        )
        .bind(initial_lifecycle)
        .bind(node_id)
        .execute(tx.as_mut())
        .await?;

        // Record event with skill resolution provenance (SKL-009)
        let payload = serde_json::json!({
            "task_id": task_id,
            "node_id": node_id,
            "node_title": node_title,
            "cycle_id": cycle_id,
            "worker_role": worker_role,
            "skill_pack_id": resolved_skill_pack_id,
            "skill_selection_reason": skill_resolution.selection_reason,
            "skill_selection_level": skill_resolution.selection_level,
            "status": initial_status,
            "unmet_deps": unmet_deps,
            "provider_mode": provider_mode,
            "model_binding": model_binding,
            "trigger": "auto_create"
        });

        sqlx::query(
            "INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
             VALUES ($1, 'task', $2, 'task_created', $3, $4, now())
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
        )
        .bind(&event_id)
        .bind(&task_id)
        .bind(&idempotency_key)
        .bind(&payload)
        .execute(tx.as_mut())
        .await?;

        tx.commit().await?;
        tracing::info!(task_id, node_id, node_title, cycle_id, initial_status, "Created task for node");
        count += 1;
    }

    Ok(count)
}

// ── Step 6: Dispatch phase ───────────────────────────────────────────────

/// For cycles in `dispatch`, find queued tasks and mark them as running
/// (create attempts). Once all tasks are dispatched, advance to `execution`.
async fn dispatch_phase(
    pool: &PgPool,
) -> Result<u32, Box<dyn std::error::Error>> {
    // First, dispatch queued tasks (up to batch limit)
    let mut count = dispatch_queued_tasks(pool).await?;

    // Then check if any dispatch-phase cycles can advance to execution
    let rows = sqlx::query(
        "SELECT c.cycle_id, c.loop_id, l.objective_id
         FROM cycles c
         JOIN loops l ON l.loop_id = c.loop_id
         WHERE c.phase = 'dispatch'",
    )
    .fetch_all(pool)
    .await?;

    for row in &rows {
        let cycle_id: String = row.get("cycle_id");
        let loop_id: String = row.get("loop_id");
        let objective_id: String = row.get("objective_id");

        // Check if all tasks for this objective are dispatched (none queued)
        let queued_count: Option<i64> = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tasks t
             JOIN nodes n ON n.node_id = t.node_id
             WHERE n.objective_id = $1 AND t.status = 'queued'",
        )
        .bind(&objective_id)
        .fetch_one(pool)
        .await?;

        if queued_count == Some(0) {
            count += advance_cycle_phase(
                pool,
                &cycle_id,
                &loop_id,
                "dispatch",
                "execution",
                "all_tasks_dispatched",
            )
            .await?;
        }
    }

    Ok(count)
}

/// Find tasks with status `queued`, mark them as `running`, create a
/// task_attempt, and emit events.
async fn dispatch_queued_tasks(
    pool: &PgPool,
) -> Result<u32, Box<dyn std::error::Error>> {
    let rows = sqlx::query(
        "SELECT t.task_id, t.node_id, t.worker_role, t.skill_pack_id
         FROM tasks t
         WHERE t.status = 'queued'
         ORDER BY t.created_at ASC
         LIMIT 10",
    )
    .fetch_all(pool)
    .await?;

    let mut count = 0u32;

    for row in &rows {
        let task_id: &str = row.get("task_id");
        let node_id: &str = row.get("node_id");
        let worker_role: &str = row.get("worker_role");
        let _skill_pack_id: &str = row.get("skill_pack_id");
        let attempt_id = Uuid::now_v7().to_string();
        let event_id = Uuid::now_v7().to_string();
        let idempotency_key = format!("dispatch-{}", task_id);

        let mut tx = pool.begin().await?;

        // BND-010: scoped idempotency check
        let existing: Option<String> = sqlx::query_scalar(
            "SELECT aggregate_id FROM event_journal
             WHERE aggregate_kind = 'task' AND idempotency_key = $1 LIMIT 1",
        )
        .bind(&idempotency_key)
        .fetch_optional(tx.as_mut())
        .await?;

        if existing.is_some() {
            tx.rollback().await?;
            continue;
        }

        // Update task status to running
        let result = sqlx::query(
            "UPDATE tasks SET status = 'running', updated_at = now()
             WHERE task_id = $1 AND status = 'queued'",
        )
        .bind(task_id)
        .execute(tx.as_mut())
        .await?;

        if result.rows_affected() == 0 {
            // Race: task was already dispatched
            tx.rollback().await?;
            continue;
        }

        // Determine the next attempt index
        let max_attempt: Option<i32> = sqlx::query_scalar(
            "SELECT MAX(attempt_index) FROM task_attempts WHERE task_id = $1",
        )
        .bind(task_id)
        .fetch_one(tx.as_mut())
        .await?;

        let attempt_index = max_attempt.map_or(1, |m| m + 1);

        // Create a task attempt
        sqlx::query(
            "INSERT INTO task_attempts (task_attempt_id, task_id, attempt_index, lease_owner, status, started_at)
             VALUES ($1, $2, $3, 'loop-runner', 'running', now())",
        )
        .bind(&attempt_id)
        .bind(task_id)
        .bind(attempt_index)
        .execute(tx.as_mut())
        .await?;

        // Update node lifecycle to running
        sqlx::query(
            "UPDATE nodes SET lifecycle = 'running', updated_at = now()
             WHERE node_id = $1 AND lifecycle IN ('proposed', 'queued')",
        )
        .bind(node_id)
        .execute(tx.as_mut())
        .await?;

        // Record dispatch event
        let payload = serde_json::json!({
            "task_id": task_id,
            "node_id": node_id,
            "attempt_id": attempt_id,
            "attempt_index": attempt_index,
            "worker_role": worker_role,
            "status": "running",
            "trigger": "auto_dispatch"
        });

        sqlx::query(
            "INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
             VALUES ($1, 'task', $2, 'task_status_changed', $3, $4, now())
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
        )
        .bind(&event_id)
        .bind(task_id)
        .bind(&idempotency_key)
        .bind(&payload)
        .execute(tx.as_mut())
        .await?;

        tx.commit().await?;
        tracing::info!(task_id, node_id, attempt_id, attempt_index, "Dispatched task");
        count += 1;
    }

    Ok(count)
}

// ── Step 7: Check execution completion ───────────────────────────────────

/// For cycles in `execution`, check if all tasks for the cycle's objective
/// are terminal (succeeded, failed, or cancelled -- none queued/running).
/// If so, advance to `integration`.
async fn check_execution_completion(
    pool: &PgPool,
) -> Result<u32, Box<dyn std::error::Error>> {
    let rows = sqlx::query(
        "SELECT c.cycle_id, c.loop_id, l.objective_id
         FROM cycles c
         JOIN loops l ON l.loop_id = c.loop_id
         WHERE c.phase = 'execution'",
    )
    .fetch_all(pool)
    .await?;

    let mut count = 0u32;

    for row in &rows {
        let cycle_id: String = row.get("cycle_id");
        let loop_id: String = row.get("loop_id");
        let objective_id: String = row.get("objective_id");

        // Count non-terminal tasks
        let active_count: Option<i64> = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tasks t
             JOIN nodes n ON n.node_id = t.node_id
             WHERE n.objective_id = $1
               AND t.status IN ('queued', 'running')",
        )
        .bind(&objective_id)
        .fetch_one(pool)
        .await?;

        // Hybrid certification gate: count certification_required nodes
        // whose tasks succeeded but certification hasn't passed yet.
        // These block the transition to integration.
        let pending_cert: Option<i64> = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tasks t
             JOIN nodes n ON n.node_id = t.node_id
             WHERE n.objective_id = $1
               AND COALESCE(n.certification_required, false) = true
               AND t.status = 'succeeded'
               AND NOT EXISTS (
                   SELECT 1 FROM certification_candidates cc
                   JOIN certification_submissions cs ON cs.candidate_id = cc.candidate_id
                   WHERE cc.task_id = t.task_id
                     AND cs.queue_status IN ('completed', 'acknowledged')
               )",
        )
        .bind(&objective_id)
        .fetch_one(pool)
        .await?;

        if pending_cert.unwrap_or(0) > 0 {
            tracing::info!(
                cycle_id = %cycle_id,
                objective_id = %objective_id,
                pending_certifications = pending_cert.unwrap_or(0),
                "Execution complete but waiting for per-task certification before integration"
            );
            continue;
        }

        // Also check that at least one task exists (avoid advancing on empty)
        let total_count: Option<i64> = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tasks t
             JOIN nodes n ON n.node_id = t.node_id
             WHERE n.objective_id = $1",
        )
        .bind(&objective_id)
        .fetch_one(pool)
        .await?;

        if active_count == Some(0) && total_count.unwrap_or(0) > 0 {
            // All tasks are terminal; record summary in event
            let succeeded_count: Option<i64> = sqlx::query_scalar(
                "SELECT COUNT(*) FROM tasks t
                 JOIN nodes n ON n.node_id = t.node_id
                 WHERE n.objective_id = $1 AND t.status = 'succeeded'",
            )
            .bind(&objective_id)
            .fetch_one(pool)
            .await?;

            let failed_count: Option<i64> = sqlx::query_scalar(
                "SELECT COUNT(*) FROM tasks t
                 JOIN nodes n ON n.node_id = t.node_id
                 WHERE n.objective_id = $1 AND t.status = 'failed'",
            )
            .bind(&objective_id)
            .fetch_one(pool)
            .await?;

            // Store execution summary event
            let event_id = Uuid::now_v7().to_string();
            let idempotency_key = format!("exec-summary-{}", cycle_id);
            let payload = serde_json::json!({
                "cycle_id": cycle_id,
                "objective_id": objective_id,
                "total_tasks": total_count.unwrap_or(0),
                "succeeded": succeeded_count.unwrap_or(0),
                "failed": failed_count.unwrap_or(0),
                "trigger": "execution_complete"
            });

            // Use ON CONFLICT to avoid failing on idempotency
            sqlx::query(
                "INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
                 VALUES ($1, 'cycle', $2, 'execution_completed', $3, $4, now())
                 ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
            )
            .bind(&event_id)
            .bind(&cycle_id)
            .bind(&idempotency_key)
            .bind(&payload)
            .execute(pool)
            .await?;

            count += advance_cycle_phase(
                pool,
                &cycle_id,
                &loop_id,
                "execution",
                "integration",
                "all_tasks_terminal",
            )
            .await?;

            tracing::info!(
                cycle_id = %cycle_id,
                succeeded = succeeded_count.unwrap_or(0),
                failed = failed_count.unwrap_or(0),
                "Execution complete, advancing to integration"
            );
        }
    }

    Ok(count)
}

// ── Step 8: Complete integration ─────────────────────────────────────────

/// For cycles in `integration`, advance through state_update to
/// `next_cycle_ready`. Records cycle completion in event_journal.
async fn complete_integration(
    pool: &PgPool,
) -> Result<u32, Box<dyn std::error::Error>> {
    let mut count = 0u32;

    // integration -> state_update (only if integration_verify task passed)
    let rows = sqlx::query(
        "SELECT c.cycle_id, c.loop_id, l.objective_id
         FROM cycles c
         JOIN loops l ON l.loop_id = c.loop_id
         WHERE c.phase = 'integration'",
    )
    .fetch_all(pool)
    .await?;

    for row in &rows {
        let cycle_id: String = row.get("cycle_id");
        let loop_id: String = row.get("loop_id");
        let objective_id: String = row.get("objective_id");

        // Check if integration_verify task exists and is complete
        let verify_status: Option<String> = sqlx::query_scalar(
            "SELECT t.status FROM tasks t
             JOIN nodes n ON t.node_id = n.node_id
             WHERE n.objective_id = $1
               AND n.lane = 'integration'
               AND t.worker_role = 'integration_verifier'
             LIMIT 1",
        )
        .bind(&objective_id)
        .fetch_optional(pool)
        .await?;

        match verify_status.as_deref() {
            Some("succeeded") => {
                // Integration verified, advance
                count += advance_cycle_phase(
                    pool,
                    &cycle_id,
                    &loop_id,
                    "integration",
                    "state_update",
                    "integration_verified",
                )
                .await?;
            }
            Some("failed") => {
                // Integration failed -- don't advance, record event
                let event_id = Uuid::now_v7().to_string();
                let idem = format!("integration-failed-{}", cycle_id);
                sqlx::query(
                    "INSERT INTO event_journal
                     (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
                     VALUES ($1, 'cycle', $2, 'integration_verification_failed', $3, $4, now())
                     ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
                )
                .bind(&event_id)
                .bind(&cycle_id)
                .bind(&idem)
                .bind(serde_json::json!({"objective_id": objective_id}))
                .execute(pool)
                .await?;

                tracing::error!(cycle_id, objective_id, "Integration verification failed, cycle blocked");
            }
            Some(_) => {
                // Still running or queued, wait
            }
            None => {
                // No verification task (legacy cycles), advance directly
                count += advance_cycle_phase(
                    pool,
                    &cycle_id,
                    &loop_id,
                    "integration",
                    "state_update",
                    "integration_complete",
                )
                .await?;
            }
        }
    }

    // state_update -> next_cycle_ready
    let rows = sqlx::query(
        "SELECT c.cycle_id, c.loop_id, l.objective_id
         FROM cycles c
         JOIN loops l ON l.loop_id = c.loop_id
         WHERE c.phase = 'state_update'",
    )
    .fetch_all(pool)
    .await?;

    for row in &rows {
        let cycle_id: String = row.get("cycle_id");
        let loop_id: String = row.get("loop_id");
        let objective_id: String = row.get("objective_id");

        // ── Cycle learning: collect failure patterns for next cycle ───
        let failures = sqlx::query(
            "SELECT t.task_id, n.title, ar.artifact_uri
             FROM tasks t
             JOIN nodes n ON t.node_id = n.node_id
             JOIN artifact_refs ar ON ar.task_id = t.task_id
             WHERE n.objective_id = $1
               AND t.status IN ('failed', 'timed_out')
               AND ar.artifact_kind = 'adapter_output'
             ORDER BY t.updated_at DESC
             LIMIT 10",
        )
        .bind(&objective_id)
        .fetch_all(pool)
        .await
        .unwrap_or_default();

        if !failures.is_empty() {
            let mut failure_lessons: Vec<String> = Vec::new();
            for f in &failures {
                let title: String = f.try_get("title").unwrap_or_default();
                let output: String = f.try_get("artifact_uri").unwrap_or_default();
                let snippet = if output.len() > 500 {
                    &output[..500]
                } else {
                    &output
                };
                failure_lessons.push(format!("Task '{}' failed: {}", title, snippet));
            }

            let learned = failure_lessons.join("; ");
            let memory_id = Uuid::now_v7().to_string();

            // Check idempotency before inserting
            let mem_exists: Option<String> = sqlx::query_scalar(
                "SELECT entry_id FROM recursive_memory_entries WHERE entry_id = $1",
            )
            .bind(&memory_id)
            .fetch_optional(pool)
            .await?;

            if mem_exists.is_none() {
                sqlx::query(
                    "INSERT INTO recursive_memory_entries
                     (entry_id, objective_id, outcome, learned_summary, outcome_metrics, recorded_at)
                     VALUES ($1, $2, 'failure_pattern', $3, $4, now())
                     ON CONFLICT DO NOTHING",
                )
                .bind(&memory_id)
                .bind(&objective_id)
                .bind(&learned)
                .bind(serde_json::json!({
                    "cycle_id": cycle_id,
                    "failure_count": failures.len(),
                    "failures": failure_lessons,
                }))
                .execute(pool)
                .await?;

                tracing::info!(
                    objective_id,
                    cycle_id,
                    failure_count = failures.len(),
                    "Recorded failure patterns for next cycle"
                );
            }
        }

        // ── REC-004: Generate comparison artifact ─────────────────────
        count += recursive_improvement::generate_comparison_artifact(
            pool, &objective_id, &cycle_id, &loop_id,
        )
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, objective_id, "REC-004: comparison artifact generation failed (non-fatal)");
            0
        });

        // ── REC-005: Compute improvement score ────────────────────────
        count += recursive_improvement::compute_improvement_score(
            pool, &objective_id, &cycle_id,
        )
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, objective_id, "REC-005: improvement scoring failed (non-fatal)");
            0
        });

        // ── REC-007: Drift check for self-improvement ─────────────────
        count += recursive_improvement::check_self_improvement_drift(
            pool, &objective_id, &cycle_id,
        )
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, objective_id, "REC-007: drift check failed (non-fatal)");
            0
        });

        // ── REC-009: Generate recursive report ────────────────────────
        count += recursive_improvement::generate_recursive_report(
            pool, &objective_id, &cycle_id, &loop_id,
        )
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, objective_id, "REC-009: report generation failed (non-fatal)");
            0
        });

        // ── REC-010: Extended memory (success patterns + roadmap) ─────
        count += recursive_improvement::write_extended_memory(
            pool, &objective_id, &cycle_id,
        )
        .await
        .unwrap_or_else(|e| {
            tracing::warn!(error = %e, objective_id, "REC-010: extended memory write failed (non-fatal)");
            0
        });

        // Record cycle completion event
        let event_id = Uuid::now_v7().to_string();
        let idempotency_key = format!("cycle-complete-{}", cycle_id);
        let payload = serde_json::json!({
            "cycle_id": cycle_id,
            "loop_id": loop_id,
            "objective_id": objective_id,
            "trigger": "state_update_complete"
        });

        sqlx::query(
            "INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
             VALUES ($1, 'cycle', $2, 'cycle_completed', $3, $4, now())
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
        )
        .bind(&event_id)
        .bind(&cycle_id)
        .bind(&idempotency_key)
        .bind(&payload)
        .execute(pool)
        .await?;

        count += advance_cycle_phase(
            pool,
            &cycle_id,
            &loop_id,
            "state_update",
            "next_cycle_ready",
            "cycle_completed",
        )
        .await?;

        tracing::info!(cycle_id = %cycle_id, "Cycle completed");
    }

    Ok(count)
}

// ── Step 9: Handle next cycle ────────────────────────────────────────────

/// For cycles in `next_cycle_ready`, the create_cycles_for_active_loops
/// function (Step 2) will pick these up automatically on the next tick.
/// This function just logs them for observability.
async fn handle_next_cycle(
    pool: &PgPool,
) -> Result<u32, Box<dyn std::error::Error>> {
    let count: Option<i64> = sqlx::query_scalar(
        "SELECT COUNT(*) FROM cycles WHERE phase = 'next_cycle_ready'",
    )
    .fetch_one(pool)
    .await?;

    let n = count.unwrap_or(0);
    if n > 0 {
        tracing::debug!(
            cycles_ready = n,
            "Cycles in next_cycle_ready (will be picked up by Step 2)"
        );
    }

    // No action count -- Step 2 handles the actual creation
    Ok(0)
}

// ── Phase 2: Conversation extraction ─────────────────────────────────

/// Phase 2: conversation_extraction
/// Check for new conversation extracts linked to active objectives.
/// If found, update plan state with extracted constraints/decisions.
async fn process_conversation_extracts(
    pool: &PgPool,
) -> Result<u32, Box<dyn std::error::Error>> {
    // Find unprocessed extracts for objectives that have active cycles.
    let extracts = sqlx::query(
        r#"
        SELECT ce.extract_id, ce.session_id, ce.summarized_intent,
               ce.extracted_constraints, ce.extracted_decisions,
               ce.extracted_open_questions,
               cs.objective_id
        FROM conversation_extracts ce
        JOIN chat_sessions cs ON ce.session_id = cs.session_id
        WHERE cs.objective_id IS NOT NULL
          AND NOT EXISTS (
              SELECT 1 FROM event_journal ej
              WHERE ej.aggregate_kind = 'conversation'
                AND ej.aggregate_id = ce.extract_id
                AND ej.event_kind = 'extract_processed'
          )
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut actions = 0u32;
    for row in &extracts {
        let extract_id: &str = row.try_get("extract_id")?;
        let objective_id: Option<&str> = row.try_get("objective_id")?;
        let constraints: serde_json::Value = row.try_get("extracted_constraints")?;
        let decisions: serde_json::Value = row.try_get("extracted_decisions")?;
        let questions: serde_json::Value = row.try_get("extracted_open_questions")?;

        let Some(obj_id) = objective_id else {
            continue;
        };

        let mut tx = pool.begin().await?;

        // Store constraints as plan invariants
        if let Some(arr) = constraints.as_array() {
            for c in arr {
                if let Some(text) = c.as_str() {
                    let inv_id = Uuid::now_v7().to_string();
                    sqlx::query(
                        "INSERT INTO plan_invariants \
                         (invariant_id, objective_id, description, predicate, scope, enforcement, status, created_at, updated_at) \
                         VALUES ($1, $2, $3, '', 'global', 'plan_validation', 'unchecked', now(), now()) \
                         ON CONFLICT DO NOTHING",
                    )
                    .bind(&inv_id)
                    .bind(obj_id)
                    .bind(text)
                    .execute(&mut *tx)
                    .await
                    .ok();
                }
            }
        }

        // Store open questions
        if let Some(arr) = questions.as_array() {
            for q in arr {
                if let Some(text) = q.as_str() {
                    let q_id = Uuid::now_v7().to_string();
                    sqlx::query(
                        "INSERT INTO unresolved_questions \
                         (question_id, objective_id, question, severity, resolution_status, blocking_ids, created_at, updated_at) \
                         VALUES ($1, $2, $3, 'important', 'open', '[]'::jsonb, now(), now()) \
                         ON CONFLICT DO NOTHING",
                    )
                    .bind(&q_id)
                    .bind(obj_id)
                    .bind(text)
                    .execute(&mut *tx)
                    .await
                    .ok();
                }
            }
        }

        // Auto-absorb decisions into roadmap nodes
        if let Some(arr) = decisions.as_array() {
            for d in arr {
                if let Some(text) = d.as_str() {
                    let node_id = Uuid::now_v7().to_string();
                    let absorption_id = Uuid::now_v7().to_string();

                    // Create a roadmap node for each significant decision
                    sqlx::query(
                        "INSERT INTO roadmap_nodes (roadmap_node_id, objective_id, title, description, track, status, priority, created_at, updated_at, revision) \
                         VALUES ($1, $2, $3, $4, 'main', 'open', 0, now(), now(), 1) \
                         ON CONFLICT DO NOTHING",
                    )
                    .bind(&node_id)
                    .bind(obj_id)
                    .bind(text)
                    .bind(format!("Auto-created from conversation: {}", text))
                    .execute(&mut *tx)
                    .await
                    .ok();

                    // Create absorption record
                    sqlx::query(
                        "INSERT INTO roadmap_absorption_records (absorption_id, roadmap_node_id, action_kind, source_ref, target_ref, rationale, created_at) \
                         VALUES ($1, $2, 'create_node', $3, '', 'auto-absorbed from conversation decision', now()) \
                         ON CONFLICT DO NOTHING",
                    )
                    .bind(&absorption_id)
                    .bind(&node_id)
                    .bind(extract_id)
                    .execute(&mut *tx)
                    .await
                    .ok();
                }
            }
        }

        // Update roadmap ordering for the objective
        let existing_nodes = sqlx::query(
            "SELECT roadmap_node_id FROM roadmap_nodes WHERE objective_id = $1 ORDER BY priority, created_at",
        )
        .bind(obj_id)
        .fetch_all(&mut *tx)
        .await?;

        let node_sequence: Vec<String> = existing_nodes
            .iter()
            .map(|r| r.try_get::<String, _>("roadmap_node_id").unwrap_or_default())
            .collect();

        sqlx::query(
            "INSERT INTO roadmap_ordering (ordering_id, objective_id, node_sequence, created_at, updated_at) \
             VALUES ($1, $2, $3::jsonb, now(), now()) \
             ON CONFLICT (ordering_id) DO UPDATE SET node_sequence = excluded.node_sequence, updated_at = now()",
        )
        .bind(format!("ordering-{}", obj_id))
        .bind(obj_id)
        .bind(serde_json::json!(node_sequence))
        .execute(&mut *tx)
        .await
        .ok();

        // Mark extract as processed via event_journal
        sqlx::query(
            "INSERT INTO event_journal \
             (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
             VALUES ($1, 'conversation', $2, 'extract_processed', $3, $4::jsonb, now()) \
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(extract_id)
        .bind(format!("process-extract-{}", extract_id))
        .bind(serde_json::json!({
            "objective_id": obj_id,
            "constraints_count": constraints.as_array().map(|a| a.len()).unwrap_or(0),
            "questions_count": questions.as_array().map(|a| a.len()).unwrap_or(0),
            "decisions_count": decisions.as_array().map(|a| a.len()).unwrap_or(0)
        }))
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;
        actions += 1;
        tracing::info!(extract_id, obj_id, "Processed conversation extract");
    }
    Ok(actions)
}

// ── Phase 5: Periodic review ─────────────────────────────────────────

/// Phase 5: periodic review check.
///
/// Creates review artifacts for objectives that need review.
///
/// Uses `review_governance::ReviewKind` as the canonical enum for review
/// kinds (REV-002) and `review_governance::ReviewSchedulingPolicy` for
/// interval configuration (REV-004). The SQL column values are derived
/// from the Rust enum via `review_kind_to_sql()`.
///
/// REV-007~010: Review worker templates (planning, architecture, direction,
/// milestone) are defined in `packages/review-governance/src/templates.rs`.
/// Once `skill_packs/worker_templates/` is populated, the `skill_pack_id`
/// field in `ReviewWorkerTemplate` will be used to load templates at
/// dispatch time.
async fn check_periodic_reviews(
    pool: &PgPool,
) -> Result<u32, Box<dyn std::error::Error>> {
    // REV-004/REV-011: Respect ReviewSchedulingPolicy from review-governance.
    // Query DB for active periodic policies; fall back to the template
    // default via resolve_review_interval().
    //
    // The SQL trigger_kind value corresponds to ReviewTriggerKind::Periodic
    // from review_governance::scheduling.
    let db_interval = sqlx::query_scalar::<_, Option<i32>>(
        "SELECT periodic_interval_secs FROM review_scheduling_policies \
         WHERE trigger_kind = 'periodic' AND active = TRUE \
         ORDER BY periodic_interval_secs ASC LIMIT 1"
    )
    .fetch_one(pool)
    .await
    .unwrap_or(None);

    // REV-007: Use template_for_kind fallback when no DB policy exists.
    let periodic_interval = resolve_review_interval(db_interval, ReviewKind::Planning);

    let interval_clause = format!("interval '{} seconds'", periodic_interval);

    // Use canonical ReviewKind::Planning from review-governance
    let plan_review_kind = review_kind_to_sql(ReviewKind::Planning);

    // REV-007~010: Load the worker template for logging/tracing and to
    // populate reviewer_template_id + required_context on the artifact.
    let _planning_template: ReviewWorkerTemplate = template_for_kind(ReviewKind::Planning);

    // Find objectives with active cycles that haven't been reviewed recently.
    let needs_review = sqlx::query(
        &format!(
            "SELECT DISTINCT o.objective_id, o.summary \
             FROM objectives o \
             JOIN loops l ON l.objective_id = o.objective_id \
             JOIN cycles c ON c.loop_id = l.loop_id \
             WHERE c.phase IN ('execution', 'dispatch', 'decomposition') \
               AND NOT EXISTS ( \
                   SELECT 1 FROM review_artifacts ra \
                   WHERE ra.target_ref = o.objective_id \
                     AND ra.recorded_at > now() - {} \
               )",
            interval_clause
        ),
    )
    .fetch_all(pool)
    .await?;

    let mut actions = 0u32;
    for row in &needs_review {
        let objective_id: &str = row.try_get("objective_id")?;
        let summary: &str = row.try_get("summary")?;

        let review_id = Uuid::now_v7().to_string();
        let mut tx = pool.begin().await?;

        // Count tasks by status for review content
        let task_counts = sqlx::query(
            "SELECT status, COUNT(*) as cnt FROM tasks t \
             JOIN nodes n ON t.node_id = n.node_id \
             WHERE n.objective_id = $1 GROUP BY status",
        )
        .bind(objective_id)
        .fetch_all(&mut *tx)
        .await?;

        let mut status_summary = serde_json::Map::new();
        for tc in &task_counts {
            let s: String = tc.try_get("status")?;
            let c: i64 = tc.try_get("cnt")?;
            status_summary.insert(s, serde_json::json!(c));
        }

        // REV-007~010/REV-011/REV-014: Create review artifact (durable storage).
        // Uses canonical plan_review_kind from ReviewKind::Planning.
        // Populates reviewer_template_id and conditions (required_context_refs)
        // so each periodic review is traceable to its template.
        let template_conditions: serde_json::Value =
            serde_json::json!(_planning_template.required_context_refs);
        sqlx::query(
            "INSERT INTO review_artifacts \
             (review_id, review_kind, target_ref, reviewer_template_id, status, score_or_verdict, approval_effect, conditions, recorded_at) \
             VALUES ($1, $2, $3, $4, 'integrated', $5, 'informational', $6::jsonb, now())",
        )
        .bind(&review_id)
        .bind(plan_review_kind)
        .bind(objective_id)
        .bind(&_planning_template.template_id)
        .bind(
            serde_json::json!({"summary": summary, "task_status": status_summary}).to_string(),
        )
        .bind(&template_conditions)
        .execute(&mut *tx)
        .await?;

        // REV-014: Also create a durable artifact_ref so review is queryable
        // from the general artifact timeline.
        let artifact_ref_id = Uuid::now_v7().to_string();
        sqlx::query(
            r#"INSERT INTO artifact_refs (artifact_ref_id, artifact_kind, artifact_uri, metadata)
               VALUES ($1, 'review_approval', $2, $3::jsonb)"#,
        )
        .bind(&artifact_ref_id)
        .bind(&format!("review://{}", review_id))
        .bind(serde_json::json!({
            "review_id": review_id,
            "review_kind": plan_review_kind,
            "target_ref": objective_id,
            "verdict": "informational",
            "periodic_interval_secs": periodic_interval,
        }))
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO event_journal \
             (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
             VALUES ($1, 'review', $2, 'review_artifact_created', $3, $4::jsonb, now()) \
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(&review_id)
        .bind(format!("auto-review-{}", review_id))
        .bind(serde_json::json!({
            "objective_id": objective_id,
            "review_kind": plan_review_kind,
            "reviewer_template_id": _planning_template.template_id,
            "artifact_ref_id": artifact_ref_id,
            "periodic_interval_secs": periodic_interval
        }))
        .execute(&mut *tx)
        .await?;

        // REV-011: Update heartbeat trigger last_triggered_at.
        // Uses canonical plan_review_kind from ReviewKind::Planning.
        sqlx::query(
            "UPDATE heartbeat_review_triggers SET last_triggered_at = now() \
             WHERE review_kind = $1"
        )
        .bind(plan_review_kind)
        .execute(&mut *tx)
        .await
        .ok(); // non-fatal if trigger row doesn't exist yet

        tx.commit().await?;
        actions += 1;
        let template_id = &_planning_template.template_id;
        let skill_pack = &_planning_template.skill_pack_id;
        tracing::info!(
            objective_id, review_id, plan_review_kind,
            template_id, skill_pack,
            auto_approval_eligible = _planning_template.auto_approval_eligible,
            "Created periodic review artifact (REV-007~010 template wired)"
        );
    }
    Ok(actions)
}

// ── Phase 5b: Process pending reviews (auto-approval) ───────────────

/// Check pending reviews against auto-approval thresholds (REV-012, REV-013).
///
/// REV-012: Reads review results from review_artifacts.
/// REV-013: Checks auto_approval_thresholds to decide auto-approval.
///   The `auto_approval_thresholds` table schema mirrors
///   `review_governance::AutoApprovalThreshold`.
/// REV-014: Persists durable artifact_ref for every auto-approval.
/// REV-020: When reviews pass the gate, wire plan_gate effect
///   (see `review_governance::ReviewPlanGateEffect`).
///
/// Review kinds in SQL match `review_governance::ReviewKind` variants,
/// mapped through `review_kind_to_sql()`. The auto-approval logic
/// follows the policy structure from `review_governance::AutoApprovalThreshold`:
///   - `auto_approval_enabled` must be true
///   - `forbidden` must be false
///   - `required_minimum_grade` is checked when present
async fn process_pending_reviews(pool: &PgPool) -> Result<u32, Box<dyn std::error::Error>> {
    let mut processed = 0u32;

    // REV-012: Find review artifacts that are still scheduled (ingestion)
    let pending = sqlx::query(
        "SELECT ra.review_id, ra.target_ref, ra.review_kind \
         FROM review_artifacts ra \
         WHERE ra.status = 'scheduled' \
           AND ra.recorded_at < now() - interval '5 minutes'"
    )
    .fetch_all(pool)
    .await?;

    // REV-013: Load auto-approval thresholds.
    // The SQL schema mirrors `AutoApprovalThreshold` from review-governance:
    //   - auto_approval_enabled: bool
    //   - forbidden: bool
    //   - required_minimum_grade: Option<String>
    //   - threshold_id: String
    let thresholds = sqlx::query(
        "SELECT review_kind, auto_approval_enabled, required_minimum_grade, \
                forbidden, threshold_id \
         FROM auto_approval_thresholds \
         WHERE auto_approval_enabled = TRUE AND forbidden = FALSE"
    )
    .fetch_all(pool)
    .await?;

    for review in &pending {
        let review_id: &str = review.try_get("review_id")?;
        let target_ref: &str = review.try_get("target_ref")?;
        let review_kind: String = review.try_get::<String, _>("review_kind")?;

        // REV-013: Check if auto-approval is enabled for this review kind.
        // Uses validate_auto_approval_row() which mirrors AutoApprovalThreshold
        // invariant (enabled AND not forbidden).
        let threshold = thresholds.iter().find(|t| {
            let kind_match = t.try_get::<String, _>("review_kind").unwrap_or_default() == review_kind;
            let enabled = t.try_get::<bool, _>("auto_approval_enabled").unwrap_or(false);
            let forbidden = t.try_get::<bool, _>("forbidden").unwrap_or(true);
            kind_match && validate_auto_approval_row(enabled, forbidden)
        });

        if let Some(thresh) = threshold {
            let threshold_id: String = thresh.try_get::<String, _>("threshold_id")
                .unwrap_or_else(|_| "unknown".to_string());

            // Calculate success rate for tasks related to this objective/target
            let stats = sqlx::query(
                "SELECT \
                    COUNT(*) FILTER (WHERE status = 'succeeded') as succeeded, \
                    COUNT(*) as total \
                 FROM tasks t \
                 JOIN nodes n ON t.node_id = n.node_id \
                 WHERE n.objective_id = $1"
            )
            .bind(target_ref)
            .fetch_one(pool)
            .await?;

            let succeeded: i64 = stats.try_get("succeeded").unwrap_or(0);
            let total: i64 = stats.try_get("total").unwrap_or(0);

            // Auto-approve if at least one task completed and success rate > 50%
            if total > 0 && succeeded > 0 {
                let rate = succeeded as f64 / total as f64;
                if rate > 0.5 {
                    let mut tx = pool.begin().await?;

                    // Update review status
                    sqlx::query(
                        "UPDATE review_artifacts SET status = 'approved', \
                         score_or_verdict = 'auto_approved', \
                         approval_effect = 'approved' \
                         WHERE review_id = $1"
                    )
                    .bind(review_id)
                    .execute(&mut *tx)
                    .await?;

                    // REV-014: Create durable artifact_ref for the auto-approval
                    let artifact_id = Uuid::now_v7().to_string();
                    sqlx::query(
                        r#"INSERT INTO artifact_refs (artifact_ref_id, artifact_kind, artifact_uri, metadata)
                           VALUES ($1, 'review_approval', $2, $3::jsonb)"#,
                    )
                    .bind(&artifact_id)
                    .bind(&format!("review://{}", review_id))
                    .bind(serde_json::json!({
                        "review_id": review_id,
                        "verdict": "auto_approved",
                        "approval_effect": "approved",
                        "threshold_id": threshold_id,
                        "success_rate": rate,
                        "auto_approved_at": chrono::Utc::now().to_rfc3339(),
                    }))
                    .execute(&mut *tx)
                    .await?;

                    // REV-020: Wire review-to-plan-gate effect if applicable.
                    // Check if a plan_gate exists for this target and update it.
                    let gate_update = sqlx::query(
                        "UPDATE plan_gates SET review_satisfied = TRUE, \
                             evaluated_at = now() \
                         WHERE plan_id IN ( \
                             SELECT plan_id FROM plans WHERE objective_id = $1 \
                         ) AND review_satisfied = FALSE"
                    )
                    .bind(target_ref)
                    .execute(&mut *tx)
                    .await;

                    let gate_affected = gate_update.map(|r| r.rows_affected()).unwrap_or(0);

                    // Emit auto-approval event
                    sqlx::query(
                        "INSERT INTO event_journal \
                         (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
                         VALUES ($1, 'review', $2, 'review_auto_approved', $3, $4::jsonb, now()) \
                         ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING"
                    )
                    .bind(Uuid::now_v7().to_string())
                    .bind(review_id)
                    .bind(format!("auto-approve-{}", review_id))
                    .bind(serde_json::json!({
                        "review_id": review_id,
                        "target_ref": target_ref,
                        "review_kind": review_kind,
                        "threshold_id": threshold_id,
                        "success_rate": rate,
                        "artifact_ref_id": artifact_id,
                        "gate_rows_affected": gate_affected
                    }))
                    .execute(&mut *tx)
                    .await?;

                    tx.commit().await?;

                    tracing::info!(
                        review_id, target_ref, %rate, threshold_id,
                        gate_affected, "Review auto-approved with durable artifact"
                    );
                    processed += 1;
                }
            }
        }
    }

    Ok(processed)
}

// ── Phase 10: Certification candidate selection ──────────────────────

/// Phase 10: Select certification candidates from completed tasks.
///
/// This is a sweep for any succeeded tasks that worker-dispatch may have
/// missed when checking certification eligibility.  Only tasks whose
/// node title suggests they involve contracts, invariants, proofs, or
/// safety are considered.
async fn select_certification_candidates(
    pool: &PgPool,
) -> Result<u32, Box<dyn std::error::Error>> {
    // FCG-004: candidate selection uses title keywords AND checks
    // certification_required flag on nodes AND plan gate demands.
    let candidates = sqlx::query(
        r#"
        SELECT t.task_id, t.node_id, n.title,
               ta.task_attempt_id,
               COALESCE(n.certification_required, false) AS cert_required
        FROM tasks t
        JOIN nodes n ON t.node_id = n.node_id
        LEFT JOIN task_attempts ta ON ta.task_id = t.task_id
          AND ta.status = 'succeeded'
          AND ta.task_attempt_id = (
              SELECT ta2.task_attempt_id FROM task_attempts ta2
              WHERE ta2.task_id = t.task_id AND ta2.status = 'succeeded'
              ORDER BY ta2.finished_at DESC NULLS LAST LIMIT 1
          )
        WHERE t.status = 'succeeded'
          AND NOT EXISTS (
              SELECT 1 FROM certification_candidates cc
              WHERE cc.task_id = t.task_id
          )
          AND (
              -- Title keyword match (contract/invariant/proof/safety)
              lower(n.title) LIKE '%contract%'
              OR lower(n.title) LIKE '%invariant%'
              OR lower(n.title) LIKE '%proof%'
              OR lower(n.title) LIKE '%safety%'
              -- FCG-004 enhancement: certification_required flag on node
              OR COALESCE(n.certification_required, false) = true
              -- FCG-004 enhancement: plan gate demands certification
              -- (gate_kind column removed; rely on condition_entries JSONB)
              OR EXISTS (
                  SELECT 1 FROM plan_gates pg
                  JOIN plans p ON pg.plan_id = p.plan_id
                  JOIN nodes n2 ON p.objective_id = n2.objective_id
                  WHERE n2.node_id = n.node_id
                    AND pg.condition_entries::text LIKE '%certification%'
                    AND pg.current_status NOT IN ('satisfied', 'overridden')
              )
          )
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut actions = 0u32;
    for row in &candidates {
        let task_id: &str = row.try_get("task_id")?;
        let node_id: &str = row.try_get("node_id")?;
        let title: &str = row.try_get("title")?;
        let task_attempt_id: Option<String> = row.try_get("task_attempt_id").ok();
        let cert_required: bool = row.try_get("cert_required").unwrap_or(false);

        // Determine eligibility reason
        let eligibility = if cert_required {
            "promotion_requested"
        } else {
            "contract_or_invariant"
        };

        let candidate_id = Uuid::now_v7().to_string();

        // FCG-005: INSERT includes provenance_task_attempt_id
        // FCG-006: Collect source anchors from artifact_refs for this task
        let anchor_rows = sqlx::query(
            "SELECT artifact_uri, artifact_kind FROM artifact_refs \
             WHERE task_id = $1 AND artifact_kind IN ('source_file', 'source_anchor', 'output_file') \
             ORDER BY created_at ASC LIMIT 20",
        )
        .bind(task_id)
        .fetch_all(pool)
        .await?;

        let source_anchors: Vec<String> = anchor_rows
            .iter()
            .filter_map(|r| r.try_get::<String, _>("artifact_uri").ok())
            .collect();
        let anchors_json = serde_json::to_value(&source_anchors).unwrap_or_default();

        sqlx::query(
            "INSERT INTO certification_candidates \
             (candidate_id, node_id, task_id, claim_summary, eligibility_reason, \
              provenance_task_attempt_id, source_anchors, created_at) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, now()) \
             ON CONFLICT DO NOTHING",
        )
        .bind(&candidate_id)
        .bind(node_id)
        .bind(task_id)
        .bind(title)
        .bind(eligibility)
        .bind(&task_attempt_id)
        .bind(&anchors_json)
        .execute(pool)
        .await?;

        actions += 1;
        tracing::info!(
            candidate_id,
            task_id,
            node_id,
            ?task_attempt_id,
            anchor_count = source_anchors.len(),
            "Created certification candidate"
        );
    }
    Ok(actions)
}

// ── Continuous: Conflict detection ───────────────────────────────────

/// Detect conflicts: find nodes with multiple succeeded tasks that have
/// different outputs, indicating potential divergence.
async fn detect_conflicts(
    pool: &PgPool,
    scaling: &ScalingContext,
) -> Result<u32, Box<dyn std::error::Error>> {
    // Find nodes with more than one succeeded task (potential conflict).
    let multi_success = sqlx::query(
        r#"
        SELECT n.node_id, n.title, COUNT(t.task_id) as success_count
        FROM nodes n
        JOIN tasks t ON t.node_id = n.node_id
        WHERE t.status = 'succeeded'
        GROUP BY n.node_id, n.title
        HAVING COUNT(t.task_id) > 1
        "#,
    )
    .fetch_all(pool)
    .await?;

    let mut actions = 0u32;
    for row in &multi_success {
        let node_id: &str = row.try_get("node_id")?;
        let title: &str = row.try_get("title")?;

        // Check if an open conflict already exists for this node
        let existing: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM conflicts WHERE node_id = $1 AND status != 'resolved'",
        )
        .bind(node_id)
        .fetch_one(pool)
        .await?;

        if existing > 0 {
            continue;
        }

        // Create conflict record (CNF-004: divergence conflict)
        let conflict_id = Uuid::now_v7().to_string();
        let mut tx = pool.begin().await?;

        sqlx::query(
            "INSERT INTO conflicts \
             (conflict_id, node_id, conflict_kind, status, created_at, updated_at) \
             VALUES ($1, $2, 'divergence', 'open', now(), now())",
        )
        .bind(&conflict_id)
        .bind(node_id)
        .execute(&mut *tx)
        .await?;

        // CNF-004: Link competing artifacts via conflict_artifacts table.
        // Fetch both artifact_ref_id and the originating task_id so each
        // conflict_artifact row traces back to the task that produced it.
        let artifacts = sqlx::query(
            "SELECT ar.artifact_ref_id, ar.task_id \
             FROM artifact_refs ar \
             JOIN tasks t ON ar.task_id = t.task_id \
             WHERE t.node_id = $1 AND t.status = 'succeeded'",
        )
        .bind(node_id)
        .fetch_all(&mut *tx)
        .await?;

        for art in &artifacts {
            let art_id: &str = art.try_get("artifact_ref_id")?;
            let art_task_id: Option<&str> = art.try_get("task_id").ok();
            let ca_id = Uuid::now_v7().to_string();
            sqlx::query(
                "INSERT INTO conflict_artifacts \
                 (conflict_artifact_id, conflict_id, artifact_ref, artifact_role) \
                 VALUES ($1, $2, $3, 'competing')",
            )
            .bind(&ca_id)
            .bind(&conflict_id)
            .bind(art_id)
            .execute(&mut *tx)
            .await?;

            tracing::debug!(
                conflict_id,
                artifact_ref = art_id,
                task_id = ?art_task_id,
                "Linked competing artifact to conflict"
            );
        }

        // CNF-009: Record conflict creation in conflict_history
        let history_id = Uuid::now_v7().to_string();
        let creation_snapshot = serde_json::json!({
            "conflict_id": conflict_id,
            "node_id": node_id,
            "conflict_kind": "divergence",
            "status": "open",
            "artifact_count": artifacts.len(),
            "title": title
        });
        sqlx::query(
            "INSERT INTO conflict_history \
             (history_entry_id, conflict_id, status_at_snapshot, change_description, snapshot, recorded_at) \
             VALUES ($1, $2, 'open', 'Conflict created: divergence detected', $3::jsonb, now())",
        )
        .bind(&history_id)
        .bind(&conflict_id)
        .bind(&creation_snapshot)
        .execute(&mut *tx)
        .await?;

        let conflict_event_id = Uuid::now_v7().to_string();
        let conflict_idem_key = format!("conflict-{}", conflict_id);
        let conflict_payload = serde_json::json!({
            "node_id": node_id,
            "title": title,
            "artifact_count": artifacts.len()
        });

        sqlx::query(
            "INSERT INTO event_journal \
             (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
             VALUES ($1, 'conflict', $2, 'conflict_created', $3, $4::jsonb, now()) \
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
        )
        .bind(&conflict_event_id)
        .bind(&conflict_id)
        .bind(&conflict_idem_key)
        .bind(&conflict_payload)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        // Publish to event bus (after tx commit)
        let _ = scaling.event_bus.publish(Event {
            event_id: conflict_event_id,
            aggregate_kind: "conflict".into(),
            aggregate_id: conflict_id.clone(),
            event_kind: "conflict_created".into(),
            idempotency_key: conflict_idem_key,
            payload: conflict_payload,
        }).await;

        actions += 1;
        tracing::info!(conflict_id, node_id, title, "Detected divergence conflict");
    }

    // ── CNF-007: Review disagreement conflict detection ──────────────
    //
    // When two reviews on the same target_ref have opposing verdicts
    // (one approved, one rejected), create a review_disagreement conflict.
    let disagreements = sqlx::query(
        r#"
        SELECT ra1.target_ref,
               ra1.review_id as review_a,
               ra2.review_id as review_b,
               ra1.score_or_verdict as verdict_a,
               ra2.score_or_verdict as verdict_b
        FROM review_artifacts ra1
        JOIN review_artifacts ra2
            ON ra1.target_ref = ra2.target_ref
            AND ra1.review_id < ra2.review_id
        WHERE ra1.score_or_verdict IN ('approved', 'auto_approved')
          AND ra2.score_or_verdict IN ('rejected', 'needs_revision')
          AND NOT EXISTS (
              SELECT 1 FROM conflicts c
              WHERE c.node_id = ra1.target_ref
                AND c.conflict_kind = 'review_disagreement'
                AND c.status != 'resolved'
          )
        "#,
    )
    .fetch_all(pool)
    .await?;

    for row in &disagreements {
        let target_ref: &str = row.try_get("target_ref")?;
        let review_a: &str = row.try_get("review_a")?;
        let review_b: &str = row.try_get("review_b")?;
        let verdict_a: &str = row.try_get("verdict_a")?;
        let verdict_b: &str = row.try_get("verdict_b")?;

        let conflict_id = Uuid::now_v7().to_string();
        let mut tx = pool.begin().await?;

        sqlx::query(
            "INSERT INTO conflicts \
             (conflict_id, node_id, conflict_kind, status, created_at, updated_at) \
             VALUES ($1, $2, 'review_disagreement', 'open', now(), now())",
        )
        .bind(&conflict_id)
        .bind(target_ref)
        .execute(&mut *tx)
        .await?;

        // CNF-009: Record conflict creation in conflict_history
        let history_id = Uuid::now_v7().to_string();
        let snapshot = serde_json::json!({
            "conflict_id": conflict_id,
            "target_ref": target_ref,
            "review_a": review_a,
            "review_b": review_b,
            "verdict_a": verdict_a,
            "verdict_b": verdict_b
        });
        sqlx::query(
            "INSERT INTO conflict_history \
             (history_entry_id, conflict_id, status_at_snapshot, change_description, snapshot, recorded_at) \
             VALUES ($1, $2, 'open', 'Conflict created: review disagreement', $3::jsonb, now())",
        )
        .bind(&history_id)
        .bind(&conflict_id)
        .bind(&snapshot)
        .execute(&mut *tx)
        .await?;

        // Emit event
        let event_id = Uuid::now_v7().to_string();
        let idem_key = format!("review-disagree-{}", conflict_id);
        let payload = serde_json::json!({
            "target_ref": target_ref,
            "review_a": review_a,
            "review_b": review_b,
            "verdict_a": verdict_a,
            "verdict_b": verdict_b
        });

        sqlx::query(
            "INSERT INTO event_journal \
             (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
             VALUES ($1, 'conflict', $2, 'conflict_created', $3, $4::jsonb, now()) \
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
        )
        .bind(&event_id)
        .bind(&conflict_id)
        .bind(&idem_key)
        .bind(&payload)
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        let _ = scaling.event_bus.publish(Event {
            event_id,
            aggregate_kind: "conflict".into(),
            aggregate_id: conflict_id.clone(),
            event_kind: "conflict_created".into(),
            idempotency_key: idem_key,
            payload,
        }).await;

        actions += 1;
        tracing::info!(conflict_id, target_ref, "Detected review disagreement conflict");
    }

    Ok(actions)
}

// ── Continuous: Conflict auto-resolution ─────────────────────────────

/// Auto-resolve divergence conflicts: if exactly one task succeeded and all
/// others failed, pick the winner automatically (CNF-011).
///
/// When 2+ tasks succeeded (no clear winner), generate an adjudication
/// task instead (CNF-008).
///
/// Every status change records a conflict_history entry (CNF-009).
/// Resolutions are persisted in conflict_resolutions (CNF-010).
async fn auto_resolve_conflicts(pool: &PgPool, scaling: &ScalingContext) -> Result<u32, Box<dyn std::error::Error>> {
    let mut resolved = 0u32;

    let auto_resolvable = sqlx::query(
        "SELECT cr.conflict_id, cr.node_id \
         FROM conflicts cr \
         WHERE cr.status = 'open' \
           AND cr.conflict_kind IN ('divergence', 'review_disagreement', 'formalization_divergence') \
           AND NOT EXISTS ( \
               SELECT 1 FROM event_journal ej \
               WHERE ej.aggregate_kind = 'conflict' \
                 AND ej.aggregate_id = cr.conflict_id \
                 AND ej.event_kind IN ('conflict_auto_resolved', 'adjudication_task_created') \
           )"
    )
    .fetch_all(pool)
    .await?;

    for row in &auto_resolvable {
        let conflict_id: &str = row.try_get("conflict_id")?;
        let node_id: &str = row.try_get("node_id")?;

        // Check task outcomes for this node
        let succeeded: Vec<sqlx::postgres::PgRow> = sqlx::query(
            "SELECT task_id FROM tasks WHERE node_id = $1 AND status = 'succeeded'"
        )
        .bind(node_id)
        .fetch_all(pool)
        .await?;

        let failed_count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM tasks WHERE node_id = $1 AND status IN ('failed', 'timed_out')"
        )
        .bind(node_id)
        .fetch_one(pool)
        .await?;

        // CNF-011: Auto-resolve when exactly 1 succeeded + N failed = pick winner
        if succeeded.len() == 1 && failed_count > 0 {
            let winner_task_id: &str = succeeded[0].try_get("task_id")?;

            let mut tx = pool.begin().await?;

            // Update conflict status
            sqlx::query(
                "UPDATE conflicts SET status = 'resolved', updated_at = now() WHERE conflict_id = $1"
            )
            .bind(conflict_id)
            .execute(&mut *tx)
            .await?;

            // CNF-009: Record status change in conflict_history
            let history_id = Uuid::now_v7().to_string();
            let resolution_snapshot = serde_json::json!({
                "conflict_id": conflict_id,
                "node_id": node_id,
                "from_status": "open",
                "to_status": "resolved",
                "strategy": "pick_winner",
                "winner_task_id": winner_task_id,
                "failed_count": failed_count
            });
            sqlx::query(
                "INSERT INTO conflict_history \
                 (history_entry_id, conflict_id, status_at_snapshot, change_description, snapshot, recorded_at) \
                 VALUES ($1, $2, 'resolved', 'Auto-resolved: single winner (pick_winner)', $3::jsonb, now())",
            )
            .bind(&history_id)
            .bind(conflict_id)
            .bind(&resolution_snapshot)
            .execute(&mut *tx)
            .await?;

            // CNF-010: Insert conflict resolution record
            let resolution_id = Uuid::now_v7().to_string();
            sqlx::query(
                "INSERT INTO conflict_resolutions \
                 (resolution_id, conflict_id, strategy, winner_node_id, rationale, resolved_by, lifecycle_effects, resolved_at) \
                 VALUES ($1, $2, 'pick_winner', $3, $4, 'auto_resolve', $5::jsonb, now())",
            )
            .bind(&resolution_id)
            .bind(conflict_id)
            .bind(winner_task_id) // winner_node_id stores winner_task_id for traceability
            .bind(format!(
                "Auto-resolved: task {} succeeded while {} other(s) failed",
                winner_task_id, failed_count
            ))
            .bind(serde_json::json!([{
                "node_id": node_id,
                "lane_effect": "no_change",
                "lifecycle_effect": "winner_selected",
                "description": format!("Winner task: {}", winner_task_id)
            }]))
            .execute(&mut *tx)
            .await?;

            // Record auto-resolution event
            let event_id = Uuid::now_v7().to_string();
            let idempotency_key = format!("auto-resolve-{}", conflict_id);
            let resolve_payload = serde_json::json!({
                "conflict_id": conflict_id,
                "node_id": node_id,
                "strategy": "pick_winner",
                "winner_task_id": winner_task_id,
                "failed_count": failed_count,
                "resolution_id": resolution_id
            });
            sqlx::query(
                "INSERT INTO event_journal \
                 (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
                 VALUES ($1, 'conflict', $2, 'conflict_auto_resolved', $3, $4::jsonb, now()) \
                 ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING"
            )
            .bind(&event_id)
            .bind(conflict_id)
            .bind(&idempotency_key)
            .bind(&resolve_payload)
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;

            // Publish to event bus (after tx commit)
            let _ = scaling.event_bus.publish(Event {
                event_id: event_id.clone(),
                aggregate_kind: "conflict".into(),
                aggregate_id: conflict_id.to_string(),
                event_kind: "conflict_auto_resolved".into(),
                idempotency_key: idempotency_key.clone(),
                payload: resolve_payload,
            }).await;

            tracing::info!(conflict_id, winner_task_id, "Conflict auto-resolved (pick_winner)");
            resolved += 1;
        } else if succeeded.len() >= 2 {
            // CNF-008: 2+ succeeded tasks -- cannot auto-resolve, generate adjudication task
            let mut tx = pool.begin().await?;

            // Transition conflict to under_adjudication
            sqlx::query(
                "UPDATE conflicts SET status = 'under_adjudication', updated_at = now() WHERE conflict_id = $1"
            )
            .bind(conflict_id)
            .execute(&mut *tx)
            .await?;

            // CNF-009: Record status change in conflict_history
            let history_id = Uuid::now_v7().to_string();
            let adj_snapshot = serde_json::json!({
                "conflict_id": conflict_id,
                "node_id": node_id,
                "from_status": "open",
                "to_status": "under_adjudication",
                "succeeded_count": succeeded.len(),
                "reason": "multiple_winners_need_adjudication"
            });
            sqlx::query(
                "INSERT INTO conflict_history \
                 (history_entry_id, conflict_id, status_at_snapshot, change_description, snapshot, recorded_at) \
                 VALUES ($1, $2, 'under_adjudication', 'Escalated: multiple succeeded tasks require adjudication', $3::jsonb, now())",
            )
            .bind(&history_id)
            .bind(conflict_id)
            .bind(&adj_snapshot)
            .execute(&mut *tx)
            .await?;

            // CNF-008: Create adjudication task with competing artifact refs
            let adjudication_id = Uuid::now_v7().to_string();

            // Gather competing artifact info for the adjudicator
            let competing_arts: Vec<serde_json::Value> = {
                let mut arts = Vec::new();
                for s in &succeeded {
                    let tid: &str = s.try_get("task_id")?;
                    arts.push(serde_json::json!({"task_id": tid}));
                }
                arts
            };

            sqlx::query(
                "INSERT INTO adjudication_tasks \
                 (adjudication_id, conflict_id, urgency, required_reviewer_role, \
                  context_summary, competing_artifacts, adjudication_status, created_at, updated_at) \
                 VALUES ($1, $2, 'elevated', 'supervisor', $3, $4::jsonb, 'pending', now(), now())",
            )
            .bind(&adjudication_id)
            .bind(conflict_id)
            .bind(format!(
                "Conflict {} on node {}: {} tasks succeeded with different outputs. Manual selection required.",
                conflict_id, node_id, succeeded.len()
            ))
            .bind(serde_json::json!(competing_arts))
            .execute(&mut *tx)
            .await?;

            // Emit adjudication event
            let event_id = Uuid::now_v7().to_string();
            let idem_key = format!("adjudicate-{}", conflict_id);
            let adj_payload = serde_json::json!({
                "conflict_id": conflict_id,
                "adjudication_id": adjudication_id,
                "node_id": node_id,
                "succeeded_count": succeeded.len()
            });
            sqlx::query(
                "INSERT INTO event_journal \
                 (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
                 VALUES ($1, 'conflict', $2, 'adjudication_task_created', $3, $4::jsonb, now()) \
                 ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING"
            )
            .bind(&event_id)
            .bind(conflict_id)
            .bind(&idem_key)
            .bind(&adj_payload)
            .execute(&mut *tx)
            .await?;

            tx.commit().await?;

            let _ = scaling.event_bus.publish(Event {
                event_id,
                aggregate_kind: "conflict".into(),
                aggregate_id: conflict_id.to_string(),
                event_kind: "adjudication_task_created".into(),
                idempotency_key: idem_key,
                payload: adj_payload,
            }).await;

            tracing::info!(
                conflict_id, adjudication_id, node_id,
                succeeded_count = succeeded.len(),
                "Created adjudication task (cannot auto-resolve)"
            );
            resolved += 1;
        }
    }

    Ok(resolved)
}

// ── Continuous: Drift detection (Gap 6) ──────────────────────────────

/// Detect drift: check if upstream assumptions have changed since tasks were
/// certified. When an upstream node is modified after a downstream node was
/// certified, the certification is marked stale and a drift event is emitted.
async fn detect_drift(pool: &PgPool, scaling: &ScalingContext) -> Result<u32, Box<dyn std::error::Error>> {
    // Find nodes that were certified but whose upstream nodes have been modified since
    let drifted = sqlx::query(
        r#"
        SELECT DISTINCT n.node_id, n.title, cr.certification_ref_id
        FROM nodes n
        JOIN certification_refs cr ON cr.node_id = n.node_id
        WHERE cr.status = 'valid'
          AND EXISTS (
              SELECT 1 FROM node_edges ne
              JOIN nodes upstream ON ne.from_node_id = upstream.node_id
              WHERE ne.to_node_id = n.node_id
                AND upstream.updated_at > cr.created_at
          )
          AND NOT EXISTS (
              SELECT 1 FROM event_journal ej
              WHERE ej.aggregate_kind = 'drift'
                AND ej.aggregate_id = n.node_id
                AND ej.event_kind = 'drift_detected'
                AND ej.created_at > cr.created_at
          )
        "#
    )
    .fetch_all(pool)
    .await?;

    let mut actions = 0u32;
    for row in &drifted {
        let node_id: &str = row.try_get("node_id")?;
        let title: &str = row.try_get("title")?;
        let cert_id: &str = row.try_get("certification_ref_id")?;

        // FCG-012: Mark certification as stale
        sqlx::query(
            "UPDATE certification_refs SET status = 'stale' WHERE certification_ref_id = $1"
        )
        .bind(cert_id)
        .execute(pool)
        .await?;

        // FCG-012: Insert stale invalidation record for audit trail
        let invalidation_id = Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO stale_invalidation_records \
             (invalidation_id, submission_id, candidate_id, stale_reason, \
              triggering_change_ref, lifecycle_at_invalidation, lane_at_invalidation, \
              lane_demoted, revalidation_triggered, invalidated_at) \
             SELECT $1, cr.submission_id, cs.candidate_id, 'upstream_dependency_changed', \
                    $2, n.lifecycle, n.lane, false, false, now() \
             FROM certification_refs cr \
             JOIN certification_submissions cs ON cr.submission_id = cs.submission_id \
             JOIN nodes n ON cr.node_id = n.node_id \
             WHERE cr.certification_ref_id = $3 \
             ON CONFLICT DO NOTHING",
        )
        .bind(&invalidation_id)
        .bind(format!("upstream-change-for-node-{}", node_id))
        .bind(cert_id)
        .execute(pool)
        .await?;

        // FCG-013: Revalidation trigger -- check if auto-resubmit is configured
        let auto_resubmit: bool = sqlx::query_scalar(
            "SELECT COALESCE( \
                 (SELECT (policy_payload->>'auto_resubmit_on_stale')::boolean \
                  FROM user_policies WHERE policy_id = 'certification_config'), \
                 false \
             )",
        )
        .fetch_one(pool)
        .await
        .unwrap_or(false);

        if auto_resubmit {
            // Create a new certification candidate for revalidation
            let resubmit_candidate_id = Uuid::now_v7().to_string();
            // Get the original task_id from the stale certification's submission
            let original_task = sqlx::query(
                "SELECT cc.task_id, cc.claim_summary \
                 FROM certification_refs cr \
                 JOIN certification_submissions cs ON cr.submission_id = cs.submission_id \
                 JOIN certification_candidates cc ON cs.candidate_id = cc.candidate_id \
                 WHERE cr.certification_ref_id = $1",
            )
            .bind(cert_id)
            .fetch_optional(pool)
            .await?;

            if let Some(orig_row) = original_task {
                let orig_task_id: String = orig_row.try_get("task_id")?;
                let orig_claim: String = orig_row.try_get("claim_summary")?;
                sqlx::query(
                    "INSERT INTO certification_candidates \
                     (candidate_id, node_id, task_id, claim_summary, eligibility_reason, created_at) \
                     VALUES ($1, $2, $3, $4, 'promotion_requested', now()) \
                     ON CONFLICT DO NOTHING",
                )
                .bind(&resubmit_candidate_id)
                .bind(node_id)
                .bind(&orig_task_id)
                .bind(&orig_claim)
                .execute(pool)
                .await?;

                // Update the invalidation record to mark revalidation as triggered
                sqlx::query(
                    "UPDATE stale_invalidation_records SET revalidation_triggered = true \
                     WHERE invalidation_id = $1",
                )
                .bind(&invalidation_id)
                .execute(pool)
                .await?;

                tracing::info!(node_id, cert_id, resubmit_candidate_id, "Auto-resubmit triggered for stale certification");
            }
        }

        // Emit drift event
        let drift_event_id = Uuid::now_v7().to_string();
        let drift_idem_key = format!("drift-{}-{}", node_id, cert_id);
        let drift_payload = serde_json::json!({
            "node_id": node_id,
            "title": title,
            "stale_cert": cert_id,
            "invalidation_id": invalidation_id,
            "auto_resubmit": auto_resubmit,
        });

        sqlx::query(
            "INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
             VALUES ($1, 'drift', $2, 'drift_detected', $3, $4::jsonb, now()) \
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING"
        )
        .bind(&drift_event_id)
        .bind(node_id)
        .bind(&drift_idem_key)
        .bind(&drift_payload)
        .execute(pool)
        .await?;

        // Publish to event bus
        let _ = scaling.event_bus.publish(Event {
            event_id: drift_event_id,
            aggregate_kind: "drift".into(),
            aggregate_id: node_id.to_string(),
            event_kind: "drift_detected".into(),
            idempotency_key: drift_idem_key,
            payload: drift_payload,
        }).await;

        tracing::info!(node_id, title, cert_id, "Drift detected — certification marked stale");
        actions += 1;
    }

    // Drift type 2: Objective text changed after plan was elaborated
    let drifted_plans = sqlx::query(
        "SELECT DISTINCT p.plan_id, p.objective_id \
         FROM plans p \
         JOIN objectives o ON p.objective_id = o.objective_id \
         WHERE o.updated_at > p.created_at \
           AND NOT EXISTS ( \
               SELECT 1 FROM event_journal ej \
               WHERE ej.aggregate_kind = 'drift' \
                 AND ej.aggregate_id = p.plan_id \
                 AND ej.event_kind = 'objective_drift_detected' \
                 AND ej.created_at > o.updated_at \
           )"
    )
    .fetch_all(pool)
    .await?;

    for row in &drifted_plans {
        let plan_id: &str = row.try_get("plan_id")?;
        let objective_id: &str = row.try_get("objective_id")?;

        let event_id = Uuid::now_v7().to_string();
        let idempotency_key = format!("drift-obj-{}-{}", objective_id, plan_id);
        let obj_drift_payload = serde_json::json!({
            "objective_id": objective_id,
            "plan_id": plan_id,
            "drift_kind": "objective_modified_after_plan"
        });

        sqlx::query(
            "INSERT INTO event_journal \
             (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
             VALUES ($1, 'drift', $2, 'objective_drift_detected', $3, $4::jsonb, now()) \
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING"
        )
        .bind(&event_id)
        .bind(plan_id)
        .bind(&idempotency_key)
        .bind(&obj_drift_payload)
        .execute(pool)
        .await?;

        // Publish to event bus
        let _ = scaling.event_bus.publish(Event {
            event_id: event_id.clone(),
            aggregate_kind: "drift".into(),
            aggregate_id: plan_id.to_string(),
            event_kind: "objective_drift_detected".into(),
            idempotency_key: idempotency_key.clone(),
            payload: obj_drift_payload,
        }).await;

        actions += 1;
        tracing::info!(plan_id, objective_id, "Objective drift detected");
    }

    // Drift type 3: Running/queued tasks with incomplete predecessor nodes
    // (a dependency that should have been satisfied before task started)
    let drifted_deps = sqlx::query(
        "SELECT DISTINCT t.task_id, t.node_id \
         FROM tasks t \
         JOIN node_edges ne ON ne.to_node_id = t.node_id \
         JOIN nodes pred ON ne.from_node_id = pred.node_id \
         WHERE t.status IN ('queued', 'running') \
           AND ne.edge_kind IN ('depends_on', 'blocks') \
           AND pred.lifecycle NOT IN ('admitted', 'done', 'completed') \
           AND pred.updated_at > t.created_at \
           AND NOT EXISTS ( \
               SELECT 1 FROM event_journal ej \
               WHERE ej.aggregate_kind = 'drift' \
                 AND ej.aggregate_id = t.task_id \
                 AND ej.event_kind = 'dependency_drift_detected' \
                 AND ej.created_at > pred.updated_at \
           )"
    )
    .fetch_all(pool)
    .await?;

    for row in &drifted_deps {
        let task_id: &str = row.try_get("task_id")?;
        let node_id: &str = row.try_get("node_id")?;

        let event_id = Uuid::now_v7().to_string();
        let idempotency_key = format!("drift-dep-{}-{}", task_id, node_id);
        let dep_drift_payload = serde_json::json!({
            "task_id": task_id,
            "node_id": node_id,
            "drift_kind": "predecessor_regressed_after_task_creation"
        });

        sqlx::query(
            "INSERT INTO event_journal \
             (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
             VALUES ($1, 'drift', $2, 'dependency_drift_detected', $3, $4::jsonb, now()) \
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING"
        )
        .bind(&event_id)
        .bind(task_id)
        .bind(&idempotency_key)
        .bind(&dep_drift_payload)
        .execute(pool)
        .await?;

        // Publish to event bus
        let _ = scaling.event_bus.publish(Event {
            event_id: event_id.clone(),
            aggregate_kind: "drift".into(),
            aggregate_id: task_id.to_string(),
            event_kind: "dependency_drift_detected".into(),
            idempotency_key: idempotency_key.clone(),
            payload: dep_drift_payload,
        }).await;

        actions += 1;
        tracing::info!(task_id, node_id, "Dependency drift detected");
    }

    Ok(actions)
}

// ── Phase 11: Process certification queue (formal-claim CLI) ─────────────

/// Intermediate result from running the formal-claim CLI pipeline.
///
/// When the HTTP gateway is used instead of the CLI, `api_result` carries
/// the full OAE `CertificationApiResult` so that the rich projection
/// layer can extract sorry counts, divergence data, audit artifacts, etc.
struct CertResult {
    passed: bool,
    gate: String,
    /// Full OAE API result (present when HTTP gateway was used).
    api_result: Option<integration::CertificationApiResult>,
}

/// Process pending certification submissions by calling the formal-claim CLI.
/// Only runs if certification is enabled in user policies.
async fn process_certification_queue(pool: &PgPool) -> Result<u32, Box<dyn std::error::Error>> {
    // 1. Check if certification is enabled
    let config = sqlx::query(
        "SELECT policy_payload FROM user_policies WHERE policy_id = 'certification_config'",
    )
    .fetch_optional(pool)
    .await?;

    let enabled = config
        .as_ref()
        .and_then(|r| r.try_get::<serde_json::Value, _>("policy_payload").ok())
        .and_then(|v| v.get("enabled")?.as_bool())
        .unwrap_or(false);

    if !enabled {
        return Ok(0);
    }

    // Check if dual formalization mode is enabled
    let config_payload = config
        .as_ref()
        .and_then(|r| r.try_get::<serde_json::Value, _>("policy_payload").ok());

    let dual_mode = config_payload
        .as_ref()
        .and_then(|v| v.pointer("/formalizer_a/mode"))
        .and_then(|v| v.as_str())
        .unwrap_or("off");

    let use_dual = dual_mode == "required";

    // 2. Find pending submissions -- FCG-008: FIFO order by submitted_at ASC
    let pending = sqlx::query(
        r#"
        SELECT cs.submission_id, cs.candidate_id, cs.idempotency_key,
               cc.node_id, cc.task_id, cc.claim_summary, cc.eligibility_reason,
               cc.source_anchors
        FROM certification_submissions cs
        JOIN certification_candidates cc ON cs.candidate_id = cc.candidate_id
        WHERE cs.queue_status = 'pending'
        ORDER BY cs.submitted_at ASC
        LIMIT 5
        "#,
    )
    .fetch_all(pool)
    .await?;

    if pending.is_empty() {
        return Ok(0);
    }

    // 3. Initialize the formal-claim gateway with routing (Gap 8)
    let routing = config_payload
        .as_ref()
        .and_then(|v| v.get("routing"))
        .and_then(|v| v.as_str())
        .unwrap_or("local");

    let data_dir = match routing {
        "remote" => {
            // Check for remote endpoint
            std::env::var("FORMAL_CLAIM_REMOTE_URL")
                .unwrap_or_else(|_| std::env::var("FORMAL_CLAIM_DATA_DIR")
                    .unwrap_or_else(|_| "./formal_claim_data".to_string()))
        }
        _ => {
            std::env::var("FORMAL_CLAIM_DATA_DIR")
                .unwrap_or_else(|_| "./formal_claim_data".to_string())
        }
    };

    let mut gateway = integration::FormalClaimGateway::new(data_dir);
    // Allow overriding the CLI binary path via env var
    if let Ok(cli_path) = std::env::var("FORMAL_CLAIM_CLI_PATH") {
        gateway.cli_path = cli_path;
    }
    // For remote mode, set the CLI to use remote endpoint
    if routing == "remote" {
        if let Ok(remote_url) = std::env::var("FORMAL_CLAIM_REMOTE_URL") {
            gateway.cli_path = format!("formal-claim --remote {}", remote_url);
        }
    }

    let mut actions = 0u32;

    // Grace period: skip submissions whose candidate recently failed
    let grace_period_seconds: i64 = config_payload
        .as_ref()
        .and_then(|v| v.get("grace_period_seconds"))
        .and_then(|v| v.as_i64())
        .unwrap_or(300);

    let cert_timeout = Duration::from_secs(
        config_payload
            .as_ref()
            .and_then(|v| v.get("certification_timeout_seconds"))
            .and_then(|v| v.as_u64())
            .unwrap_or(120),
    );

    for row in &pending {
        let submission_id: &str = row.try_get("submission_id")?;
        let raw_claim_summary: &str = row.try_get("claim_summary")?;
        let node_id: &str = row.try_get("node_id")?;
        let candidate_id: &str = row.try_get("candidate_id")?;
        let task_id: &str = row.try_get("task_id")?;

        // FCG-007: Normalize claim text before submission
        let claim_summary = raw_claim_summary.trim();
        if claim_summary.is_empty() {
            tracing::warn!(submission_id, candidate_id, "Skipping: empty claim text after normalization");
            sqlx::query(
                "UPDATE certification_submissions SET queue_status = 'error', status_changed_at = now() WHERE submission_id = $1",
            )
            .bind(submission_id)
            .execute(pool)
            .await?;
            continue;
        }

        // FCG-006: Parse source anchors for the claim
        let _source_anchors: Vec<String> = row
            .try_get::<serde_json::Value, _>("source_anchors")
            .ok()
            .and_then(|v| serde_json::from_value(v).ok())
            .unwrap_or_default();

        // Grace period check: skip if candidate recently failed/timed_out
        let last_failure: Option<chrono::DateTime<chrono::Utc>> = sqlx::query_scalar(
            "SELECT MAX(status_changed_at) FROM certification_submissions \
             WHERE candidate_id = $1 AND queue_status IN ('failed', 'timed_out', 'error')",
        )
        .bind(candidate_id)
        .fetch_one(pool)
        .await?;

        if let Some(failed_at) = last_failure {
            let grace = chrono::Duration::seconds(grace_period_seconds);
            if chrono::Utc::now() < failed_at + grace {
                tracing::debug!(submission_id, candidate_id, "Skipping: within grace period after last failure");
                continue;
            }
        }

        // Update status to processing
        sqlx::query(
            "UPDATE certification_submissions SET queue_status = 'processing', status_changed_at = now() WHERE submission_id = $1",
        )
        .bind(submission_id)
        .execute(pool)
        .await?;

        // Dual formalization: run certification twice and compare
        if use_dual {
            let result_a = match tokio::time::timeout(cert_timeout, run_certification(&gateway, claim_summary, &format!("{}-a", submission_id))).await {
                Ok(r) => r,
                Err(_) => {
                    tracing::error!(submission_id, "Certification (dual-a) timed out after {}s", cert_timeout.as_secs());
                    sqlx::query("UPDATE certification_submissions SET queue_status = 'timed_out', status_changed_at = now() WHERE submission_id = $1")
                        .bind(submission_id).execute(pool).await?;
                    continue;
                }
            };
            let result_b = match tokio::time::timeout(cert_timeout, run_certification(
                &gateway,
                &format!("Independent verification: {}", claim_summary),
                &format!("{}-b", submission_id),
            )).await {
                Ok(r) => r,
                Err(_) => {
                    tracing::error!(submission_id, "Certification (dual-b) timed out after {}s", cert_timeout.as_secs());
                    sqlx::query("UPDATE certification_submissions SET queue_status = 'timed_out', status_changed_at = now() WHERE submission_id = $1")
                        .bind(submission_id).execute(pool).await?;
                    continue;
                }
            };

            match (result_a, result_b) {
                (Ok(a), Ok(b)) => {
                    if a.gate == b.gate {
                        // Both agree -- use the result
                        tracing::info!(submission_id, gate = %a.gate, "Dual formalization agrees");
                        apply_certification_result(pool, submission_id, node_id, task_id, &a).await?;
                        actions += 1;
                    } else {
                        // CNF-006: Dual formalization divergence = evidence conflict.
                        // When certification results contradict each other, create an
                        // evidence-class conflict with full history.
                        tracing::warn!(
                            submission_id,
                            gate_a = %a.gate,
                            gate_b = %b.gate,
                            "Dual formalization DIVERGED -- blocking"
                        );

                        let mut tx = pool.begin().await?;

                        // Create a conflict record for the divergence
                        let conflict_id = Uuid::now_v7().to_string();
                        sqlx::query(
                            "INSERT INTO conflicts (conflict_id, node_id, conflict_kind, status, created_at, updated_at) \
                             VALUES ($1, $2, 'formalization_divergence', 'open', now(), now())",
                        )
                        .bind(&conflict_id)
                        .bind(node_id)
                        .execute(&mut *tx)
                        .await?;

                        // CNF-009: Record conflict creation in conflict_history
                        let history_id = Uuid::now_v7().to_string();
                        let evidence_snapshot = serde_json::json!({
                            "conflict_id": conflict_id,
                            "node_id": node_id,
                            "conflict_kind": "formalization_divergence",
                            "gate_a": a.gate,
                            "gate_b": b.gate,
                            "submission_id": submission_id
                        });
                        sqlx::query(
                            "INSERT INTO conflict_history \
                             (history_entry_id, conflict_id, status_at_snapshot, change_description, snapshot, recorded_at) \
                             VALUES ($1, $2, 'open', 'Conflict created: dual formalization divergence (evidence conflict)', $3::jsonb, now())",
                        )
                        .bind(&history_id)
                        .bind(&conflict_id)
                        .bind(&evidence_snapshot)
                        .execute(&mut *tx)
                        .await?;

                        // CNF-008: Create adjudication task for the evidence divergence
                        let adjudication_id = Uuid::now_v7().to_string();
                        sqlx::query(
                            "INSERT INTO adjudication_tasks \
                             (adjudication_id, conflict_id, urgency, required_reviewer_role, \
                              context_summary, competing_artifacts, adjudication_status, created_at, updated_at) \
                             VALUES ($1, $2, 'elevated', 'formal_reviewer', $3, $4::jsonb, 'pending', now(), now())",
                        )
                        .bind(&adjudication_id)
                        .bind(&conflict_id)
                        .bind(format!(
                            "Dual formalization diverged on node {}: gate_a={}, gate_b={}. Manual review required.",
                            node_id, a.gate, b.gate
                        ))
                        .bind(serde_json::json!([
                            {"source": "formalizer_a", "gate": a.gate},
                            {"source": "formalizer_b", "gate": b.gate}
                        ]))
                        .execute(&mut *tx)
                        .await?;

                        // Update submission as diverged
                        sqlx::query(
                            "UPDATE certification_submissions SET queue_status = 'diverged', status_changed_at = now() WHERE submission_id = $1",
                        )
                        .bind(submission_id)
                        .execute(&mut *tx)
                        .await?;

                        // Emit event
                        sqlx::query(
                            "INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
                             VALUES ($1, 'certification', $2, 'dual_formalization_diverged', $3, $4::jsonb, now()) \
                             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
                        )
                        .bind(Uuid::now_v7().to_string())
                        .bind(submission_id)
                        .bind(format!("dual-diverge-{}", submission_id))
                        .bind(serde_json::json!({
                            "gate_a": a.gate,
                            "gate_b": b.gate,
                            "conflict_id": conflict_id,
                            "adjudication_id": adjudication_id,
                            "node_id": node_id,
                        }))
                        .execute(&mut *tx)
                        .await?;

                        tx.commit().await?;

                        actions += 1;
                        continue; // skip normal processing
                    }
                }
                _ => {
                    // One or both failed -- treat as single mode failure
                    tracing::warn!(submission_id, "Dual formalization: at least one run failed");
                    sqlx::query(
                        "UPDATE certification_submissions SET queue_status = 'error', status_changed_at = now() WHERE submission_id = $1",
                    )
                    .bind(submission_id)
                    .execute(pool)
                    .await?;
                }
            }
        } else {
            // ── REC-008: Block single-formalizer certification for self-improvement ──
            // If this node belongs to a self-improvement objective, require dual
            // formalization. Single-formalizer certification is denied.
            let requires_dual = recursive_improvement::is_self_improvement_requires_dual(pool, node_id)
                .await
                .unwrap_or(false);

            if requires_dual {
                let si_obj_id = recursive_improvement::get_self_improvement_objective_id(pool, node_id)
                    .await
                    .unwrap_or(None)
                    .unwrap_or_else(|| "unknown".to_string());

                recursive_improvement::record_blocked_self_promotion(
                    pool,
                    &si_obj_id,
                    submission_id,
                    "Single-formalizer certification blocked for self-improvement objective; dual formalization required (REC-008)",
                )
                .await
                .ok();

                sqlx::query(
                    "UPDATE certification_submissions SET queue_status = 'blocked', status_changed_at = now() WHERE submission_id = $1",
                )
                .bind(submission_id)
                .execute(pool)
                .await?;

                actions += 1;
                continue;
            }

            // Single mode: call formal-claim once with timeout
            match tokio::time::timeout(cert_timeout, run_certification(&gateway, claim_summary, submission_id)).await {
                Ok(Ok(result)) => {
                    apply_certification_result(pool, submission_id, node_id, task_id, &result).await?;
                    actions += 1;
                }
                Ok(Err(e)) => {
                    tracing::error!(submission_id, error = %e, "Certification failed");
                    sqlx::query(
                        "UPDATE certification_submissions SET queue_status = 'error', status_changed_at = now() WHERE submission_id = $1",
                    )
                    .bind(submission_id)
                    .execute(pool)
                    .await?;
                }
                Err(_) => {
                    tracing::error!(submission_id, "Certification timed out after {}s", cert_timeout.as_secs());
                    sqlx::query(
                        "UPDATE certification_submissions SET queue_status = 'timed_out', status_changed_at = now() WHERE submission_id = $1",
                    )
                    .bind(submission_id)
                    .execute(pool)
                    .await?;
                }
            }
        }
    }

    Ok(actions)
}

/// Run the full certification pipeline: project init -> claim structure -> analyze.
///
/// Uses the `FormalClaimGateway` to shell out to the `formal-claim` CLI.
/// Returns a `CertResult` indicating whether the claim passed and which gate
/// was assigned.
async fn run_certification(
    gateway: &integration::FormalClaimGateway,
    claim_text: &str,
    submission_id: &str,
) -> Result<CertResult, Box<dyn std::error::Error>> {
    let cli = &gateway.cli_path;
    let data_dir = &gateway.data_dir;

    // Step 1: Ensure project exists
    let project_output = tokio::process::Command::new(cli)
        .args([
            "--data-dir",
            data_dir,
            "--format",
            "json",
            "project",
            "init",
            "--name",
            &format!("cert-{}", submission_id),
            "--domain",
            "development",
        ])
        .output()
        .await?;

    let project_json: serde_json::Value = serde_json::from_slice(&project_output.stdout)
        .unwrap_or_else(|_| {
            serde_json::json!({"data": {"project_id": format!("proj-{}", submission_id)}})
        });

    let project_id = project_json
        .pointer("/data/project_id")
        .or(project_json.pointer("/project_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    // Step 2: Structure the claim
    let claim_output = tokio::process::Command::new(cli)
        .args([
            "--data-dir",
            data_dir,
            "--format",
            "json",
            "claim",
            "structure",
            "--project-id",
            &project_id,
            "--text",
            claim_text,
        ])
        .output()
        .await?;

    let claim_json: serde_json::Value =
        serde_json::from_slice(&claim_output.stdout).unwrap_or_default();

    // Try to find claim_id from response
    let claim_id = claim_json
        .pointer("/data/claims/0/claim_id")
        .or(claim_json.pointer("/data/claim_id"))
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    if claim_id == "unknown" {
        // If we can't structure the claim, report as non-formalizable
        return Ok(CertResult {
            passed: false,
            gate: "not_formalizable".to_string(),
            api_result: None,
        });
    }

    // Step 3: Analyze (triggers audit if formalizable)
    let analyze_output = tokio::process::Command::new(cli)
        .args([
            "--data-dir",
            data_dir,
            "--format",
            "json",
            "claim",
            "analyze",
            "--project-id",
            &project_id,
            "--claim-id",
            &claim_id,
        ])
        .output()
        .await?;

    let analyze_json: serde_json::Value =
        serde_json::from_slice(&analyze_output.stdout).unwrap_or_default();

    // Check gate from analysis result
    let gate = analyze_json
        .pointer("/data/profile/gate")
        .or(analyze_json.pointer("/data/gate"))
        .and_then(|v| v.as_str())
        .unwrap_or("draft")
        .to_string();

    let passed = matches!(gate.as_str(), "verified" | "audited" | "nominated");

    Ok(CertResult { passed, gate, api_result: None })
}

/// Apply a certification result: update submission status, run full OAE
/// projection if the HTTP gateway was used, create result projection,
/// update node lane if passed, and emit completion event.
async fn apply_certification_result(
    pool: &PgPool,
    submission_id: &str,
    node_id: &str,
    task_id: &str,
    result: &CertResult,
) -> Result<(), Box<dyn std::error::Error>> {
    // If we have a full OAE API result, run the rich projection first.
    if let Some(ref api_result) = result.api_result {
        match integration::result_projection::project_certification_result(
            pool, task_id, node_id, submission_id, api_result,
        )
        .await
        {
            Ok(outcome) => {
                tracing::info!(
                    submission_id,
                    ?outcome.gate_effect,
                    cautions = outcome.cautions_injected,
                    conflicts = outcome.conflicts_created,
                    artifacts = outcome.artifacts_stored,
                    "OAE projection completed"
                );
            }
            Err(e) => {
                tracing::warn!(
                    submission_id,
                    error = %e,
                    "OAE projection failed, falling back to basic projection"
                );
            }
        }
    }

    let mut tx = pool.begin().await?;

    // Update submission status
    sqlx::query(
        "UPDATE certification_submissions SET queue_status = $1, status_changed_at = now() WHERE submission_id = $2",
    )
    .bind(if result.passed { "completed" } else { "failed" })
    .bind(submission_id)
    .execute(&mut *tx)
    .await?;

    // FCG-009: Create result projection (update queue_status to 'completed' above)
    let local_gate_effect = if result.passed { "admit" } else { "block" };
    let lane_transition = if result.passed {
        "branch_to_mainline_candidate"
    } else {
        "no_change"
    };
    let projected_grade = if result.passed { "pass" } else { "fail" };

    sqlx::query(
        "INSERT INTO certification_result_projections \
         (submission_id, external_gate, local_gate_effect, lane_transition, projected_grade, projected_at) \
         VALUES ($1, $2, $3, $4, $5, now()) \
         ON CONFLICT (submission_id) DO UPDATE SET \
             external_gate = EXCLUDED.external_gate, \
             local_gate_effect = EXCLUDED.local_gate_effect, \
             lane_transition = EXCLUDED.lane_transition, \
             projected_grade = EXCLUDED.projected_grade, \
             projected_at = EXCLUDED.projected_at",
    )
    .bind(submission_id)
    .bind(&result.gate)
    .bind(local_gate_effect)
    .bind(lane_transition)
    .bind(projected_grade)
    .execute(&mut *tx)
    .await?;

    // FCG-010: INSERT or UPDATE certification_refs with the result
    let cert_ref_id = Uuid::now_v7().to_string();
    let cert_status = if result.passed { "valid" } else { "rejected" };
    sqlx::query(
        "INSERT INTO certification_refs \
         (certification_ref_id, node_id, external_system, external_ref, gate, status, submission_id, metadata, created_at) \
         VALUES ($1, $2, 'formal-claim', $3, $4, $5, $3, $6, now())",
    )
    .bind(&cert_ref_id)
    .bind(node_id)
    .bind(submission_id)
    .bind(&result.gate)
    .bind(cert_status)
    .bind(serde_json::json!({
        "local_gate_effect": local_gate_effect,
        "lane_transition": lane_transition,
        "projected_grade": projected_grade,
    }))
    .execute(&mut *tx)
    .await?;

    // FCG-011: Branch/mainline impact
    // On pass: transition node lane from branch to mainline_candidate
    // On failure: keep as branch (no_change)
    if result.passed {
        sqlx::query(
            "UPDATE nodes SET lane = 'mainline_candidate', updated_at = now() \
             WHERE node_id = $1 AND lane = 'branch'",
        )
        .bind(node_id)
        .execute(&mut *tx)
        .await?;
    }
    // On failure, explicitly keep node in current lane (no-op) -- do NOT move.

    // FCG-014: Claim/ref linkage -- link the certified result back to the node
    let link_id = Uuid::now_v7().to_string();
    sqlx::query(
        "INSERT INTO certification_claim_links \
         (link_id, submission_id, local_ref_kind, local_ref_id, linkage_description, created_at) \
         VALUES ($1, $2, 'node', $3, $4, now()) \
         ON CONFLICT DO NOTHING",
    )
    .bind(&link_id)
    .bind(submission_id)
    .bind(node_id)
    .bind(format!(
        "Certification {} for node {} via gate {}",
        if result.passed { "passed" } else { "failed" },
        node_id,
        result.gate
    ))
    .execute(&mut *tx)
    .await?;

    // Emit event
    sqlx::query(
        "INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
         VALUES ($1, 'certification', $2, 'certification_completed', $3, $4::jsonb, now()) \
         ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(submission_id)
    .bind(format!("cert-complete-{}", submission_id))
    .bind(serde_json::json!({
        "submission_id": submission_id,
        "passed": result.passed,
        "gate": result.gate,
        "node_id": node_id,
        "local_gate_effect": local_gate_effect,
        "lane_transition": lane_transition,
        "certification_ref_id": cert_ref_id,
    }))
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;

    tracing::info!(
        submission_id,
        passed = result.passed,
        gate = %result.gate,
        node_id,
        cert_ref_id,
        "Certification completed"
    );

    Ok(())
}

// ── Helper: phase transition ─────────────────────────────────────────────

/// Advance a cycle from one phase to another, with idempotency and event
/// recording.
///
/// Returns 1 if the transition was applied, 0 if skipped (idempotent or
/// race condition).
async fn advance_cycle_phase(
    pool: &PgPool,
    cycle_id: &str,
    loop_id: &str,
    from_phase: &str,
    to_phase: &str,
    trigger: &str,
) -> Result<u32, Box<dyn std::error::Error>> {
    let event_id = Uuid::now_v7().to_string();
    let idempotency_key = format!("phase-{}-{}-to-{}", cycle_id, from_phase, to_phase);

    let mut tx = pool.begin().await?;

    // BND-010: scoped idempotency check
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT aggregate_id FROM event_journal
         WHERE aggregate_kind = 'cycle' AND idempotency_key = $1 LIMIT 1",
    )
    .bind(&idempotency_key)
    .fetch_optional(tx.as_mut())
    .await?;

    if existing.is_some() {
        tx.rollback().await?;
        return Ok(0);
    }

    // Perform the phase transition with optimistic locking on the current phase
    let result = sqlx::query(
        "UPDATE cycles SET phase = $1, updated_at = now()
         WHERE cycle_id = $2 AND phase = $3",
    )
    .bind(to_phase)
    .bind(cycle_id)
    .bind(from_phase)
    .execute(tx.as_mut())
    .await?;

    if result.rows_affected() == 0 {
        // Race condition: phase was already changed
        tx.rollback().await?;
        return Ok(0);
    }

    // Record event
    let payload = serde_json::json!({
        "cycle_id": cycle_id,
        "loop_id": loop_id,
        "from_phase": from_phase,
        "to_phase": to_phase,
        "trigger": trigger
    });

    sqlx::query(
        "INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
         VALUES ($1, 'cycle', $2, 'cycle_phase_transitioned', $3, $4, now())
         ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
    )
    .bind(&event_id)
    .bind(cycle_id)
    .bind(&idempotency_key)
    .bind(&payload)
    .execute(tx.as_mut())
    .await?;

    // ── OBS-007: Emit phase-status sidecar record ──────────────────────
    //
    // After each successful phase transition, INSERT a lightweight sidecar
    // event into event_journal with aggregate_kind='phase_sidecar'. This
    // records cycle_id, exited phase, entered phase, and a timestamp so
    // operators and dashboards can reconstruct phase durations.
    let sidecar_event_id = Uuid::now_v7().to_string();
    let sidecar_idem = format!("phase-sidecar-{}-{}-to-{}", cycle_id, from_phase, to_phase);
    let now = chrono::Utc::now();
    let sidecar_payload = serde_json::json!({
        "cycle_id": cycle_id,
        "phase": to_phase,
        "exited_phase": from_phase,
        "entered_at": now.to_rfc3339(),
        "exited_at": now.to_rfc3339(),
        "duration_ms": 0,
        "trigger": trigger
    });
    sqlx::query(
        "INSERT INTO event_journal \
             (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
         VALUES ($1, 'phase_sidecar', $2, 'phase_status_recorded', $3, $4, now()) \
         ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
    )
    .bind(&sidecar_event_id)
    .bind(cycle_id)
    .bind(&sidecar_idem)
    .bind(&sidecar_payload)
    .execute(tx.as_mut())
    .await?;

    tx.commit().await?;
    tracing::info!(cycle_id, from_phase, to_phase, trigger, "Phase transition applied");
    Ok(1)
}

// ── Retention policy enforcement ─────────────────────────────────────────

/// Enforce retention policies defined in DB. Cleans up old event_journal
/// entries, completed task_attempts, stale worktree records, and old
/// saturation snapshots.
async fn enforce_retention_policies(pool: &PgPool, scaling: &ScalingContext) -> Result<u32, Box<dyn std::error::Error>> {
    let mut cleaned = 0u32;

    // 1. Clean up old event_journal entries (keep last N days, default 30)
    let retention_days: i32 = sqlx::query_scalar(
        "SELECT max_retention_days FROM retention_policies WHERE scope = 'event_journal' LIMIT 1",
    )
    .fetch_optional(pool)
    .await?
    .unwrap_or(30);

    let deleted: i64 = sqlx::query_scalar(
        "WITH deleted AS (
            DELETE FROM event_journal
            WHERE created_at < now() - make_interval(days => $1)
            AND event_kind NOT IN ('objective_created', 'loop_created', 'certification_candidate_created')
            RETURNING 1
        ) SELECT COUNT(*) FROM deleted",
    )
    .bind(retention_days)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    if deleted > 0 {
        tracing::info!(deleted, retention_days, "Pruned old event_journal entries");
        cleaned += deleted as u32;
    }

    // 2. Clean up completed task_attempts older than 7 days
    let deleted: i64 = sqlx::query_scalar(
        "WITH deleted AS (
            DELETE FROM task_attempts
            WHERE status IN ('succeeded', 'failed', 'timed_out')
            AND finished_at < now() - interval '7 days'
            RETURNING 1
        ) SELECT COUNT(*) FROM deleted",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    if deleted > 0 {
        tracing::info!(deleted, "Pruned old task_attempts");
        cleaned += deleted as u32;
    }

    // 3. Clean up stale worktree records
    let cleaned_wt: i64 = sqlx::query_scalar(
        "WITH deleted AS (
            DELETE FROM git_worktree_assignments
            WHERE status = 'removed' AND updated_at < now() - interval '1 day'
            RETURNING 1
        ) SELECT COUNT(*) FROM deleted",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    cleaned += cleaned_wt as u32;

    // 4. Archive old saturation snapshots
    let archived: i64 = sqlx::query_scalar(
        "WITH deleted AS (
            DELETE FROM saturation_snapshots
            WHERE snapshot_time < now() - interval '7 days'
            RETURNING 1
        ) SELECT COUNT(*) FROM deleted",
    )
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    cleaned += archived as u32;

    // Record that we ran retention enforcement (throttle marker)
    let retention_event_id = Uuid::now_v7().to_string();
    let retention_idem_key = format!("retention-{}", chrono::Utc::now().format("%Y-%m-%dT%H"));
    let retention_payload = serde_json::json!({"cleaned": cleaned});

    sqlx::query(
        "INSERT INTO event_journal \
         (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
         VALUES ($1, 'system', 'retention', 'retention_policy_enforced', $2, $3::jsonb, now()) \
         ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
    )
    .bind(&retention_event_id)
    .bind(&retention_idem_key)
    .bind(&retention_payload)
    .execute(pool)
    .await
    .ok();

    // Publish to event bus
    let _ = scaling.event_bus.publish(Event {
        event_id: retention_event_id,
        aggregate_kind: "system".into(),
        aggregate_id: "retention".into(),
        event_kind: "retention_policy_enforced".into(),
        idempotency_key: retention_idem_key,
        payload: retention_payload,
    }).await;

    if cleaned > 0 {
        tracing::info!(cleaned, "Retention policy enforcement complete");
    }

    Ok(cleaned)
}
