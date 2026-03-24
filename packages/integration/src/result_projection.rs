//! OAE -> SIEGE result projection layer.
//!
//! Translates a full `CertificationApiResult` from the OAE formal-claim
//! engine into the local SQL state model: submission status, plan gates,
//! cautions, conflicts, artifact refs, and event journal entries.

use crate::gateway::GateEffect;
use crate::http_gateway::CertificationApiResult;

/// Outcome of projecting a certification result into local state.
#[derive(Debug, Default)]
pub struct ProjectionOutcome {
    pub gate_effect: GateEffect,
    pub cautions_injected: u32,
    pub conflicts_created: u32,
    pub artifacts_stored: u32,
}

/// Project a full OAE `CertificationApiResult` into SIEGE's SQL state model.
///
/// This is the translation layer between OAE's response and SIEGE's tables.
/// It performs the following in a single transaction:
///
/// 1. Update `certification_submissions` queue_status and status_changed_at.
/// 2. Update `plan_gates` if the result carries a gate.
/// 3. Inject cautions into `tasks` when sorry holes are found.
/// 4. Create `conflicts` for dual-formalization divergence.
/// 5. Create `conflicts` for assurance-profile blocking issues.
/// 6. Store audit trust_surface and probe_results as `artifact_refs`.
/// 7. Store per-formalizer verification details as `artifact_refs`.
/// 8. Record errors in `event_journal`.
/// 9. Compute and return the `GateEffect`.
#[cfg(feature = "sqlx")]
pub async fn project_certification_result(
    pool: &sqlx::PgPool,
    task_id: &str,
    node_id: &str,
    submission_id: &str,
    result: &CertificationApiResult,
) -> Result<ProjectionOutcome, crate::cli_gateway::GatewayError> {
    use crate::cli_gateway::GatewayError;

    let mut tx = pool.begin().await.map_err(|e| GatewayError::IoError(e.to_string()))?;
    let mut outcome = ProjectionOutcome::default();

    // 1. Update certification_submissions status
    sqlx::query(
        "UPDATE certification_submissions \
         SET queue_status = $1, \
             status_changed_at = now() \
         WHERE submission_id = $2",
    )
    .bind("completed")
    .bind(submission_id)
    .execute(&mut *tx)
    .await
    .map_err(|e| GatewayError::IoError(e.to_string()))?;

    // 2. Map gate to plan_gates update
    if !result.gate.is_empty() {
        sqlx::query(
            "UPDATE plan_gates SET current_status = $1, evaluated_at = now() \
             WHERE plan_id IN ( \
                 SELECT p.plan_id FROM plans p \
                 JOIN nodes n ON p.objective_id = n.objective_id \
                 WHERE n.node_id = $2 \
                 ORDER BY p.created_at DESC LIMIT 1 \
             )",
        )
        .bind(&result.assurance_profile.formal_status)
        .bind(node_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| GatewayError::IoError(e.to_string()))?;
    }

    // 3. Inject cautions when sorry holes are detected
    let sorry_count = result.total_sorry_count();
    if sorry_count > 0 {
        let caution = serde_json::json!([
            format!("sorry detected: {} proof holes found by verifier", sorry_count)
        ]);
        sqlx::query(
            "UPDATE tasks SET cautions = cautions || $1, updated_at = now() WHERE task_id = $2",
        )
        .bind(&caution)
        .bind(task_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| GatewayError::IoError(e.to_string()))?;
        outcome.cautions_injected += 1;
    }

    // 4. Create conflict for dual-formalization divergence
    if result.has_divergence() {
        let conflict_id = uuid::Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO conflicts (conflict_id, node_id, conflict_kind, status, created_at, updated_at) \
             VALUES ($1, $2, 'divergence', 'open', now(), now()) \
             ON CONFLICT DO NOTHING",
        )
        .bind(&conflict_id)
        .bind(node_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| GatewayError::IoError(e.to_string()))?;
        outcome.conflicts_created += 1;
    }

    // 5. Create conflicts for blocking issues
    for _issue in &result.assurance_profile.blocking_issues {
        let conflict_id = uuid::Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO conflicts (conflict_id, node_id, conflict_kind, status, created_at, updated_at) \
             VALUES ($1, $2, 'mainline_integration', 'open', now(), now()) \
             ON CONFLICT DO NOTHING",
        )
        .bind(&conflict_id)
        .bind(node_id)
        .execute(&mut *tx)
        .await
        .map_err(|e| GatewayError::IoError(e.to_string()))?;
        outcome.conflicts_created += 1;
    }

    // 6. Store audit trust_surface and probe_results as artifact_refs
    if result.audit.trust_surface != serde_json::Value::Null {
        let artifact_id = uuid::Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO artifact_refs (artifact_ref_id, task_id, artifact_kind, artifact_uri, metadata) \
             VALUES ($1, $2, 'trust_surface', $3, $4)",
        )
        .bind(&artifact_id)
        .bind(task_id)
        .bind(serde_json::to_string(&result.audit.trust_surface).unwrap_or_default())
        .bind(serde_json::json!({"source": "oae_audit", "submission_id": submission_id}))
        .execute(&mut *tx)
        .await
        .map_err(|e| GatewayError::IoError(e.to_string()))?;
        outcome.artifacts_stored += 1;
    }

    for (i, probe) in result.audit.probe_results.iter().enumerate() {
        let artifact_id = uuid::Uuid::now_v7().to_string();
        sqlx::query(
            "INSERT INTO artifact_refs (artifact_ref_id, task_id, artifact_kind, artifact_uri, metadata) \
             VALUES ($1, $2, 'audit_probe', $3, $4)",
        )
        .bind(&artifact_id)
        .bind(task_id)
        .bind(serde_json::to_string(probe).unwrap_or_default())
        .bind(serde_json::json!({"source": "oae_audit", "probe_index": i, "submission_id": submission_id}))
        .execute(&mut *tx)
        .await
        .map_err(|e| GatewayError::IoError(e.to_string()))?;
        outcome.artifacts_stored += 1;
    }

    // 7. Store verification details as artifact_refs
    for (label, detail) in [("a", &result.verification_a), ("b", &result.verification_b)] {
        if let Some(v) = detail {
            let artifact_id = uuid::Uuid::now_v7().to_string();
            sqlx::query(
                "INSERT INTO artifact_refs (artifact_ref_id, task_id, artifact_kind, artifact_uri, metadata) \
                 VALUES ($1, $2, 'verification_result', $3, $4)",
            )
            .bind(&artifact_id)
            .bind(task_id)
            .bind(serde_json::to_string(v).unwrap_or_default())
            .bind(serde_json::json!({
                "source": "oae_verification",
                "formalizer": label,
                "backend_id": v.backend_id,
                "sorry_count": v.sorry_count,
                "success": v.success,
                "submission_id": submission_id
            }))
            .execute(&mut *tx)
            .await
            .map_err(|e| GatewayError::IoError(e.to_string()))?;
            outcome.artifacts_stored += 1;
        }
    }

    // 8. Record errors in event_journal
    if !result.errors.is_empty() {
        let event_id = uuid::Uuid::now_v7().to_string();
        let idempotency_key = format!("cert-errors-{}", submission_id);
        sqlx::query(
            "INSERT INTO event_journal \
             (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at) \
             VALUES ($1, 'certification', $2, 'certification_errors_recorded', $3, $4, now()) \
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
        )
        .bind(&event_id)
        .bind(submission_id)
        .bind(&idempotency_key)
        .bind(serde_json::json!({
            "task_id": task_id,
            "node_id": node_id,
            "errors": result.errors,
            "verdict": result.verdict,
        }))
        .execute(&mut *tx)
        .await
        .map_err(|e| GatewayError::IoError(e.to_string()))?;
    }

    // 9. Compute gate effect
    outcome.gate_effect = result.to_gate_effect();

    tx.commit().await.map_err(|e| GatewayError::IoError(e.to_string()))?;

    Ok(outcome)
}
