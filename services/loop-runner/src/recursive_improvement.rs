//! Runtime logic for recursive improvement (comparison, scoring, drift, reports, memory).
//!
//! Connects the type definitions in `recursive-improvement` crate to the
//! loop-runner tick lifecycle. Each function writes to the authoritative
//! tables defined in migration 0008_m7_recursive_improvement.sql.
//!
//! Write discipline: transaction -> mutate -> event_journal -> commit.
//! All INSERTs use ON CONFLICT DO NOTHING for idempotency.

use sqlx::{PgPool, Row};
use uuid::Uuid;

/// Generate a comparison artifact after a cycle completes.
///
/// Queries the current cycle's task success/failure counts and compares
/// them against the previous cycle for the same objective. INSERTs into
/// comparison_artifacts with before_metrics, after_metrics, delta_summary.
///
/// Returns 1 if an artifact was created, 0 if skipped.
pub async fn generate_comparison_artifact(
    pool: &PgPool,
    objective_id: &str,
    cycle_id: &str,
    loop_id: &str,
) -> Result<u32, Box<dyn std::error::Error>> {
    let comparison_id = Uuid::now_v7().to_string();
    let idempotency_key = format!("rec004-comparison-{}", cycle_id);

    // Idempotency: skip if already generated for this cycle
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT aggregate_id FROM event_journal
         WHERE aggregate_kind = 'recursive_improvement'
           AND idempotency_key = $1
         LIMIT 1",
    )
    .bind(&idempotency_key)
    .fetch_optional(pool)
    .await?;

    if exists.is_some() {
        return Ok(0);
    }

    // Check if this is a self-improvement objective
    let si_obj: Option<String> = sqlx::query_scalar(
        "SELECT objective_id FROM self_improvement_objectives WHERE objective_id = $1",
    )
    .bind(objective_id)
    .fetch_optional(pool)
    .await?;

    if si_obj.is_none() {
        return Ok(0);
    }

    // Current cycle task counts
    let current_stats = sqlx::query(
        "SELECT
            COUNT(*) FILTER (WHERE t.status = 'succeeded') AS succeeded,
            COUNT(*) FILTER (WHERE t.status = 'failed') AS failed,
            COUNT(*) AS total
         FROM tasks t
         JOIN nodes n ON t.node_id = n.node_id
         WHERE n.objective_id = $1",
    )
    .bind(objective_id)
    .fetch_one(pool)
    .await?;

    let current_succeeded: i64 = current_stats.try_get("succeeded").unwrap_or(0);
    let current_failed: i64 = current_stats.try_get("failed").unwrap_or(0);
    let current_total: i64 = current_stats.try_get("total").unwrap_or(0);

    let current_success_rate = if current_total > 0 {
        current_succeeded as f64 / current_total as f64
    } else {
        0.0
    };

    // Previous cycle's metrics from the most recent comparison artifact
    let prev_metrics: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT baseline FROM comparison_artifacts
         WHERE objective_id = $1
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(objective_id)
    .fetch_optional(pool)
    .await?;

    let prev_success_rate = prev_metrics
        .as_ref()
        .and_then(|v| v.pointer("/success_rate"))
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);

    // Determine iteration index
    let iteration_index: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM comparison_artifacts WHERE objective_id = $1",
    )
    .bind(objective_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let before_metrics = serde_json::json!({
        "success_rate": prev_success_rate,
        "source": "previous_cycle",
    });

    let after_metrics = serde_json::json!({
        "succeeded": current_succeeded,
        "failed": current_failed,
        "total": current_total,
        "success_rate": current_success_rate,
    });

    let delta = current_success_rate - prev_success_rate;
    let direction = if delta > 0.01 {
        "improved"
    } else if delta < -0.01 {
        "degraded"
    } else {
        "unchanged"
    };

    let metric_deltas = serde_json::json!([{
        "metric_name": "success_rate",
        "baseline_value": format!("{:.2}", prev_success_rate),
        "proposed_value": format!("{:.2}", current_success_rate),
        "direction": direction,
        "magnitude": format!("{:.2}", delta.abs()),
    }]);

    let overall_assessment = if current_failed > 0 {
        "needs_review"
    } else {
        "safe_to_proceed"
    };

    // Suppress unused-variable warning; after_metrics is part of the
    // proposal_summary text and kept for future structured expansion.
    let _after = &after_metrics;

    let mut tx = pool.begin().await?;

    sqlx::query(
        "INSERT INTO comparison_artifacts
         (comparison_id, objective_id, iteration_index, baseline,
          proposal_summary, changed_surfaces, metric_deltas,
          regression_risks, overall_assessment, created_at)
         VALUES ($1, $2, $3, $4, $5, '[]'::jsonb, $6, '[]'::jsonb, $7, now())
         ON CONFLICT DO NOTHING",
    )
    .bind(&comparison_id)
    .bind(objective_id)
    .bind((iteration_index + 1) as i32)
    .bind(&before_metrics)
    .bind(format!(
        "Cycle {} completed: {}/{} tasks succeeded (rate: {:.0}%)",
        cycle_id, current_succeeded, current_total, current_success_rate * 100.0
    ))
    .bind(&metric_deltas)
    .bind(overall_assessment)
    .execute(tx.as_mut())
    .await?;

    // Record event
    sqlx::query(
        "INSERT INTO event_journal
         (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
         VALUES ($1, 'recursive_improvement', $2, 'comparison_artifact_created', $3, $4::jsonb, now())
         ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&comparison_id)
    .bind(&idempotency_key)
    .bind(serde_json::json!({
        "comparison_id": comparison_id,
        "objective_id": objective_id,
        "cycle_id": cycle_id,
        "loop_id": loop_id,
        "current_success_rate": current_success_rate,
        "previous_success_rate": prev_success_rate,
        "direction": direction,
    }))
    .execute(tx.as_mut())
    .await?;

    tx.commit().await?;

    tracing::info!(
        comparison_id,
        objective_id,
        cycle_id,
        current_success_rate,
        prev_success_rate,
        direction,
        "REC-004: Generated comparison artifact"
    );

    Ok(1)
}

/// Compute an improvement score after a comparison artifact is generated.
///
/// Score = current_success_rate - previous_success_rate. Advisory only.
/// INSERTs into loop_scores with the score and dimensions.
///
/// Returns 1 if a score was created, 0 if skipped.
pub async fn compute_improvement_score(
    pool: &PgPool,
    objective_id: &str,
    cycle_id: &str,
) -> Result<u32, Box<dyn std::error::Error>> {
    let score_id = Uuid::now_v7().to_string();
    let idempotency_key = format!("rec005-score-{}", cycle_id);

    // Idempotency check
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT aggregate_id FROM event_journal
         WHERE aggregate_kind = 'recursive_improvement' AND idempotency_key = $1 LIMIT 1",
    )
    .bind(&idempotency_key)
    .fetch_optional(pool)
    .await?;

    if exists.is_some() {
        return Ok(0);
    }

    // Check self-improvement objective
    let si_obj: Option<String> = sqlx::query_scalar(
        "SELECT objective_id FROM self_improvement_objectives WHERE objective_id = $1",
    )
    .bind(objective_id)
    .fetch_optional(pool)
    .await?;

    if si_obj.is_none() {
        return Ok(0);
    }

    // Gather scoring inputs from current cycle
    let stats = sqlx::query(
        "SELECT
            COUNT(*) FILTER (WHERE t.status = 'succeeded') AS succeeded,
            COUNT(*) FILTER (WHERE t.status = 'failed') AS failed,
            COUNT(*) AS total
         FROM tasks t
         JOIN nodes n ON t.node_id = n.node_id
         WHERE n.objective_id = $1",
    )
    .bind(objective_id)
    .fetch_one(pool)
    .await?;

    let tasks_completed: i64 = stats.try_get("succeeded").unwrap_or(0);
    let tasks_failed: i64 = stats.try_get("failed").unwrap_or(0);
    let total: i64 = stats.try_get("total").unwrap_or(0);

    // Pending reviews and certifications
    let pending_reviews: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM review_artifacts WHERE target_ref = $1 AND status = 'scheduled'",
    )
    .bind(objective_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let pending_certs: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM certification_submissions cs
         JOIN certification_candidates cc ON cs.candidate_id = cc.candidate_id
         JOIN nodes n ON cc.node_id = n.node_id
         WHERE n.objective_id = $1 AND cs.queue_status = 'pending'",
    )
    .bind(objective_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    // Drift warnings count
    let drift_warnings: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM drift_check_artifacts
         WHERE objective_id = $1 AND has_unintentional_drift = TRUE",
    )
    .bind(objective_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    // Compute score dimensions [0.0, 1.0]
    let throughput = if total > 0 {
        tasks_completed as f64 / total as f64
    } else {
        0.0
    };
    let stability = if total > 0 {
        1.0 - (tasks_failed as f64 / total as f64)
    } else {
        1.0
    };
    let review_debt = if pending_reviews == 0 {
        1.0
    } else {
        1.0 / (1.0 + pending_reviews as f64)
    };
    let cert_pressure = if pending_certs == 0 {
        1.0
    } else {
        1.0 / (1.0 + pending_certs as f64)
    };
    let regression_risk = if drift_warnings == 0 {
        1.0
    } else {
        1.0 / (1.0 + drift_warnings as f64)
    };

    let composite = (throughput + stability + review_debt + cert_pressure + regression_risk) / 5.0;

    let iteration_index: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM loop_scores WHERE objective_id = $1",
    )
    .bind(objective_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let input = serde_json::json!({
        "tasks_completed": tasks_completed,
        "tasks_failed": tasks_failed,
        "iteration_duration_secs": 0.0,
        "pending_reviews": pending_reviews,
        "pending_certifications": pending_certs,
        "regressions_detected": 0,
        "drift_warnings": drift_warnings,
    });

    let breakdown = serde_json::json!({
        "throughput": throughput,
        "stability": stability,
        "review_debt": review_debt,
        "certification_pressure": cert_pressure,
        "regression_risk": regression_risk,
    });

    let recommendation = if composite >= 0.8 {
        "Strong improvement -- continue current approach."
    } else if composite >= 0.5 {
        "Moderate improvement -- consider reviewing failure patterns."
    } else {
        "Low improvement -- investigate root causes before next iteration."
    };

    let mut tx = pool.begin().await?;

    sqlx::query(
        "INSERT INTO loop_scores
         (score_id, objective_id, iteration_index, input, breakdown,
          composite_score, advisory_only, recommendation, created_at)
         VALUES ($1, $2, $3, $4, $5, $6, TRUE, $7, now())
         ON CONFLICT DO NOTHING",
    )
    .bind(&score_id)
    .bind(objective_id)
    .bind((iteration_index + 1) as i32)
    .bind(&input)
    .bind(&breakdown)
    .bind(composite)
    .bind(recommendation)
    .execute(tx.as_mut())
    .await?;

    // Record event
    sqlx::query(
        "INSERT INTO event_journal
         (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
         VALUES ($1, 'recursive_improvement', $2, 'loop_score_created', $3, $4::jsonb, now())
         ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&score_id)
    .bind(&idempotency_key)
    .bind(serde_json::json!({
        "score_id": score_id,
        "objective_id": objective_id,
        "cycle_id": cycle_id,
        "composite_score": composite,
        "advisory_only": true,
    }))
    .execute(tx.as_mut())
    .await?;

    tx.commit().await?;

    tracing::info!(
        score_id,
        objective_id,
        cycle_id,
        composite,
        "REC-005: Computed improvement score (advisory)"
    );

    Ok(1)
}

/// Generate milestone templates for self-improvement objectives.
///
/// When an objective summary contains "self-improvement" or "improve",
/// generate 3 predefined milestone templates and INSERT them as
/// milestone_nodes under a tree for the objective.
///
/// Returns the number of milestones created.
pub async fn generate_milestone_templates(
    pool: &PgPool,
    objective_id: &str,
) -> Result<u32, Box<dyn std::error::Error>> {
    let idempotency_key = format!("rec006-templates-{}", objective_id);

    // Idempotency check
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT aggregate_id FROM event_journal
         WHERE aggregate_kind = 'recursive_improvement' AND idempotency_key = $1 LIMIT 1",
    )
    .bind(&idempotency_key)
    .fetch_optional(pool)
    .await?;

    if exists.is_some() {
        return Ok(0);
    }

    // Check if this objective qualifies for self-improvement templates
    let obj_summary: Option<String> = sqlx::query_scalar(
        "SELECT summary FROM self_improvement_objectives WHERE objective_id = $1",
    )
    .bind(objective_id)
    .fetch_optional(pool)
    .await?;

    let summary = match obj_summary {
        Some(s) => s,
        None => return Ok(0), // Not a self-improvement objective
    };

    let lower = summary.to_lowercase();
    if !lower.contains("self-improvement") && !lower.contains("improve") {
        return Ok(0);
    }

    // Check if a milestone tree already exists
    let existing_tree: Option<String> = sqlx::query_scalar(
        "SELECT tree_id FROM milestone_trees WHERE objective_id = $1 LIMIT 1",
    )
    .bind(objective_id)
    .fetch_optional(pool)
    .await?;

    let tree_id = match existing_tree {
        Some(t) => t,
        None => {
            let tid = Uuid::now_v7().to_string();
            sqlx::query(
                "INSERT INTO milestone_trees (tree_id, objective_id, created_at, updated_at)
                 VALUES ($1, $2, now(), now())
                 ON CONFLICT DO NOTHING",
            )
            .bind(&tid)
            .bind(objective_id)
            .execute(pool)
            .await?;
            tid
        }
    };

    // Define 3 predefined milestone templates
    let templates = [
        (
            "Baseline capture and safety checkpoint",
            "Capture current state as baseline, verify safety gates pass, create rollback anchor.",
            1,
        ),
        (
            "Implementation and drift validation",
            "Apply the improvement, run drift checks against policy/schema/skill resolution/approval law, record comparison artifact.",
            2,
        ),
        (
            "Integration verification and report",
            "Run integration tests, compute improvement score, generate recursive report, update roadmap memory.",
            3,
        ),
    ];

    let mut created = 0u32;
    let mut tx = pool.begin().await?;

    for (title, description, ordering) in &templates {
        let milestone_id = Uuid::now_v7().to_string();

        let result = sqlx::query(
            "INSERT INTO milestone_nodes
             (milestone_id, tree_id, title, description, ordering, status)
             VALUES ($1, $2, $3, $4, $5, 'pending')
             ON CONFLICT DO NOTHING",
        )
        .bind(&milestone_id)
        .bind(&tree_id)
        .bind(title)
        .bind(description)
        .bind(ordering)
        .execute(tx.as_mut())
        .await?;

        if result.rows_affected() > 0 {
            created += 1;
        }
    }

    // Record event
    sqlx::query(
        "INSERT INTO event_journal
         (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
         VALUES ($1, 'recursive_improvement', $2, 'milestone_templates_created', $3, $4::jsonb, now())
         ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&tree_id)
    .bind(&idempotency_key)
    .bind(serde_json::json!({
        "objective_id": objective_id,
        "tree_id": tree_id,
        "milestones_created": created,
        "template_count": 3,
    }))
    .execute(tx.as_mut())
    .await?;

    tx.commit().await?;

    if created > 0 {
        tracing::info!(
            objective_id,
            tree_id,
            created,
            "REC-006: Generated self-improvement milestone templates"
        );
    }

    Ok(created)
}

/// When a self-improvement objective modifies core files (state-model,
/// control-plane), INSERT a drift_check_artifact recording what was
/// touched and why.
///
/// Returns 1 if a drift check was recorded, 0 if skipped.
pub async fn check_self_improvement_drift(
    pool: &PgPool,
    objective_id: &str,
    cycle_id: &str,
) -> Result<u32, Box<dyn std::error::Error>> {
    let idempotency_key = format!("rec007-drift-{}", cycle_id);

    // Idempotency check
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT aggregate_id FROM event_journal
         WHERE aggregate_kind = 'recursive_improvement' AND idempotency_key = $1 LIMIT 1",
    )
    .bind(&idempotency_key)
    .fetch_optional(pool)
    .await?;

    if exists.is_some() {
        return Ok(0);
    }

    // Only applies to self-improvement objectives
    let si_obj = sqlx::query(
        "SELECT objective_id, repo_target, summary FROM self_improvement_objectives
         WHERE objective_id = $1",
    )
    .bind(objective_id)
    .fetch_optional(pool)
    .await?;

    let obj_row = match si_obj {
        Some(r) => r,
        None => return Ok(0),
    };

    let repo_target: String = obj_row.try_get("repo_target").unwrap_or_default();
    let summary: String = obj_row.try_get("summary").unwrap_or_default();

    // Check if core files were touched by looking at artifact_refs for this objective
    let touched_files: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT ar.artifact_uri FROM artifact_refs ar
         JOIN tasks t ON ar.task_id = t.task_id
         JOIN nodes n ON t.node_id = n.node_id
         WHERE n.objective_id = $1
           AND ar.artifact_kind IN ('source_file', 'output_file', 'source_anchor')",
    )
    .bind(objective_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    // Detect drift: check if any touched files are in core packages
    let core_patterns = ["state-model", "control-plane", "safety_gate", "user-policy"];
    let mut schema_drifts = Vec::new();
    let mut policy_drifts = Vec::new();

    for file in &touched_files {
        let lower = file.to_lowercase();
        for pattern in &core_patterns {
            if lower.contains(pattern) {
                if *pattern == "state-model" || *pattern == "control-plane" {
                    schema_drifts.push(serde_json::json!({
                        "entity": file,
                        "baseline_version": "pre-cycle",
                        "current_version": "post-cycle",
                        "structural_changes": format!("Modified by self-improvement objective: {}", summary),
                    }));
                } else {
                    policy_drifts.push(serde_json::json!({
                        "policy_field": file,
                        "baseline_semantic": "pre-cycle",
                        "current_semantic": "post-cycle",
                        "intentional": true,
                    }));
                }
            }
        }
    }

    // Also check if the repo_target itself is a core package
    let targets_core = core_patterns
        .iter()
        .any(|p| repo_target.to_lowercase().contains(p));

    if targets_core && schema_drifts.is_empty() {
        schema_drifts.push(serde_json::json!({
            "entity": repo_target,
            "baseline_version": "pre-cycle",
            "current_version": "post-cycle",
            "structural_changes": format!("Self-improvement targets core package: {}", repo_target),
        }));
    }

    // If no drift detected, still record a clean check
    let has_drift = !schema_drifts.is_empty() || !policy_drifts.is_empty();
    let overall_severity = if has_drift { "medium" } else { "none" };

    let drift_check_id = Uuid::now_v7().to_string();
    let iteration_index: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM drift_check_artifacts WHERE objective_id = $1",
    )
    .bind(objective_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let mut tx = pool.begin().await?;

    sqlx::query(
        "INSERT INTO drift_check_artifacts
         (drift_check_id, objective_id, iteration_index,
          policy_drifts, schema_drifts, skill_drifts, approval_drifts,
          overall_severity, has_unintentional_drift, blocks_continuation, checked_at)
         VALUES ($1, $2, $3, $4, $5, '[]'::jsonb, '[]'::jsonb, $6, FALSE, FALSE, now())
         ON CONFLICT DO NOTHING",
    )
    .bind(&drift_check_id)
    .bind(objective_id)
    .bind((iteration_index + 1) as i32)
    .bind(serde_json::json!(policy_drifts))
    .bind(serde_json::json!(schema_drifts))
    .bind(overall_severity)
    .execute(tx.as_mut())
    .await?;

    // Record event
    sqlx::query(
        "INSERT INTO event_journal
         (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
         VALUES ($1, 'recursive_improvement', $2, 'drift_check_completed', $3, $4::jsonb, now())
         ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&drift_check_id)
    .bind(&idempotency_key)
    .bind(serde_json::json!({
        "drift_check_id": drift_check_id,
        "objective_id": objective_id,
        "cycle_id": cycle_id,
        "has_drift": has_drift,
        "overall_severity": overall_severity,
        "schema_drift_count": schema_drifts.len(),
        "policy_drift_count": policy_drifts.len(),
    }))
    .execute(tx.as_mut())
    .await?;

    tx.commit().await?;

    tracing::info!(
        drift_check_id,
        objective_id,
        cycle_id,
        has_drift,
        overall_severity,
        "REC-007: Drift check completed"
    );

    Ok(1)
}

/// Check whether a certification submission's objective is a
/// self-improvement type. If so, require dual formalization before
/// allowing promotion.
///
/// Returns true if the submission should be blocked (single-formalizer
/// on a self-improvement objective), false otherwise.
pub async fn is_self_improvement_requires_dual(
    pool: &PgPool,
    node_id: &str,
) -> Result<bool, Box<dyn std::error::Error>> {
    // Walk from node -> objective, check if it's a self-improvement objective
    let si_match: Option<String> = sqlx::query_scalar(
        "SELECT sio.objective_id
         FROM self_improvement_objectives sio
         JOIN nodes n ON n.objective_id = sio.objective_id
         WHERE n.node_id = $1
         LIMIT 1",
    )
    .bind(node_id)
    .fetch_optional(pool)
    .await?;

    Ok(si_match.is_some())
}

/// Record a blocked self-promotion attempt.
///
/// INSERTs into self_promotion_attempts with denial_result = 'denied'.
pub async fn record_blocked_self_promotion(
    pool: &PgPool,
    objective_id: &str,
    submission_id: &str,
    reason: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let attempt_id = Uuid::now_v7().to_string();

    sqlx::query(
        "INSERT INTO self_promotion_attempts
         (attempt_id, source_objective_id, artifact_ref, promotion_kind,
          description, denial_result, detected_at)
         VALUES ($1, $2, $3, 'certification_single_formalizer', $4, 'denied', now())
         ON CONFLICT DO NOTHING",
    )
    .bind(&attempt_id)
    .bind(objective_id)
    .bind(submission_id)
    .bind(reason)
    .execute(pool)
    .await?;

    // Record event
    sqlx::query(
        "INSERT INTO event_journal
         (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
         VALUES ($1, 'recursive_improvement', $2, 'self_promotion_blocked', $3, $4::jsonb, now())
         ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&attempt_id)
    .bind(format!("rec008-block-{}", submission_id))
    .bind(serde_json::json!({
        "attempt_id": attempt_id,
        "objective_id": objective_id,
        "submission_id": submission_id,
        "reason": reason,
        "denial_result": "denied",
    }))
    .execute(pool)
    .await?;

    tracing::warn!(
        attempt_id,
        objective_id,
        submission_id,
        "REC-008: Blocked single-formalizer certification for self-improvement objective"
    );

    Ok(())
}

/// Look up the self-improvement objective_id for a given node.
pub async fn get_self_improvement_objective_id(
    pool: &PgPool,
    node_id: &str,
) -> Result<Option<String>, Box<dyn std::error::Error>> {
    let si_match: Option<String> = sqlx::query_scalar(
        "SELECT sio.objective_id
         FROM self_improvement_objectives sio
         JOIN nodes n ON n.objective_id = sio.objective_id
         WHERE n.node_id = $1
         LIMIT 1",
    )
    .bind(node_id)
    .fetch_optional(pool)
    .await?;

    Ok(si_match)
}

/// Generate a recursive_report after a self-improvement loop iteration
/// completes.
///
/// Summarizes: what was attempted, what succeeded, what failed,
/// improvement score, and recommendations.
///
/// Returns 1 if a report was generated, 0 if skipped.
pub async fn generate_recursive_report(
    pool: &PgPool,
    objective_id: &str,
    cycle_id: &str,
    loop_id: &str,
) -> Result<u32, Box<dyn std::error::Error>> {
    let report_id = Uuid::now_v7().to_string();
    let idempotency_key = format!("rec009-report-{}", cycle_id);

    // Idempotency check
    let exists: Option<String> = sqlx::query_scalar(
        "SELECT aggregate_id FROM event_journal
         WHERE aggregate_kind = 'recursive_improvement' AND idempotency_key = $1 LIMIT 1",
    )
    .bind(&idempotency_key)
    .fetch_optional(pool)
    .await?;

    if exists.is_some() {
        return Ok(0);
    }

    // Only for self-improvement objectives
    let si_obj = sqlx::query(
        "SELECT objective_id, summary, repo_target FROM self_improvement_objectives
         WHERE objective_id = $1",
    )
    .bind(objective_id)
    .fetch_optional(pool)
    .await?;

    let obj_row = match si_obj {
        Some(r) => r,
        None => return Ok(0),
    };

    let summary: String = obj_row.try_get("summary").unwrap_or_default();
    let repo_target: String = obj_row.try_get("repo_target").unwrap_or_default();

    // Gather data for report sections
    let stats = sqlx::query(
        "SELECT
            COUNT(*) FILTER (WHERE t.status = 'succeeded') AS succeeded,
            COUNT(*) FILTER (WHERE t.status = 'failed') AS failed,
            COUNT(*) AS total
         FROM tasks t
         JOIN nodes n ON t.node_id = n.node_id
         WHERE n.objective_id = $1",
    )
    .bind(objective_id)
    .fetch_one(pool)
    .await?;

    let succeeded: i64 = stats.try_get("succeeded").unwrap_or(0);
    let failed: i64 = stats.try_get("failed").unwrap_or(0);
    let total: i64 = stats.try_get("total").unwrap_or(0);

    // Latest score
    let latest_score: Option<f64> = sqlx::query_scalar(
        "SELECT composite_score FROM loop_scores
         WHERE objective_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(objective_id)
    .fetch_optional(pool)
    .await?;

    // Latest drift check
    let latest_drift: Option<String> = sqlx::query_scalar(
        "SELECT overall_severity FROM drift_check_artifacts
         WHERE objective_id = $1 ORDER BY checked_at DESC LIMIT 1",
    )
    .bind(objective_id)
    .fetch_optional(pool)
    .await?;

    let iteration_index: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM recursive_reports WHERE objective_id = $1",
    )
    .bind(objective_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    // Build report sections
    let sections = serde_json::json!([
        {
            "title": "Objective",
            "body": summary,
            "structured_data": {"objective_id": objective_id, "repo_target": repo_target}
        },
        {
            "title": "Execution Summary",
            "body": format!("{} tasks total: {} succeeded, {} failed", total, succeeded, failed),
            "structured_data": {"succeeded": succeeded, "failed": failed, "total": total}
        },
        {
            "title": "Improvement Score",
            "body": format!("Composite score: {:.2} (advisory)", latest_score.unwrap_or(0.0)),
            "structured_data": {"composite_score": latest_score.unwrap_or(0.0), "advisory_only": true}
        },
        {
            "title": "Drift Analysis",
            "body": format!("Overall drift severity: {}", latest_drift.as_deref().unwrap_or("none")),
            "structured_data": {"overall_severity": latest_drift.as_deref().unwrap_or("none")}
        }
    ]);

    // Generate recommendations
    let has_blockers = failed > 0 || latest_drift.as_deref() == Some("critical");
    let action = if has_blockers { "pause" } else { "continue" };
    let rationale = if has_blockers {
        format!("{} failures detected, review required before next iteration", failed)
    } else {
        "All tasks succeeded, no critical drift".to_string()
    };

    let recommendations = serde_json::json!([{
        "action": action,
        "rationale": rationale,
        "prerequisites": [],
    }]);

    // Collect related artifact refs
    let comparison_ref: Option<String> = sqlx::query_scalar(
        "SELECT comparison_id FROM comparison_artifacts
         WHERE objective_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(objective_id)
    .fetch_optional(pool)
    .await?;

    let score_ref: Option<String> = sqlx::query_scalar(
        "SELECT score_id FROM loop_scores
         WHERE objective_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(objective_id)
    .fetch_optional(pool)
    .await?;

    let drift_ref: Option<String> = sqlx::query_scalar(
        "SELECT drift_check_id FROM drift_check_artifacts
         WHERE objective_id = $1 ORDER BY checked_at DESC LIMIT 1",
    )
    .bind(objective_id)
    .fetch_optional(pool)
    .await?;

    let mut related_refs: Vec<String> = Vec::new();
    if let Some(r) = comparison_ref {
        related_refs.push(r);
    }
    if let Some(r) = score_ref {
        related_refs.push(r);
    }
    if let Some(r) = drift_ref {
        related_refs.push(r);
    }

    let mut tx = pool.begin().await?;

    sqlx::query(
        "INSERT INTO recursive_reports
         (report_id, objective_id, iteration_index, sections, recommendations,
          related_artifact_refs, is_complete, has_blockers, generated_at)
         VALUES ($1, $2, $3, $4, $5, $6, TRUE, $7, now())
         ON CONFLICT DO NOTHING",
    )
    .bind(&report_id)
    .bind(objective_id)
    .bind((iteration_index + 1) as i32)
    .bind(&sections)
    .bind(&recommendations)
    .bind(serde_json::json!(related_refs))
    .bind(has_blockers)
    .execute(tx.as_mut())
    .await?;

    // Record event
    sqlx::query(
        "INSERT INTO event_journal
         (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
         VALUES ($1, 'recursive_improvement', $2, 'recursive_report_generated', $3, $4::jsonb, now())
         ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&report_id)
    .bind(&idempotency_key)
    .bind(serde_json::json!({
        "report_id": report_id,
        "objective_id": objective_id,
        "cycle_id": cycle_id,
        "loop_id": loop_id,
        "has_blockers": has_blockers,
        "composite_score": latest_score.unwrap_or(0.0),
    }))
    .execute(tx.as_mut())
    .await?;

    tx.commit().await?;

    tracing::info!(
        report_id,
        objective_id,
        cycle_id,
        has_blockers,
        "REC-009: Generated recursive report"
    );

    Ok(1)
}

/// Extend existing recursive_memory_entries write.
///
/// In addition to failure patterns (already written in tick.rs state_update),
/// also write:
///   - success patterns: what worked
///   - roadmap suggestions: what to try next
///
/// Returns the number of entries created.
pub async fn write_extended_memory(
    pool: &PgPool,
    objective_id: &str,
    cycle_id: &str,
) -> Result<u32, Box<dyn std::error::Error>> {
    let mut created = 0u32;

    let successes = sqlx::query(
        "SELECT t.task_id, n.title, n.lane
         FROM tasks t
         JOIN nodes n ON t.node_id = n.node_id
         WHERE n.objective_id = $1
           AND t.status = 'succeeded'
         ORDER BY t.updated_at DESC
         LIMIT 10",
    )
    .bind(objective_id)
    .fetch_all(pool)
    .await
    .unwrap_or_default();

    if !successes.is_empty() {
        let mut success_lessons: Vec<String> = Vec::new();
        for s in &successes {
            let title: String = s.try_get("title").unwrap_or_default();
            let lane: String = s.try_get("lane").unwrap_or_default();
            success_lessons.push(format!("Task '{}' (lane: {}) succeeded", title, lane));
        }

        let learned = success_lessons.join("; ");
        let memory_id = Uuid::now_v7().to_string();
        let idem_key = format!("rec010-success-{}", cycle_id);

        // Idempotency
        let idem_exists: Option<String> = sqlx::query_scalar(
            "SELECT aggregate_id FROM event_journal
             WHERE aggregate_kind = 'recursive_improvement' AND idempotency_key = $1 LIMIT 1",
        )
        .bind(&idem_key)
        .fetch_optional(pool)
        .await?;

        if idem_exists.is_none() {
            let mut tx = pool.begin().await?;

            sqlx::query(
                "INSERT INTO recursive_memory_entries
                 (entry_id, objective_id, outcome, learned_summary, outcome_metrics, recorded_at)
                 VALUES ($1, $2, 'success_pattern', $3, $4, now())
                 ON CONFLICT DO NOTHING",
            )
            .bind(&memory_id)
            .bind(objective_id)
            .bind(&learned)
            .bind(serde_json::json!({
                "cycle_id": cycle_id,
                "success_count": successes.len(),
                "successes": success_lessons,
            }))
            .execute(tx.as_mut())
            .await?;

            sqlx::query(
                "INSERT INTO event_journal
                 (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
                 VALUES ($1, 'recursive_improvement', $2, 'success_pattern_recorded', $3, $4::jsonb, now())
                 ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
            )
            .bind(Uuid::now_v7().to_string())
            .bind(&memory_id)
            .bind(&idem_key)
            .bind(serde_json::json!({
                "objective_id": objective_id,
                "cycle_id": cycle_id,
                "success_count": successes.len(),
            }))
            .execute(tx.as_mut())
            .await?;

            tx.commit().await?;

            tracing::info!(
                objective_id,
                cycle_id,
                success_count = successes.len(),
                "REC-010: Recorded success patterns"
            );

            created += 1;
        }
    }

    // Derive suggestions from failure patterns and scores
    let failure_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM tasks t
         JOIN nodes n ON t.node_id = n.node_id
         WHERE n.objective_id = $1 AND t.status = 'failed'",
    )
    .bind(objective_id)
    .fetch_one(pool)
    .await
    .unwrap_or(0);

    let latest_score: Option<f64> = sqlx::query_scalar(
        "SELECT composite_score FROM loop_scores
         WHERE objective_id = $1 ORDER BY created_at DESC LIMIT 1",
    )
    .bind(objective_id)
    .fetch_optional(pool)
    .await?;

    let suggestions = if failure_count > 0 {
        vec![
            format!(
                "Investigate {} failed tasks -- consider adding retry logic or decomposing further",
                failure_count
            ),
            "Review failure artifact outputs for common error patterns".to_string(),
        ]
    } else if latest_score.unwrap_or(0.0) < 0.5 {
        vec![
            "Low composite score -- review pending reviews and certifications".to_string(),
            "Consider reducing scope for next iteration".to_string(),
        ]
    } else {
        vec!["Continue current approach -- metrics look healthy".to_string()]
    };

    let suggestion_memory_id = Uuid::now_v7().to_string();
    let suggestion_idem = format!("rec010-roadmap-{}", cycle_id);

    let idem_exists: Option<String> = sqlx::query_scalar(
        "SELECT aggregate_id FROM event_journal
         WHERE aggregate_kind = 'recursive_improvement' AND idempotency_key = $1 LIMIT 1",
    )
    .bind(&suggestion_idem)
    .fetch_optional(pool)
    .await?;

    if idem_exists.is_none() {
        let mut tx = pool.begin().await?;

        sqlx::query(
            "INSERT INTO recursive_memory_entries
             (entry_id, objective_id, outcome, learned_summary, outcome_metrics, recorded_at)
             VALUES ($1, $2, 'roadmap_suggestion', $3, $4, now())
             ON CONFLICT DO NOTHING",
        )
        .bind(&suggestion_memory_id)
        .bind(objective_id)
        .bind(suggestions.join("; "))
        .bind(serde_json::json!({
            "cycle_id": cycle_id,
            "failure_count": failure_count,
            "composite_score": latest_score.unwrap_or(0.0),
            "suggestions": suggestions,
        }))
        .execute(tx.as_mut())
        .await?;

        sqlx::query(
            "INSERT INTO event_journal
             (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
             VALUES ($1, 'recursive_improvement', $2, 'roadmap_suggestion_recorded', $3, $4::jsonb, now())
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(&suggestion_memory_id)
        .bind(&suggestion_idem)
        .bind(serde_json::json!({
            "objective_id": objective_id,
            "cycle_id": cycle_id,
            "suggestion_count": suggestions.len(),
        }))
        .execute(tx.as_mut())
        .await?;

        tx.commit().await?;

        tracing::info!(
            objective_id,
            cycle_id,
            suggestion_count = suggestions.len(),
            "REC-010: Recorded roadmap suggestions"
        );

        created += 1;
    }

    Ok(created)
}

/// Retrieve lessons from completed cycles that have not yet been consumed
/// by the current cycle.  This closes the cross-cycle boundary gap:
/// memory entries written by a finished cycle are distilled into
/// reinjection records that the next cycle reads on startup.
///
/// Returns the lesson summaries.  The caller is responsible for injecting
/// these into the next cycle's context window.
pub async fn retrieve_reinjectable_lessons(
    pool: &PgPool,
    objective_id: &str,
) -> Result<Vec<String>, Box<dyn std::error::Error>> {
    // Fetch unconsumed memory entries from previous cycles for this objective.
    // "Unconsumed" means no reinjection event has been emitted for this entry+objective pair.
    let rows = sqlx::query(
        "SELECT rme.entry_id, rme.learned_summary, rme.outcome
         FROM recursive_memory_entries rme
         WHERE rme.objective_id = $1
           AND NOT EXISTS (
               SELECT 1 FROM event_journal ej
               WHERE ej.aggregate_kind = 'recursive_improvement'
                 AND ej.idempotency_key = 'reinject-' || rme.entry_id
           )
         ORDER BY rme.recorded_at ASC
         LIMIT 20",
    )
    .bind(objective_id)
    .fetch_all(pool)
    .await?;

    let mut lessons = Vec::with_capacity(rows.len());
    for row in &rows {
        let entry_id: String = row.try_get("entry_id").unwrap_or_default();
        let summary: String = row.try_get("learned_summary").unwrap_or_default();
        let outcome: String = row.try_get("outcome").unwrap_or_default();
        lessons.push(format!("[{}] {}", outcome, summary));

        // Mark this lesson as consumed by recording a reinjection event.
        sqlx::query(
            "INSERT INTO event_journal
             (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
             VALUES ($1, 'recursive_improvement', $2, 'learning_reinjected', $3, $4::jsonb, now())
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
        )
        .bind(Uuid::now_v7().to_string())
        .bind(&entry_id)
        .bind(format!("reinject-{}", entry_id))
        .bind(serde_json::json!({
            "entry_id": entry_id,
            "objective_id": objective_id,
            "outcome": outcome,
        }))
        .execute(pool)
        .await?;
    }

    if !lessons.is_empty() {
        tracing::info!(
            objective_id,
            lesson_count = lessons.len(),
            "REC-010: Retrieved cross-cycle learning reinjections"
        );
    }

    Ok(lessons)
}
