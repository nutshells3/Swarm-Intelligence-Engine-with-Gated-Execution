use axum::extract::{Path, Query, State};
use axum::response::Json;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use review_governance::{ReviewArtifactSummary, ReviewKind, ReviewOutcome, generate_review_digest};

use crate::error::{ApiResult, bad_request, internal_error, not_found};
use crate::state::AppState;

// ── Request / Response types ────────────────────────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateReviewRequest {
    pub review_kind: String,
    pub target_ref: String,
    pub reviewer_template_id: Option<String>,
    pub idempotency_key: String,
}

#[derive(Serialize, Clone, utoipa::ToSchema)]
pub struct ReviewResponse {
    pub review_id: String,
    pub review_kind: String,
    pub target_ref: String,
    pub reviewer_template_id: Option<String>,
    pub status: String,
    pub score_or_verdict: Option<String>,
    pub findings_summary: String,
    pub conditions: serde_json::Value,
    pub approval_effect: Option<String>,
    pub is_auto_approval: bool,
    pub recorded_at: String,
    pub duplicated: bool,
}

/// Response type for the human digest endpoint (REV-019).
#[derive(Serialize, Clone, utoipa::ToSchema)]
pub struct ReviewDigestResponse {
    pub objective_id: String,
    pub review_count: usize,
    pub digest: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateReviewRequest {
    pub status: Option<String>,
    pub score_or_verdict: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ApproveReviewRequest {
    pub verdict: Option<String>,
    pub approval_effect: Option<String>,
}

/// Standard SELECT columns for review_artifacts queries.
const REVIEW_SELECT_COLS: &str = "review_id, review_kind, target_ref, reviewer_template_id, status, score_or_verdict, findings_summary, conditions, approval_effect, is_auto_approval, recorded_at";

/// Build a `ReviewResponse` from a sqlx Row containing REVIEW_SELECT_COLS.
fn review_from_row(row: &sqlx::postgres::PgRow, duplicated: bool) -> Result<ReviewResponse, (axum::http::StatusCode, String)> {
    let recorded_at: chrono::DateTime<chrono::Utc> =
        row.try_get("recorded_at").map_err(internal_error)?;
    Ok(ReviewResponse {
        review_id: row.try_get("review_id").map_err(internal_error)?,
        review_kind: row.try_get("review_kind").map_err(internal_error)?,
        target_ref: row.try_get("target_ref").map_err(internal_error)?,
        reviewer_template_id: row.try_get("reviewer_template_id").map_err(internal_error)?,
        status: row.try_get("status").map_err(internal_error)?,
        score_or_verdict: row.try_get("score_or_verdict").map_err(internal_error)?,
        findings_summary: row.try_get("findings_summary").map_err(internal_error)?,
        conditions: row.try_get("conditions").map_err(internal_error)?,
        approval_effect: row.try_get("approval_effect").map_err(internal_error)?,
        is_auto_approval: row.try_get("is_auto_approval").map_err(internal_error)?,
        recorded_at: recorded_at.to_rfc3339(),
        duplicated,
    })
}

// Valid review kinds -- must match the review_governance::ReviewKind enum
// (snake_case serialization) and the CHECK constraint on review_artifacts.
const VALID_REVIEW_KINDS: &[&str] = &[
    "planning",
    "architecture",
    "direction",
    "milestone",
    "implementation",
];

const VALID_STATUSES: &[&str] = &["scheduled", "in_progress", "approved", "changes_requested", "superseded"];

// ── Handlers ────────────────────────────────────────────────────────────

/// POST /api/reviews
#[utoipa::path(
    post,
    path = "/api/reviews",
    request_body = CreateReviewRequest,
    responses(
        (status = 200, description = "Created review", body = ReviewResponse)
    )
)]
pub async fn create_review(
    State(state): State<AppState>,
    Json(req): Json<CreateReviewRequest>,
) -> ApiResult<ReviewResponse> {
    if !VALID_REVIEW_KINDS.contains(&req.review_kind.as_str()) {
        return Err(bad_request(&format!(
            "review_kind must be one of: {}",
            VALID_REVIEW_KINDS.join(", ")
        )));
    }

    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    let review_id = Uuid::now_v7().to_string();

    // BND-010: scoped idempotency check
    let duplicate: Option<String> = sqlx::query_scalar(
        "SELECT aggregate_id FROM event_journal WHERE aggregate_kind = 'review' AND idempotency_key = $1 LIMIT 1",
    )
    .bind(&req.idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    if let Some(existing_id) = duplicate {
        let row = sqlx::query(
            &format!("SELECT {} FROM review_artifacts WHERE review_id = $1", REVIEW_SELECT_COLS),
        )
        .bind(&existing_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal_error)?;

        tx.commit().await.map_err(internal_error)?;
        return Ok(Json(review_from_row(&row, true)?));
    }

    let row = sqlx::query(
        &format!(
            "INSERT INTO review_artifacts (review_id, review_kind, target_ref, reviewer_template_id, status, recorded_at) \
             VALUES ($1, $2, $3, $4, 'scheduled', now()) \
             RETURNING {}", REVIEW_SELECT_COLS
        ),
    )
    .bind(&review_id)
    .bind(&req.review_kind)
    .bind(&req.target_ref)
    .bind(&req.reviewer_template_id)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Emit event (ON CONFLICT guards against races with the idempotency check)
    sqlx::query(
        r#"INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           VALUES ($1, 'review', $2, 'review_created', $3, $4::jsonb, now())
           ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&review_id)
    .bind(&req.idempotency_key)
    .bind(serde_json::json!({
        "review_id": review_id,
        "review_kind": req.review_kind,
        "target_ref": req.target_ref,
        "reviewer_template_id": req.reviewer_template_id,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;
    Ok(Json(review_from_row(&row, false)?))
}

/// GET /api/reviews
#[utoipa::path(
    get,
    path = "/api/reviews",
    responses(
        (status = 200, description = "List of reviews", body = Vec<ReviewResponse>)
    )
)]
pub async fn list_reviews(
    State(state): State<AppState>,
) -> ApiResult<Vec<ReviewResponse>> {
    let rows = sqlx::query(
        &format!("SELECT {} FROM review_artifacts ORDER BY recorded_at DESC", REVIEW_SELECT_COLS),
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut results = Vec::with_capacity(rows.len());
    for row in &rows {
        results.push(review_from_row(row, false)?);
    }

    Ok(Json(results))
}

/// GET /api/reviews/{id}
#[utoipa::path(
    get,
    path = "/api/reviews/{id}",
    params(("id" = String, Path, description = "Review ID")),
    responses(
        (status = 200, description = "Review details", body = ReviewResponse)
    )
)]
pub async fn get_review(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<ReviewResponse> {
    let row = sqlx::query(
        &format!("SELECT {} FROM review_artifacts WHERE review_id = $1", REVIEW_SELECT_COLS),
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?;

    let Some(row) = row else {
        return Err(not_found("review not found"));
    };

    Ok(Json(review_from_row(&row, false)?))
}

/// PATCH /api/reviews/{id}
#[utoipa::path(
    patch,
    path = "/api/reviews/{id}",
    params(("id" = String, Path, description = "Review ID")),
    request_body = UpdateReviewRequest,
    responses(
        (status = 200, description = "Updated review", body = ReviewResponse)
    )
)]
pub async fn update_review(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<UpdateReviewRequest>,
) -> ApiResult<ReviewResponse> {
    if let Some(ref s) = req.status {
        if !VALID_STATUSES.contains(&s.as_str()) {
            return Err(bad_request(&format!(
                "status must be one of: {}",
                VALID_STATUSES.join(", ")
            )));
        }
    }

    let mut tx = state.pool.begin().await.map_err(internal_error)?;

    // Verify review exists
    let exists = sqlx::query("SELECT review_id FROM review_artifacts WHERE review_id = $1")
        .bind(&id)
        .fetch_optional(&mut *tx)
        .await
        .map_err(internal_error)?;

    if exists.is_none() {
        return Err(not_found("review not found"));
    }

    // Apply updates
    if let Some(ref status) = req.status {
        sqlx::query("UPDATE review_artifacts SET status = $1 WHERE review_id = $2")
            .bind(status)
            .bind(&id)
            .execute(&mut *tx)
            .await
            .map_err(internal_error)?;
    }

    if let Some(ref verdict) = req.score_or_verdict {
        sqlx::query("UPDATE review_artifacts SET score_or_verdict = $1 WHERE review_id = $2")
            .bind(verdict)
            .bind(&id)
            .execute(&mut *tx)
            .await
            .map_err(internal_error)?;
    }

    // Emit event
    sqlx::query(
        r#"INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           VALUES ($1, 'review', $2, 'review_updated', $3, $4::jsonb, now())"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&id)
    .bind(&format!("update_review_{}_{}", id, Uuid::now_v7()))
    .bind(serde_json::json!({
        "review_id": id,
        "status": req.status,
        "score_or_verdict": req.score_or_verdict,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Fetch updated row
    let row = sqlx::query(
        &format!("SELECT {} FROM review_artifacts WHERE review_id = $1", REVIEW_SELECT_COLS),
    )
    .bind(&id)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;
    Ok(Json(review_from_row(&row, false)?))
}

/// POST /api/reviews/{id}/approve
///
/// Approves a review, creating a durable artifact record.
#[utoipa::path(
    post,
    path = "/api/reviews/{id}/approve",
    params(("id" = String, Path, description = "Review ID")),
    request_body = ApproveReviewRequest,
    responses(
        (status = 200, description = "Approved review", body = ReviewResponse)
    )
)]
pub async fn approve_review(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ApproveReviewRequest>,
) -> ApiResult<ReviewResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;

    // Verify review exists and is not already approved
    let check_row = sqlx::query(
        "SELECT review_id, status FROM review_artifacts WHERE review_id = $1",
    )
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    let Some(check_row) = check_row else {
        return Err(not_found("review not found"));
    };

    let current_status: String = check_row.try_get("status").map_err(internal_error)?;
    if current_status == "approved" {
        return Err(bad_request("review is already approved"));
    }

    let verdict = req.verdict.unwrap_or_else(|| "approved".to_string());
    let approval_effect = req.approval_effect;

    sqlx::query(
        "UPDATE review_artifacts SET status = 'approved', score_or_verdict = $1, approval_effect = $2 WHERE review_id = $3",
    )
    .bind(&verdict)
    .bind(&approval_effect)
    .bind(&id)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Create durable artifact record
    let artifact_id = Uuid::now_v7().to_string();
    sqlx::query(
        r#"INSERT INTO artifact_refs (artifact_ref_id, artifact_kind, artifact_uri, metadata)
           VALUES ($1, 'review_approval', $2, $3::jsonb)"#,
    )
    .bind(&artifact_id)
    .bind(&format!("review://{}", id))
    .bind(serde_json::json!({
        "review_id": id,
        "verdict": verdict,
        "approval_effect": approval_effect,
        "approved_at": chrono::Utc::now().to_rfc3339(),
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Emit approval event
    sqlx::query(
        r#"INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           VALUES ($1, 'review', $2, 'review_approved', $3, $4::jsonb, now())"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&id)
    .bind(&format!("approve_review_{}", id))
    .bind(serde_json::json!({
        "review_id": id,
        "verdict": verdict,
        "approval_effect": approval_effect,
        "artifact_ref_id": artifact_id,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Fetch final state
    let row = sqlx::query(
        &format!("SELECT {} FROM review_artifacts WHERE review_id = $1", REVIEW_SELECT_COLS),
    )
    .bind(&id)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;
    Ok(Json(review_from_row(&row, false)?))
}

// ── REV-019: Human digest summary endpoint ──────────────────────────

#[derive(Deserialize)]
pub struct DigestQuery {
    pub objective_id: String,
}

/// Parse a review_kind SQL string back into `ReviewKind`.
fn parse_review_kind(s: &str) -> ReviewKind {
    match s {
        "planning" => ReviewKind::Planning,
        "architecture" => ReviewKind::Architecture,
        "direction" => ReviewKind::Direction,
        "milestone" => ReviewKind::Milestone,
        "implementation" => ReviewKind::Implementation,
        _ => ReviewKind::Planning, // fallback
    }
}

/// Parse an outcome string back into `ReviewOutcome`.
fn parse_outcome(s: Option<&str>) -> Option<ReviewOutcome> {
    match s {
        Some("approved") => Some(ReviewOutcome::Approved),
        Some("approved_with_conditions") => Some(ReviewOutcome::ApprovedWithConditions),
        Some("rejected") => Some(ReviewOutcome::Rejected),
        Some("inconclusive") => Some(ReviewOutcome::Inconclusive),
        _ => None,
    }
}

/// GET /api/reviews/digest?objective_id={id}
///
/// REV-019: Generate a human-readable digest for all reviews targeting
/// the given objective. Queries review_artifacts, maps to
/// ReviewArtifactSummary, calls generate_review_digest.
#[utoipa::path(
    get,
    path = "/api/reviews/digest",
    params(("objective_id" = String, Query, description = "Objective ID to generate digest for")),
    responses(
        (status = 200, description = "Review digest", body = ReviewDigestResponse)
    )
)]
pub async fn review_digest(
    State(state): State<AppState>,
    Query(params): Query<DigestQuery>,
) -> ApiResult<ReviewDigestResponse> {
    let rows = sqlx::query(
        &format!(
            "SELECT {} FROM review_artifacts WHERE target_ref = $1 ORDER BY recorded_at ASC",
            REVIEW_SELECT_COLS
        ),
    )
    .bind(&params.objective_id)
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let summaries: Vec<ReviewArtifactSummary> = rows
        .iter()
        .map(|row| {
            let review_id: String = row.try_get("review_id").unwrap_or_default();
            let review_kind_str: String = row.try_get("review_kind").unwrap_or_default();
            let target_ref: String = row.try_get("target_ref").unwrap_or_default();
            let outcome_str: Option<String> = row.try_get("score_or_verdict").unwrap_or(None);
            let findings: String = row.try_get("findings_summary").unwrap_or_default();
            let cond_val: serde_json::Value =
                row.try_get("conditions").unwrap_or(serde_json::json!([]));
            let recorded_at: chrono::DateTime<chrono::Utc> =
                row.try_get("recorded_at").unwrap_or_else(|_| chrono::Utc::now());

            let conditions: Vec<String> = cond_val
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();

            ReviewArtifactSummary {
                review_id,
                review_kind: parse_review_kind(&review_kind_str),
                target_ref,
                outcome: parse_outcome(outcome_str.as_deref()),
                findings_summary: findings,
                conditions,
                created_at: recorded_at,
            }
        })
        .collect();

    let review_count = summaries.len();
    let digest = generate_review_digest(&summaries);

    Ok(Json(ReviewDigestResponse {
        objective_id: params.objective_id,
        review_count,
        digest,
    }))
}
