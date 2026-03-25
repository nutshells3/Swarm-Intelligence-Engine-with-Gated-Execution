use axum::extract::{Path, State};
use axum::response::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::error::{ApiResult, bad_request, internal_error, not_found};
use crate::state::AppState;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PolicySnapshotRequest {
    pub policy_id: String,
    pub idempotency_key: String,
    pub policy_payload: Value,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct PolicySnapshotResponse {
    pub policy_id: String,
    pub revision: i32,
    pub duplicated: bool,
    pub policy_payload: Value,
}

/// GET /api/policies
///
/// Returns the most recent policy snapshots (up to 10).
#[utoipa::path(
    get,
    path = "/api/policies",
    responses(
        (status = 200, description = "List of policy snapshots", body = Vec<PolicySnapshotResponse>)
    )
)]
pub async fn list_policies(
    State(state): State<AppState>,
) -> ApiResult<Vec<PolicySnapshotResponse>> {
    let rows = sqlx::query(
        "SELECT policy_id, policy_payload, revision FROM user_policies ORDER BY created_at DESC LIMIT 10",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let policy_payload: Value = row.try_get("policy_payload").map_err(internal_error)?;
        let revision: i32 = row.try_get("revision").map_err(internal_error)?;
        let policy_id: String = row.try_get("policy_id").map_err(internal_error)?;
        results.push(PolicySnapshotResponse {
            policy_id,
            revision,
            duplicated: false,
            policy_payload,
        });
    }

    Ok(Json(results))
}

#[utoipa::path(
    post,
    path = "/api/policies",
    request_body = PolicySnapshotRequest,
    responses(
        (status = 200, description = "Created policy snapshot", body = PolicySnapshotResponse)
    )
)]
pub async fn create_policy_snapshot(
    State(state): State<AppState>,
    Json(request): Json<PolicySnapshotRequest>,
) -> ApiResult<PolicySnapshotResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;

    let duplicate: Option<String> = sqlx::query_scalar(
        r#"
        select event_id
        from event_journal
        where aggregate_kind = $1
          and aggregate_id = $2
          and idempotency_key = $3
        "#,
    )
    .bind("user_policy")
    .bind(&request.policy_id)
    .bind(&request.idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    let row = if duplicate.is_some() {
        sqlx::query(
            r#"
            select policy_id, policy_payload, revision
            from user_policies
            where policy_id = $1
            "#,
        )
        .bind(&request.policy_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal_error)?
    } else {
        let row = sqlx::query(
            r#"
            insert into user_policies (policy_id, policy_payload, created_at, revision)
            values ($1, $2::jsonb, now(), 1)
            on conflict (policy_id) do update
            set policy_payload = excluded.policy_payload,
                revision = user_policies.revision + 1
            returning policy_id, policy_payload, revision
            "#,
        )
        .bind(&request.policy_id)
        .bind(&request.policy_payload)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal_error)?;

        sqlx::query(
            r#"
            insert into event_journal (
                event_id,
                aggregate_kind,
                aggregate_id,
                event_kind,
                idempotency_key,
                payload,
                created_at
            )
            values ($1, $2, $3, $4, $5, $6::jsonb, now())
            on conflict (aggregate_kind, aggregate_id, idempotency_key) do nothing
            "#,
        )
        .bind(Uuid::now_v7().to_string())
        .bind("user_policy")
        .bind(&request.policy_id)
        .bind("user_policy_snapshot_saved")
        .bind(&request.idempotency_key)
        .bind(serde_json::json!({
            "policy_id": request.policy_id,
            "policy_payload": request.policy_payload,
        }))
        .execute(&mut *tx)
        .await
        .map_err(internal_error)?;
        row
    };

    tx.commit().await.map_err(internal_error)?;

    let policy_payload: Value = row.try_get("policy_payload").map_err(internal_error)?;
    let revision: i32 = row.try_get("revision").map_err(internal_error)?;
    let policy_id: String = row.try_get("policy_id").map_err(internal_error)?;

    Ok(Json(PolicySnapshotResponse {
        policy_id,
        revision,
        duplicated: duplicate.is_some(),
        policy_payload,
    }))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CertificationSettingsRequest {
    pub enabled: bool,
    pub frequency: String,
    pub routing: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CertificationSettingsResponse {
    pub policy_id: String,
    pub revision: i32,
    pub certification: CertificationSettingsPayload,
    pub updated: bool,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct CertificationSettingsPayload {
    pub enabled: bool,
    pub frequency: String,
    pub routing: String,
}

const VALID_FREQUENCIES: &[&str] = &["always", "on_request", "critical_only", "off"];
const VALID_ROUTINGS: &[&str] = &["local", "remote"];

/// PATCH /api/policies/{id}/certification
///
/// Toggle certification settings for a policy. This is a user-visible
/// toggle, not a hidden config.
#[utoipa::path(
    patch,
    path = "/api/policies/{id}/certification",
    params(("id" = String, Path, description = "Policy ID")),
    request_body = CertificationSettingsRequest,
    responses(
        (status = 200, description = "Updated certification settings", body = CertificationSettingsResponse)
    )
)]
pub async fn update_certification(
    State(state): State<AppState>,
    Path(policy_id): Path<String>,
    Json(req): Json<CertificationSettingsRequest>,
) -> ApiResult<CertificationSettingsResponse> {
    if !VALID_FREQUENCIES.contains(&req.frequency.as_str()) {
        return Err(bad_request(&format!(
            "frequency must be one of: {}",
            VALID_FREQUENCIES.join(", ")
        )));
    }
    if !VALID_ROUTINGS.contains(&req.routing.as_str()) {
        return Err(bad_request(&format!(
            "routing must be one of: {}",
            VALID_ROUTINGS.join(", ")
        )));
    }

    let mut tx = state.pool.begin().await.map_err(internal_error)?;

    // Fetch current policy
    let row = sqlx::query(
        "SELECT policy_id, policy_payload, revision FROM user_policies WHERE policy_id = $1",
    )
    .bind(&policy_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    let Some(row) = row else {
        return Err(not_found("policy not found"));
    };

    let mut payload: Value = row.try_get("policy_payload").map_err(internal_error)?;
    let current_revision: i32 = row.try_get("revision").map_err(internal_error)?;

    // Merge certification settings into the policy payload
    let cert_value = serde_json::json!({
        "enabled": req.enabled,
        "frequency": req.frequency,
        "routing": req.routing,
    });

    if let Some(obj) = payload.as_object_mut() {
        obj.insert("certification".to_string(), cert_value);
    }

    let new_revision = current_revision + 1;

    sqlx::query(
        r#"UPDATE user_policies
           SET policy_payload = $1::jsonb, revision = $2
           WHERE policy_id = $3"#,
    )
    .bind(&payload)
    .bind(new_revision)
    .bind(&policy_id)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Emit event
    sqlx::query(
        r#"INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           VALUES ($1, 'user_policy', $2, 'certification_settings_updated', $3, $4::jsonb, now())
           ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&policy_id)
    .bind(&format!("cert_update_{}_{}", policy_id, Uuid::now_v7()))
    .bind(serde_json::json!({
        "policy_id": policy_id,
        "certification": {
            "enabled": req.enabled,
            "frequency": req.frequency,
            "routing": req.routing,
        },
        "revision": new_revision,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    Ok(Json(CertificationSettingsResponse {
        policy_id,
        revision: new_revision,
        certification: CertificationSettingsPayload {
            enabled: req.enabled,
            frequency: req.frequency,
            routing: req.routing,
        },
        updated: true,
    }))
}

#[utoipa::path(
    get,
    path = "/api/policies/{id}",
    params(("id" = String, Path, description = "Policy ID")),
    responses(
        (status = 200, description = "Policy snapshot", body = PolicySnapshotResponse)
    )
)]
pub async fn get_policy_snapshot(
    State(state): State<AppState>,
    Path(policy_id): Path<String>,
) -> ApiResult<PolicySnapshotResponse> {
    let row = sqlx::query(
        r#"
        select policy_id, policy_payload, revision
        from user_policies
        where policy_id = $1
        "#,
    )
    .bind(policy_id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?;

    let Some(row) = row else {
        return Err(not_found("policy not found"));
    };

    let policy_payload: Value = row.try_get("policy_payload").map_err(internal_error)?;
    let revision: i32 = row.try_get("revision").map_err(internal_error)?;
    let policy_id: String = row.try_get("policy_id").map_err(internal_error)?;

    Ok(Json(PolicySnapshotResponse {
        policy_id,
        revision,
        duplicated: false,
        policy_payload,
    }))
}
