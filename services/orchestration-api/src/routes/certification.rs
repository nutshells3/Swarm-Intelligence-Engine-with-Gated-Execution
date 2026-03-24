//! Certification API endpoints.
//!
//! These routes expose certification configuration (enable/disable, frequency),
//! manual submission of candidates, queue listing, and result retrieval.

use axum::extract::{Path, State};
use axum::response::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::error::{ApiResult, bad_request, internal_error, not_found};
use crate::state::AppState;

// ── GET /api/certification/config ───────────────────────────────────────

/// Response for the current certification configuration.
#[derive(Serialize, utoipa::ToSchema)]
pub struct CertificationConfigResponse {
    pub enabled: bool,
    pub frequency: String,
    pub routing: String,
    pub policy_id: String,
    pub revision: i32,
}

/// Get the current certification configuration from user_policies.
#[utoipa::path(
    get,
    path = "/api/certification/config",
    responses(
        (status = 200, description = "Certification config", body = CertificationConfigResponse)
    )
)]
pub async fn get_certification_config(
    State(state): State<AppState>,
) -> ApiResult<CertificationConfigResponse> {
    let row = sqlx::query(
        "SELECT policy_id, policy_payload, revision FROM user_policies WHERE policy_id = 'certification_config'",
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?;

    match row {
        Some(row) => {
            let payload: Value = row.try_get("policy_payload").map_err(internal_error)?;
            let revision: i32 = row.try_get("revision").map_err(internal_error)?;
            Ok(Json(CertificationConfigResponse {
                enabled: payload
                    .get("enabled")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false),
                frequency: payload
                    .get("frequency")
                    .and_then(|v| v.as_str())
                    .unwrap_or("off")
                    .to_string(),
                routing: payload
                    .get("routing")
                    .and_then(|v| v.as_str())
                    .unwrap_or("local")
                    .to_string(),
                policy_id: "certification_config".to_string(),
                revision,
            }))
        }
        None => Ok(Json(CertificationConfigResponse {
            enabled: false,
            frequency: "off".to_string(),
            routing: "local".to_string(),
            policy_id: "certification_config".to_string(),
            revision: 0,
        })),
    }
}

// ── PATCH /api/certification/config ─────────────────────────────────────

/// Request body for updating certification configuration.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateCertificationConfigRequest {
    /// Whether certification is enabled.
    pub enabled: Option<bool>,
    /// Certification frequency: "always", "on_request", "critical_only", or "off".
    pub frequency: Option<String>,
}

/// Update the certification configuration.
#[utoipa::path(
    patch,
    path = "/api/certification/config",
    request_body = UpdateCertificationConfigRequest,
    responses(
        (status = 200, description = "Updated certification config", body = CertificationConfigResponse)
    )
)]
pub async fn update_certification_config(
    State(state): State<AppState>,
    Json(req): Json<UpdateCertificationConfigRequest>,
) -> ApiResult<CertificationConfigResponse> {
    // Validate frequency if provided
    if let Some(ref freq) = req.frequency {
        let valid = ["always", "on_request", "critical_only", "off"];
        if !valid.contains(&freq.as_str()) {
            return Err(bad_request(&format!(
                "invalid frequency '{}'; must be one of: {}",
                freq,
                valid.join(", ")
            )));
        }
    }

    let mut tx = state.pool.begin().await.map_err(internal_error)?;

    // Get current config or default
    let current = sqlx::query(
        "SELECT policy_payload FROM user_policies WHERE policy_id = 'certification_config'",
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    let mut payload = match current {
        Some(row) => {
            let p: Value = row.try_get("policy_payload").map_err(internal_error)?;
            p
        }
        None => serde_json::json!({
            "enabled": false,
            "frequency": "off"
        }),
    };

    // Apply updates
    if let Some(enabled) = req.enabled {
        payload["enabled"] = serde_json::Value::Bool(enabled);
    }
    if let Some(ref frequency) = req.frequency {
        payload["frequency"] = serde_json::Value::String(frequency.clone());
    }

    // Upsert the policy
    let row = sqlx::query(
        r#"
        INSERT INTO user_policies (policy_id, policy_payload, created_at, revision)
        VALUES ('certification_config', $1::jsonb, now(), 1)
        ON CONFLICT (policy_id) DO UPDATE
        SET policy_payload = EXCLUDED.policy_payload,
            revision = user_policies.revision + 1
        RETURNING policy_id, policy_payload, revision
        "#,
    )
    .bind(&payload)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Record the event
    let idempotency_key = format!("cert-config-{}", Uuid::now_v7());
    sqlx::query(
        r#"
        INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
        VALUES ($1, 'user_policy', 'certification_config', 'certification_config_updated', $2, $3::jsonb, now())
        ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING
        "#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&idempotency_key)
    .bind(&payload)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    let final_payload: Value = row.try_get("policy_payload").map_err(internal_error)?;
    let revision: i32 = row.try_get("revision").map_err(internal_error)?;

    Ok(Json(CertificationConfigResponse {
        enabled: final_payload
            .get("enabled")
            .and_then(|v| v.as_bool())
            .unwrap_or(false),
        frequency: final_payload
            .get("frequency")
            .and_then(|v| v.as_str())
            .unwrap_or("off")
            .to_string(),
        routing: final_payload
            .get("routing")
            .and_then(|v| v.as_str())
            .unwrap_or("local")
            .to_string(),
        policy_id: "certification_config".to_string(),
        revision,
    }))
}

// ── POST /api/certification/submit ──────────────────────────────────────

/// Request body for manually submitting a certification candidate.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct SubmitCertificationRequest {
    pub node_id: String,
    pub task_id: String,
    pub claim_summary: String,
    pub source_anchors: Option<Vec<String>>,
    pub eligibility_reason: String,
    pub idempotency_key: String,
}

/// Response for a certification submission.
#[derive(Serialize, utoipa::ToSchema)]
pub struct CertificationSubmissionResponse {
    pub candidate_id: String,
    pub submission_id: String,
    pub queue_status: String,
    pub duplicated: bool,
}

/// Manually submit a certification candidate.
#[utoipa::path(
    post,
    path = "/api/certification/submit",
    request_body = SubmitCertificationRequest,
    responses(
        (status = 200, description = "Certification submission", body = CertificationSubmissionResponse)
    )
)]
pub async fn submit_certification(
    State(state): State<AppState>,
    Json(req): Json<SubmitCertificationRequest>,
) -> ApiResult<CertificationSubmissionResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;

    // Idempotency check on submission
    let duplicate: Option<String> = sqlx::query_scalar(
        "SELECT submission_id FROM certification_submissions WHERE idempotency_key = $1",
    )
    .bind(&req.idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    if let Some(existing_submission_id) = duplicate {
        let row = sqlx::query(
            "SELECT cs.submission_id, cs.candidate_id, cs.queue_status
             FROM certification_submissions cs
             WHERE cs.submission_id = $1",
        )
        .bind(&existing_submission_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal_error)?;

        tx.commit().await.map_err(internal_error)?;

        return Ok(Json(CertificationSubmissionResponse {
            candidate_id: row.try_get("candidate_id").map_err(internal_error)?,
            submission_id: row.try_get("submission_id").map_err(internal_error)?,
            queue_status: row.try_get("queue_status").map_err(internal_error)?,
            duplicated: true,
        }));
    }

    let candidate_id = Uuid::now_v7().to_string();
    let submission_id = Uuid::now_v7().to_string();
    let anchors = req
        .source_anchors
        .unwrap_or_default();
    let anchors_json = serde_json::to_value(&anchors).map_err(internal_error)?;

    // Create the candidate
    sqlx::query(
        r#"
        INSERT INTO certification_candidates
            (candidate_id, node_id, task_id, claim_summary, source_anchors, eligibility_reason, created_at)
        VALUES ($1, $2, $3, $4, $5::jsonb, $6, now())
        "#,
    )
    .bind(&candidate_id)
    .bind(&req.node_id)
    .bind(&req.task_id)
    .bind(&req.claim_summary)
    .bind(&anchors_json)
    .bind(&req.eligibility_reason)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Create the submission
    sqlx::query(
        r#"
        INSERT INTO certification_submissions
            (submission_id, candidate_id, idempotency_key, submitted_at, queue_status, retry_count, max_retries, status_changed_at)
        VALUES ($1, $2, $3, now(), 'pending', 0, 3, now())
        "#,
    )
    .bind(&submission_id)
    .bind(&candidate_id)
    .bind(&req.idempotency_key)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Record the event
    sqlx::query(
        r#"
        INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
        VALUES ($1, 'certification', $2, 'certification_submitted', $3, $4::jsonb, now())
        ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING
        "#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&candidate_id)
    .bind(&req.idempotency_key)
    .bind(serde_json::json!({
        "candidate_id": candidate_id,
        "submission_id": submission_id,
        "node_id": req.node_id,
        "task_id": req.task_id,
        "claim_summary": req.claim_summary,
        "eligibility_reason": req.eligibility_reason,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    Ok(Json(CertificationSubmissionResponse {
        candidate_id,
        submission_id,
        queue_status: "pending".to_string(),
        duplicated: false,
    }))
}

// ── GET /api/certification/queue ────────────────────────────────────────

/// A single entry in the certification queue.
/// FCG-015: UI projection for certification state.
#[derive(Serialize, utoipa::ToSchema)]
pub struct CertificationQueueEntryResponse {
    pub submission_id: String,
    pub candidate_id: String,
    pub node_id: String,
    pub task_id: String,
    pub claim_summary: String,
    pub queue_status: String,
    pub submitted_at: String,
    pub retry_count: i32,
    /// Elapsed time since submission in human-readable form.
    pub elapsed_display: String,
    /// The eligibility reason that triggered certification.
    pub eligibility_reason: String,
    /// External gate result (if completed).
    pub external_gate: Option<String>,
    /// Local gate effect (if completed).
    pub local_gate_effect: Option<String>,
}

/// List pending and completed certification submissions.
/// FCG-015: Returns useful data for the certification queue panel.
#[utoipa::path(
    get,
    path = "/api/certification/queue",
    responses(
        (status = 200, description = "Certification queue", body = Vec<CertificationQueueEntryResponse>)
    )
)]
pub async fn list_certification_queue(
    State(state): State<AppState>,
) -> ApiResult<Vec<CertificationQueueEntryResponse>> {
    let rows = sqlx::query(
        r#"
        SELECT cs.submission_id, cs.candidate_id, cs.queue_status, cs.submitted_at, cs.retry_count,
               cc.node_id, cc.task_id, cc.claim_summary, cc.eligibility_reason,
               crp.external_gate, crp.local_gate_effect
        FROM certification_submissions cs
        JOIN certification_candidates cc ON cs.candidate_id = cc.candidate_id
        LEFT JOIN certification_result_projections crp ON crp.submission_id = cs.submission_id
        ORDER BY cs.submitted_at DESC
        LIMIT 200
        "#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let now = chrono::Utc::now();
    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let submitted_at: chrono::DateTime<chrono::Utc> =
            row.try_get("submitted_at").map_err(internal_error)?;
        let elapsed = now - submitted_at;
        let elapsed_display = if elapsed.num_hours() > 0 {
            format!("{}h {}m", elapsed.num_hours(), elapsed.num_minutes() % 60)
        } else if elapsed.num_minutes() > 0 {
            format!("{}m", elapsed.num_minutes())
        } else {
            format!("{}s", elapsed.num_seconds())
        };
        results.push(CertificationQueueEntryResponse {
            submission_id: row.try_get("submission_id").map_err(internal_error)?,
            candidate_id: row.try_get("candidate_id").map_err(internal_error)?,
            node_id: row.try_get("node_id").map_err(internal_error)?,
            task_id: row.try_get("task_id").map_err(internal_error)?,
            claim_summary: row.try_get("claim_summary").map_err(internal_error)?,
            queue_status: row.try_get("queue_status").map_err(internal_error)?,
            submitted_at: submitted_at.to_rfc3339(),
            retry_count: row.try_get("retry_count").map_err(internal_error)?,
            elapsed_display,
            eligibility_reason: row.try_get("eligibility_reason").unwrap_or_else(|_| "unknown".to_string()),
            external_gate: row.try_get("external_gate").ok(),
            local_gate_effect: row.try_get("local_gate_effect").ok(),
        });
    }

    Ok(Json(results))
}

// ── GET /api/certification/results/{submission_id} ──────────────────────

/// Full certification result for a given submission.
#[derive(Serialize, utoipa::ToSchema)]
pub struct CertificationResultResponse {
    pub submission_id: String,
    pub candidate_id: String,
    pub queue_status: String,
    pub external_gate: Option<String>,
    pub local_gate_effect: Option<String>,
    pub lane_transition: Option<String>,
    pub projected_grade: Option<String>,
    pub projected_at: Option<String>,
}

/// Get the certification result for a specific submission.
#[utoipa::path(
    get,
    path = "/api/certification/results/{submission_id}",
    params(("submission_id" = String, Path, description = "Submission ID")),
    responses(
        (status = 200, description = "Certification result", body = CertificationResultResponse)
    )
)]
pub async fn get_certification_result(
    State(state): State<AppState>,
    Path(submission_id): Path<String>,
) -> ApiResult<CertificationResultResponse> {
    let row = sqlx::query(
        r#"
        SELECT cs.submission_id, cs.candidate_id, cs.queue_status,
               crp.external_gate, crp.local_gate_effect, crp.lane_transition,
               crp.projected_grade, crp.projected_at
        FROM certification_submissions cs
        LEFT JOIN certification_result_projections crp ON cs.submission_id = crp.submission_id
        WHERE cs.submission_id = $1
        "#,
    )
    .bind(&submission_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?;

    let Some(row) = row else {
        return Err(not_found("certification submission not found"));
    };

    let projected_at: Option<chrono::DateTime<chrono::Utc>> =
        row.try_get("projected_at").map_err(internal_error)?;

    Ok(Json(CertificationResultResponse {
        submission_id: row.try_get("submission_id").map_err(internal_error)?,
        candidate_id: row.try_get("candidate_id").map_err(internal_error)?,
        queue_status: row.try_get("queue_status").map_err(internal_error)?,
        external_gate: row.try_get("external_gate").map_err(internal_error)?,
        local_gate_effect: row.try_get("local_gate_effect").map_err(internal_error)?,
        lane_transition: row.try_get("lane_transition").map_err(internal_error)?,
        projected_grade: row.try_get("projected_grade").map_err(internal_error)?,
        projected_at: projected_at.map(|t| t.to_rfc3339()),
    }))
}
