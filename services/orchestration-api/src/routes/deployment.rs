//! DEP-012: Deployment config API routes.
//!
//! GET  /api/deployment/config — reads deployment_policies table, returns
//!     current mode + endpoints.
//! PATCH /api/deployment/config — updates deployment mode by inserting a
//!     new revision (immutable audit trail).

use axum::extract::State;
use axum::response::Json;
use serde::{Deserialize, Serialize};
use sqlx::Row;

use crate::error::{ApiResult, bad_request, internal_error};
use crate::state::AppState;

// ── Response / Request types ──────────────────────────────────────────

/// Response for GET /api/deployment/config.
#[derive(Serialize, utoipa::ToSchema)]
pub struct DeploymentConfigResponse {
    pub policy_id: String,
    pub revision: i32,
    pub scope: String,
    pub deployment_mode: String,
    pub update_channel: serde_json::Value,
    pub migration_compatibility: serde_json::Value,
    pub endpoints: Vec<EndpointSummary>,
    pub created_at: String,
    pub updated_at: String,
}

/// Summary of a configured remote endpoint.
#[derive(Serialize, utoipa::ToSchema)]
pub struct EndpointSummary {
    pub endpoint_id: String,
    pub endpoint_type: String,
    pub label: String,
    pub base_url: String,
    pub health: String,
    pub last_health_check: Option<String>,
}

/// Request for PATCH /api/deployment/config.
#[derive(Deserialize, utoipa::ToSchema)]
pub struct UpdateDeploymentConfigRequest {
    /// New deployment mode. Must be one of: local_only, local_plus_remote,
    /// remote_certification_preferred, certification_disabled.
    pub deployment_mode: String,
}

const VALID_DEPLOYMENT_MODES: &[&str] = &[
    "local_only",
    "local_plus_remote",
    "remote_certification_preferred",
    "certification_disabled",
];

// ── Handlers ──────────────────────────────────────────────────────────

/// GET /api/deployment/config
///
/// DEP-012: Returns the current deployment policy (highest revision for
/// scope='global') and all configured remote endpoints.
#[utoipa::path(
    get,
    path = "/api/deployment/config",
    responses(
        (status = 200, description = "Current deployment config", body = DeploymentConfigResponse)
    )
)]
pub async fn get_deployment_config(
    State(state): State<AppState>,
) -> ApiResult<DeploymentConfigResponse> {
    // Load the latest global policy revision
    let policy_row = sqlx::query(
        r#"SELECT policy_id, revision, scope, deployment_mode,
                  update_channel, migration_compatibility,
                  created_at, updated_at
           FROM deployment_policies
           WHERE scope = 'global'
           ORDER BY revision DESC
           LIMIT 1"#,
    )
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?;

    let (policy_id, revision, scope, deployment_mode, update_channel, migration_compatibility, created_at_str, updated_at_str) =
        if let Some(row) = policy_row {
            let created_at: chrono::DateTime<chrono::Utc> =
                row.try_get("created_at").map_err(internal_error)?;
            let updated_at: chrono::DateTime<chrono::Utc> =
                row.try_get("updated_at").map_err(internal_error)?;
            let uc: serde_json::Value =
                row.try_get("update_channel").map_err(internal_error)?;
            let mc: serde_json::Value =
                row.try_get("migration_compatibility").map_err(internal_error)?;
            (
                row.try_get::<String, _>("policy_id").map_err(internal_error)?,
                row.try_get::<i32, _>("revision").map_err(internal_error)?,
                row.try_get::<String, _>("scope").map_err(internal_error)?,
                row.try_get::<String, _>("deployment_mode").map_err(internal_error)?,
                uc,
                mc,
                created_at.to_rfc3339(),
                updated_at.to_rfc3339(),
            )
        } else {
            // No policy exists yet -- return sensible defaults
            (
                String::new(),
                0,
                "global".to_string(),
                "local_only".to_string(),
                serde_json::json!({"channel": "stable", "auto_apply": false, "notify_on_available": true}),
                serde_json::json!({}),
                chrono::Utc::now().to_rfc3339(),
                chrono::Utc::now().to_rfc3339(),
            )
        };

    // Load configured endpoints
    let endpoint_rows = sqlx::query(
        r#"SELECT endpoint_id, endpoint_type, label, base_url,
                  health, last_health_check
           FROM remote_endpoints
           ORDER BY label"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let endpoints: Vec<EndpointSummary> = endpoint_rows
        .iter()
        .map(|row| {
            let last_hc: Option<chrono::DateTime<chrono::Utc>> =
                row.try_get("last_health_check").ok();
            EndpointSummary {
                endpoint_id: row.try_get("endpoint_id").unwrap_or_default(),
                endpoint_type: row.try_get("endpoint_type").unwrap_or_default(),
                label: row.try_get("label").unwrap_or_default(),
                base_url: row.try_get("base_url").unwrap_or_default(),
                health: row.try_get("health").unwrap_or_default(),
                last_health_check: last_hc.map(|dt| dt.to_rfc3339()),
            }
        })
        .collect();

    Ok(Json(DeploymentConfigResponse {
        policy_id,
        revision,
        scope,
        deployment_mode,
        update_channel,
        migration_compatibility,
        endpoints,
        created_at: created_at_str,
        updated_at: updated_at_str,
    }))
}

/// PATCH /api/deployment/config
///
/// DEP-012: Updates the deployment mode by inserting a new revision.
/// The previous revision remains for audit. Only `deployment_mode` is
/// mutable via this endpoint; other policy fields carry forward.
#[utoipa::path(
    patch,
    path = "/api/deployment/config",
    request_body = UpdateDeploymentConfigRequest,
    responses(
        (status = 200, description = "Updated deployment config", body = DeploymentConfigResponse)
    )
)]
pub async fn update_deployment_config(
    State(state): State<AppState>,
    Json(req): Json<UpdateDeploymentConfigRequest>,
) -> ApiResult<DeploymentConfigResponse> {
    // Validate deployment mode
    if !VALID_DEPLOYMENT_MODES.contains(&req.deployment_mode.as_str()) {
        return Err(bad_request(&format!(
            "deployment_mode must be one of: {}",
            VALID_DEPLOYMENT_MODES.join(", ")
        )));
    }

    let mut tx = state.pool.begin().await.map_err(internal_error)?;

    // Load the current revision (if any) so we can carry forward fields
    let current = sqlx::query(
        r#"SELECT policy_id, revision, scope, deployment_mode,
                  update_channel, migration_compatibility
           FROM deployment_policies
           WHERE scope = 'global'
           ORDER BY revision DESC
           LIMIT 1"#,
    )
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    let (new_revision, uc_json, mc_json) = if let Some(row) = &current {
        let rev: i32 = row.try_get("revision").map_err(internal_error)?;
        let uc: serde_json::Value = row.try_get("update_channel").map_err(internal_error)?;
        let mc: serde_json::Value = row.try_get("migration_compatibility").map_err(internal_error)?;
        (rev + 1, uc, mc)
    } else {
        (
            1,
            serde_json::json!({"channel": "stable", "auto_apply": false, "notify_on_available": true}),
            serde_json::json!({}),
        )
    };

    let new_policy_id = uuid::Uuid::now_v7().to_string();

    sqlx::query(
        r#"INSERT INTO deployment_policies
             (policy_id, revision, scope, deployment_mode,
              update_channel, migration_compatibility,
              created_at, updated_at)
           VALUES ($1, $2, 'global', $3, $4, $5, now(), now())"#,
    )
    .bind(&new_policy_id)
    .bind(new_revision)
    .bind(&req.deployment_mode)
    .bind(&uc_json)
    .bind(&mc_json)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Emit event
    sqlx::query(
        r#"INSERT INTO event_journal
             (event_id, aggregate_kind, aggregate_id, event_kind,
              idempotency_key, payload, created_at)
           VALUES ($1, 'deployment_policy', $2, 'deployment_mode_changed',
                   $3, $4::jsonb, now())
           ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING"#,
    )
    .bind(uuid::Uuid::now_v7().to_string())
    .bind(&new_policy_id)
    .bind(&format!("dep_mode_{}_{}", new_policy_id, new_revision))
    .bind(serde_json::json!({
        "policy_id": new_policy_id,
        "revision": new_revision,
        "deployment_mode": req.deployment_mode,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    // Return the full config (reuse GET logic via direct query)
    get_deployment_config(State(state)).await
}
