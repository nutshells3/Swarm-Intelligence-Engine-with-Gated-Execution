use axum::extract::{Path, State};
use axum::response::Json;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

use crate::error::{ApiResult, internal_error, not_found};
use crate::state::AppState;

// ── Skill Packs ─────────────────────────────────────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateSkillPackRequest {
    pub worker_role: String,
    pub description: String,
    pub accepted_task_kinds: Value,
    pub references: Value,
    pub scripts: Value,
    pub idempotency_key: String,
    /// SKL-005: Optional expected output contract.
    #[serde(default)]
    pub expected_output_contract: Option<String>,
    /// SKL-012: Optional semantic version.
    #[serde(default)]
    pub version: Option<String>,
}

/// SKL-014: Response includes accepted_task_kinds, version, deprecated,
/// and expected_output_contract for UI projection.
#[derive(Serialize, utoipa::ToSchema)]
pub struct SkillPackResponse {
    pub skill_pack_id: String,
    pub worker_role: String,
    pub description: String,
    pub accepted_task_kinds: Value,
    pub references: Value,
    pub scripts: Value,
    pub created_at: String,
    pub duplicated: bool,
    /// SKL-005: Expected output contract shape.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_output_contract: Option<String>,
    /// SKL-012: Semantic version of this skill pack.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    /// SKL-013: Whether this skill pack is deprecated.
    pub deprecated: bool,
}

/// Helper to build SkillPackResponse from a query row.
fn skill_pack_from_row(row: &sqlx::postgres::PgRow, duplicated: bool) -> Result<SkillPackResponse, (axum::http::StatusCode, String)> {
    let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
    Ok(SkillPackResponse {
        skill_pack_id: row.try_get("skill_pack_id").map_err(internal_error)?,
        worker_role: row.try_get("worker_role").map_err(internal_error)?,
        description: row.try_get("description").map_err(internal_error)?,
        accepted_task_kinds: row.try_get("accepted_task_kinds").map_err(internal_error)?,
        references: row.try_get("references").map_err(internal_error)?,
        scripts: row.try_get("scripts").map_err(internal_error)?,
        created_at: created_at.to_rfc3339(),
        duplicated,
        expected_output_contract: row.try_get("expected_output_contract").unwrap_or(None),
        version: row.try_get("version").unwrap_or(None),
        deprecated: row.try_get("deprecated").unwrap_or(false),
    })
}

#[utoipa::path(
    post,
    path = "/api/skills",
    request_body = CreateSkillPackRequest,
    responses(
        (status = 200, description = "Created skill pack", body = SkillPackResponse)
    )
)]
pub async fn create_skill_pack(
    State(state): State<AppState>,
    Json(req): Json<CreateSkillPackRequest>,
) -> ApiResult<SkillPackResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    let skill_pack_id = Uuid::now_v7().to_string();

    // BND-010: scoped idempotency check
    let duplicate: Option<String> = sqlx::query_scalar(
        "select aggregate_id from event_journal where aggregate_kind = 'skill_pack' and idempotency_key = $1 limit 1",
    )
    .bind(&req.idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    if let Some(existing_id) = duplicate {
        let row = sqlx::query(
            r#"select skill_pack_id, worker_role, description, accepted_task_kinds,
                      "references", scripts, created_at, expected_output_contract,
                      version, deprecated
               from skill_packs where skill_pack_id = $1"#,
        )
        .bind(&existing_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal_error)?;

        tx.commit().await.map_err(internal_error)?;
        return Ok(Json(skill_pack_from_row(&row, true)?));
    }

    let row = sqlx::query(
        r#"insert into skill_packs (skill_pack_id, worker_role, description, accepted_task_kinds,
              "references", scripts, expected_output_contract, version, created_at)
           values ($1, $2, $3, $4::jsonb, $5::jsonb, $6::jsonb, $7, $8, now())
           returning skill_pack_id, worker_role, description, accepted_task_kinds,
                     "references", scripts, created_at, expected_output_contract, version, deprecated"#,
    )
    .bind(&skill_pack_id)
    .bind(&req.worker_role)
    .bind(&req.description)
    .bind(&req.accepted_task_kinds)
    .bind(&req.references)
    .bind(&req.scripts)
    .bind(&req.expected_output_contract)
    .bind(&req.version)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    sqlx::query(
        r#"insert into event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           values ($1, 'skill_pack', $2, 'skill_pack_registered', $3, $4::jsonb, now())
           on conflict (aggregate_kind, aggregate_id, idempotency_key) do nothing"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&skill_pack_id)
    .bind(&req.idempotency_key)
    .bind(serde_json::json!({
        "skill_pack_id": skill_pack_id,
        "worker_role": req.worker_role,
        "description": req.description,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    Ok(Json(skill_pack_from_row(&row, false)?))
}

#[utoipa::path(
    get,
    path = "/api/skills/{id}",
    params(("id" = String, Path, description = "Skill pack ID")),
    responses(
        (status = 200, description = "Skill pack details", body = SkillPackResponse)
    )
)]
pub async fn get_skill_pack(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<SkillPackResponse> {
    let row = sqlx::query(
        r#"select skill_pack_id, worker_role, description, accepted_task_kinds,
                  "references", scripts, created_at, expected_output_contract,
                  version, deprecated
           from skill_packs where skill_pack_id = $1"#,
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?;

    let Some(row) = row else {
        return Err(not_found("skill pack not found"));
    };

    Ok(Json(skill_pack_from_row(&row, false)?))
}

#[utoipa::path(
    get,
    path = "/api/skills",
    responses(
        (status = 200, description = "List of skill packs", body = Vec<SkillPackResponse>)
    )
)]
pub async fn list_skill_packs(
    State(state): State<AppState>,
) -> ApiResult<Vec<SkillPackResponse>> {
    let rows = sqlx::query(
        r#"select skill_pack_id, worker_role, description, accepted_task_kinds,
                  "references", scripts, created_at, expected_output_contract,
                  version, deprecated
           from skill_packs order by created_at desc"#,
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        results.push(skill_pack_from_row(&row, false)?);
    }

    Ok(Json(results))
}

// ── Worker Templates ────────────────────────────────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateWorkerTemplateRequest {
    pub role: String,
    pub skill_pack_id: String,
    pub provider_mode: String,
    pub model_binding: String,
    pub allowed_task_kinds: Value,
    pub idempotency_key: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct WorkerTemplateResponse {
    pub template_id: String,
    pub role: String,
    pub skill_pack_id: String,
    pub provider_mode: String,
    pub model_binding: String,
    pub allowed_task_kinds: Value,
    pub created_at: String,
    pub duplicated: bool,
}

#[utoipa::path(
    post,
    path = "/api/templates",
    request_body = CreateWorkerTemplateRequest,
    responses(
        (status = 200, description = "Created worker template", body = WorkerTemplateResponse)
    )
)]
pub async fn create_worker_template(
    State(state): State<AppState>,
    Json(req): Json<CreateWorkerTemplateRequest>,
) -> ApiResult<WorkerTemplateResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    let template_id = Uuid::now_v7().to_string();

    // BND-010: scoped idempotency check
    let duplicate: Option<String> = sqlx::query_scalar(
        "select aggregate_id from event_journal where aggregate_kind = 'worker_template' and idempotency_key = $1 limit 1",
    )
    .bind(&req.idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    if let Some(existing_id) = duplicate {
        let row = sqlx::query(
            "select template_id, role, skill_pack_id, provider_mode, model_binding, allowed_task_kinds, created_at from worker_templates where template_id = $1",
        )
        .bind(&existing_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal_error)?;

        tx.commit().await.map_err(internal_error)?;

        let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;

        return Ok(Json(WorkerTemplateResponse {
            template_id: row.try_get("template_id").map_err(internal_error)?,
            role: row.try_get("role").map_err(internal_error)?,
            skill_pack_id: row.try_get("skill_pack_id").map_err(internal_error)?,
            provider_mode: row.try_get("provider_mode").map_err(internal_error)?,
            model_binding: row.try_get("model_binding").map_err(internal_error)?,
            allowed_task_kinds: row.try_get("allowed_task_kinds").map_err(internal_error)?,
            created_at: created_at.to_rfc3339(),
            duplicated: true,
        }));
    }

    let row = sqlx::query(
        r#"insert into worker_templates (template_id, role, skill_pack_id, provider_mode, model_binding, allowed_task_kinds, created_at)
           values ($1, $2, $3, $4, $5, $6::jsonb, now())
           returning template_id, role, skill_pack_id, provider_mode, model_binding, allowed_task_kinds, created_at"#,
    )
    .bind(&template_id)
    .bind(&req.role)
    .bind(&req.skill_pack_id)
    .bind(&req.provider_mode)
    .bind(&req.model_binding)
    .bind(&req.allowed_task_kinds)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    sqlx::query(
        r#"insert into event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           values ($1, 'worker_template', $2, 'worker_template_created', $3, $4::jsonb, now())
           on conflict (aggregate_kind, aggregate_id, idempotency_key) do nothing"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&template_id)
    .bind(&req.idempotency_key)
    .bind(serde_json::json!({
        "template_id": template_id,
        "role": req.role,
        "skill_pack_id": req.skill_pack_id,
        "provider_mode": req.provider_mode,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;

    Ok(Json(WorkerTemplateResponse {
        template_id: row.try_get("template_id").map_err(internal_error)?,
        role: row.try_get("role").map_err(internal_error)?,
        skill_pack_id: row.try_get("skill_pack_id").map_err(internal_error)?,
        provider_mode: row.try_get("provider_mode").map_err(internal_error)?,
        model_binding: row.try_get("model_binding").map_err(internal_error)?,
        allowed_task_kinds: row.try_get("allowed_task_kinds").map_err(internal_error)?,
        created_at: created_at.to_rfc3339(),
        duplicated: false,
    }))
}

#[utoipa::path(
    get,
    path = "/api/templates/{id}",
    params(("id" = String, Path, description = "Worker template ID")),
    responses(
        (status = 200, description = "Worker template details", body = WorkerTemplateResponse)
    )
)]
pub async fn get_worker_template(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<WorkerTemplateResponse> {
    let row = sqlx::query(
        "select template_id, role, skill_pack_id, provider_mode, model_binding, allowed_task_kinds, created_at from worker_templates where template_id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?;

    let Some(row) = row else {
        return Err(not_found("worker template not found"));
    };

    let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;

    Ok(Json(WorkerTemplateResponse {
        template_id: row.try_get("template_id").map_err(internal_error)?,
        role: row.try_get("role").map_err(internal_error)?,
        skill_pack_id: row.try_get("skill_pack_id").map_err(internal_error)?,
        provider_mode: row.try_get("provider_mode").map_err(internal_error)?,
        model_binding: row.try_get("model_binding").map_err(internal_error)?,
        allowed_task_kinds: row.try_get("allowed_task_kinds").map_err(internal_error)?,
        created_at: created_at.to_rfc3339(),
        duplicated: false,
    }))
}

#[utoipa::path(
    get,
    path = "/api/templates",
    responses(
        (status = 200, description = "List of worker templates", body = Vec<WorkerTemplateResponse>)
    )
)]
pub async fn list_worker_templates(
    State(state): State<AppState>,
) -> ApiResult<Vec<WorkerTemplateResponse>> {
    let rows = sqlx::query(
        "select template_id, role, skill_pack_id, provider_mode, model_binding, allowed_task_kinds, created_at from worker_templates order by created_at desc",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
        results.push(WorkerTemplateResponse {
            template_id: row.try_get("template_id").map_err(internal_error)?,
            role: row.try_get("role").map_err(internal_error)?,
            skill_pack_id: row.try_get("skill_pack_id").map_err(internal_error)?,
            provider_mode: row.try_get("provider_mode").map_err(internal_error)?,
            model_binding: row.try_get("model_binding").map_err(internal_error)?,
            allowed_task_kinds: row.try_get("allowed_task_kinds").map_err(internal_error)?,
            created_at: created_at.to_rfc3339(),
            duplicated: false,
        });
    }

    Ok(Json(results))
}
