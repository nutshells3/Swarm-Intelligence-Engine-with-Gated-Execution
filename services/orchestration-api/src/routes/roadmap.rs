use axum::extract::{Path, Query, State};
use axum::response::Json;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::error::{ApiResult, bad_request, internal_error, not_found};
use crate::state::AppState;

// ── Roadmap Nodes ───────────────────────────────────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateRoadmapNodeRequest {
    pub objective_id: String,
    pub title: String,
    pub track: String,
    pub idempotency_key: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct RoadmapNodeResponse {
    pub roadmap_node_id: String,
    pub title: String,
    pub track: String,
    pub status: String,
    pub created_at: String,
    pub updated_at: String,
    pub duplicated: bool,
}

#[utoipa::path(
    post,
    path = "/api/roadmap/nodes",
    request_body = CreateRoadmapNodeRequest,
    responses(
        (status = 200, description = "Created roadmap node", body = RoadmapNodeResponse)
    )
)]
pub async fn create_roadmap_node(
    State(state): State<AppState>,
    Json(req): Json<CreateRoadmapNodeRequest>,
) -> ApiResult<RoadmapNodeResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    let roadmap_node_id = Uuid::now_v7().to_string();

    // BND-010: scoped idempotency check
    let duplicate: Option<String> = sqlx::query_scalar(
        "select aggregate_id from event_journal where aggregate_kind = 'roadmap_node' and idempotency_key = $1 limit 1",
    )
    .bind(&req.idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    if let Some(existing_id) = duplicate {
        let row = sqlx::query(
            "select roadmap_node_id, title, track, status, created_at, updated_at from roadmap_nodes where roadmap_node_id = $1",
        )
        .bind(&existing_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal_error)?;

        tx.commit().await.map_err(internal_error)?;

        let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
        let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;

        return Ok(Json(RoadmapNodeResponse {
            roadmap_node_id: row.try_get("roadmap_node_id").map_err(internal_error)?,
            title: row.try_get("title").map_err(internal_error)?,
            track: row.try_get("track").map_err(internal_error)?,
            status: row.try_get("status").map_err(internal_error)?,
            created_at: created_at.to_rfc3339(),
            updated_at: updated_at.to_rfc3339(),
            duplicated: true,
        }));
    }

    // Server derives status (BND-003)
    let status = "open";

    let row = sqlx::query(
        r#"insert into roadmap_nodes (roadmap_node_id, objective_id, title, track, status, created_at, updated_at)
           values ($1, $2, $3, $4, $5, now(), now())
           returning roadmap_node_id, title, track, status, created_at, updated_at"#,
    )
    .bind(&roadmap_node_id)
    .bind(&req.objective_id)
    .bind(&req.title)
    .bind(&req.track)
    .bind(status)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    sqlx::query(
        r#"insert into event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           values ($1, 'roadmap_node', $2, 'roadmap_node_created', $3, $4::jsonb, now())
           on conflict (aggregate_kind, aggregate_id, idempotency_key) do nothing"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&roadmap_node_id)
    .bind(&req.idempotency_key)
    .bind(serde_json::json!({
        "roadmap_node_id": roadmap_node_id,
        "objective_id": req.objective_id,
        "title": req.title,
        "track": req.track,
        "status": status,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
    let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;

    Ok(Json(RoadmapNodeResponse {
        roadmap_node_id: row.try_get("roadmap_node_id").map_err(internal_error)?,
        title: row.try_get("title").map_err(internal_error)?,
        track: row.try_get("track").map_err(internal_error)?,
        status: row.try_get("status").map_err(internal_error)?,
        created_at: created_at.to_rfc3339(),
        updated_at: updated_at.to_rfc3339(),
        duplicated: false,
    }))
}

#[utoipa::path(
    get,
    path = "/api/roadmap/nodes/{id}",
    params(("id" = String, Path, description = "Roadmap node ID")),
    responses(
        (status = 200, description = "Roadmap node details", body = RoadmapNodeResponse)
    )
)]
pub async fn get_roadmap_node(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<RoadmapNodeResponse> {
    let row = sqlx::query(
        "select roadmap_node_id, title, track, status, created_at, updated_at from roadmap_nodes where roadmap_node_id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?;

    let Some(row) = row else {
        return Err(not_found("roadmap node not found"));
    };

    let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
    let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;

    Ok(Json(RoadmapNodeResponse {
        roadmap_node_id: row.try_get("roadmap_node_id").map_err(internal_error)?,
        title: row.try_get("title").map_err(internal_error)?,
        track: row.try_get("track").map_err(internal_error)?,
        status: row.try_get("status").map_err(internal_error)?,
        created_at: created_at.to_rfc3339(),
        updated_at: updated_at.to_rfc3339(),
        duplicated: false,
    }))
}

#[utoipa::path(
    get,
    path = "/api/roadmap/nodes",
    responses(
        (status = 200, description = "List of roadmap nodes", body = Vec<RoadmapNodeResponse>)
    )
)]
pub async fn list_roadmap_nodes(
    State(state): State<AppState>,
) -> ApiResult<Vec<RoadmapNodeResponse>> {
    let rows = sqlx::query(
        "select roadmap_node_id, title, track, status, created_at, updated_at from roadmap_nodes order by created_at desc",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
        let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;
        results.push(RoadmapNodeResponse {
            roadmap_node_id: row.try_get("roadmap_node_id").map_err(internal_error)?,
            title: row.try_get("title").map_err(internal_error)?,
            track: row.try_get("track").map_err(internal_error)?,
            status: row.try_get("status").map_err(internal_error)?,
            created_at: created_at.to_rfc3339(),
            updated_at: updated_at.to_rfc3339(),
            duplicated: false,
        });
    }

    Ok(Json(results))
}

// ── Roadmap Absorptions ─────────────────────────────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateAbsorptionRequest {
    pub roadmap_node_id: String,
    pub action_kind: String,
    pub source_ref: String,
    pub target_ref: Option<String>,
    pub rationale: String,
    pub idempotency_key: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct AbsorptionResponse {
    pub absorption_id: String,
    pub roadmap_node_id: String,
    pub action_kind: String,
    pub source_ref: String,
    pub target_ref: Option<String>,
    pub rationale: String,
    pub created_at: String,
    pub duplicated: bool,
}

#[utoipa::path(
    post,
    path = "/api/roadmap/absorptions",
    request_body = CreateAbsorptionRequest,
    responses(
        (status = 200, description = "Created absorption record", body = AbsorptionResponse)
    )
)]
pub async fn create_absorption(
    State(state): State<AppState>,
    Json(req): Json<CreateAbsorptionRequest>,
) -> ApiResult<AbsorptionResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    let absorption_id = Uuid::now_v7().to_string();

    // BND-010: scoped idempotency check
    let duplicate: Option<String> = sqlx::query_scalar(
        "select aggregate_id from event_journal where aggregate_kind = 'roadmap_absorption' and idempotency_key = $1 limit 1",
    )
    .bind(&req.idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    if let Some(existing_id) = duplicate {
        let row = sqlx::query(
            "select absorption_id, roadmap_node_id, action_kind, source_ref, target_ref, rationale, created_at from roadmap_absorption_records where absorption_id = $1",
        )
        .bind(&existing_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal_error)?;

        tx.commit().await.map_err(internal_error)?;

        let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;

        return Ok(Json(AbsorptionResponse {
            absorption_id: row.try_get("absorption_id").map_err(internal_error)?,
            roadmap_node_id: row.try_get("roadmap_node_id").map_err(internal_error)?,
            action_kind: row.try_get("action_kind").map_err(internal_error)?,
            source_ref: row.try_get("source_ref").map_err(internal_error)?,
            target_ref: row.try_get("target_ref").map_err(internal_error)?,
            rationale: row.try_get("rationale").map_err(internal_error)?,
            created_at: created_at.to_rfc3339(),
            duplicated: true,
        }));
    }

    let row = sqlx::query(
        r#"insert into roadmap_absorption_records (absorption_id, roadmap_node_id, action_kind, source_ref, target_ref, rationale, created_at)
           values ($1, $2, $3, $4, $5, $6, now())
           returning absorption_id, roadmap_node_id, action_kind, source_ref, target_ref, rationale, created_at"#,
    )
    .bind(&absorption_id)
    .bind(&req.roadmap_node_id)
    .bind(&req.action_kind)
    .bind(&req.source_ref)
    .bind(&req.target_ref)
    .bind(&req.rationale)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    sqlx::query(
        r#"insert into event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           values ($1, 'roadmap_absorption', $2, 'roadmap_node_absorbed', $3, $4::jsonb, now())
           on conflict (aggregate_kind, aggregate_id, idempotency_key) do nothing"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&absorption_id)
    .bind(&req.idempotency_key)
    .bind(serde_json::json!({
        "absorption_id": absorption_id,
        "roadmap_node_id": req.roadmap_node_id,
        "action_kind": req.action_kind,
        "source_ref": req.source_ref,
        "target_ref": req.target_ref,
        "rationale": req.rationale,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;

    Ok(Json(AbsorptionResponse {
        absorption_id: row.try_get("absorption_id").map_err(internal_error)?,
        roadmap_node_id: row.try_get("roadmap_node_id").map_err(internal_error)?,
        action_kind: row.try_get("action_kind").map_err(internal_error)?,
        source_ref: row.try_get("source_ref").map_err(internal_error)?,
        target_ref: row.try_get("target_ref").map_err(internal_error)?,
        rationale: row.try_get("rationale").map_err(internal_error)?,
        created_at: created_at.to_rfc3339(),
        duplicated: false,
    }))
}

#[utoipa::path(
    get,
    path = "/api/roadmap/absorptions",
    responses(
        (status = 200, description = "List of absorption records", body = Vec<AbsorptionResponse>)
    )
)]
pub async fn list_absorptions(
    State(state): State<AppState>,
) -> ApiResult<Vec<AbsorptionResponse>> {
    let rows = sqlx::query(
        "select absorption_id, roadmap_node_id, action_kind, source_ref, target_ref, rationale, created_at from roadmap_absorption_records order by created_at desc",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
        results.push(AbsorptionResponse {
            absorption_id: row.try_get("absorption_id").map_err(internal_error)?,
            roadmap_node_id: row.try_get("roadmap_node_id").map_err(internal_error)?,
            action_kind: row.try_get("action_kind").map_err(internal_error)?,
            source_ref: row.try_get("source_ref").map_err(internal_error)?,
            target_ref: row.try_get("target_ref").map_err(internal_error)?,
            rationale: row.try_get("rationale").map_err(internal_error)?,
            created_at: created_at.to_rfc3339(),
            duplicated: false,
        });
    }

    Ok(Json(results))
}

// ── Roadmap Absorb (full pipeline) ──────────────────────────────────────

const VALID_ACTION_KINDS: &[&str] = &[
    "create_node",
    "absorb_into_node",
    "reprioritize_node",
    "defer_node",
    "reject_node",
];

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AbsorbRoadmapRequest {
    pub source_ref: String,
    pub action_kind: String,
    pub target_node_id: Option<String>,
    pub rationale: String,
    /// Required for create_node action.
    pub title: Option<String>,
    /// Required for create_node action.
    pub objective_id: Option<String>,
    /// Optional track for create_node.
    pub track: Option<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct AbsorbRoadmapResponse {
    pub absorption_id: String,
    pub action_kind: String,
    pub affected_node_id: Option<String>,
    pub created_at: String,
}

/// POST /api/roadmap/absorb
///
/// Absorb a conversation/proposal into the roadmap. This executes the
/// action (create/update/reorder), creates an absorption record, emits
/// an event, and updates roadmap_ordering if needed.
#[utoipa::path(
    post,
    path = "/api/roadmap/absorb",
    request_body = AbsorbRoadmapRequest,
    responses(
        (status = 200, description = "Roadmap absorption result", body = AbsorbRoadmapResponse)
    )
)]
pub async fn absorb_roadmap(
    State(state): State<AppState>,
    Json(req): Json<AbsorbRoadmapRequest>,
) -> ApiResult<AbsorbRoadmapResponse> {
    if !VALID_ACTION_KINDS.contains(&req.action_kind.as_str()) {
        return Err(bad_request(&format!(
            "action_kind must be one of: {}",
            VALID_ACTION_KINDS.join(", ")
        )));
    }

    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    let absorption_id = Uuid::now_v7().to_string();
    let mut affected_node_id: Option<String> = req.target_node_id.clone();

    match req.action_kind.as_str() {
        "create_node" => {
            let title = req.title.as_deref().unwrap_or("Untitled node");
            let objective_id = req.objective_id.as_deref().ok_or_else(|| {
                bad_request("objective_id is required for create_node")
            })?;
            let track = req.track.as_deref().unwrap_or("default");
            let node_id = Uuid::now_v7().to_string();

            sqlx::query(
                r#"INSERT INTO roadmap_nodes (roadmap_node_id, objective_id, title, track, status, created_at, updated_at)
                   VALUES ($1, $2, $3, $4, 'open', now(), now())"#,
            )
            .bind(&node_id)
            .bind(objective_id)
            .bind(title)
            .bind(track)
            .execute(&mut *tx)
            .await
            .map_err(internal_error)?;

            // Append to roadmap_ordering for this objective
            let ordering_exists: Option<String> = sqlx::query_scalar(
                "SELECT ordering_id FROM roadmap_ordering WHERE objective_id = $1 LIMIT 1",
            )
            .bind(objective_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(internal_error)?;

            if let Some(ordering_id) = ordering_exists {
                sqlx::query(
                    r#"UPDATE roadmap_ordering
                       SET node_sequence = node_sequence || $1::jsonb, updated_at = now()
                       WHERE ordering_id = $2"#,
                )
                .bind(serde_json::json!([node_id]))
                .bind(&ordering_id)
                .execute(&mut *tx)
                .await
                .map_err(internal_error)?;
            } else {
                let oid = Uuid::now_v7().to_string();
                sqlx::query(
                    r#"INSERT INTO roadmap_ordering (ordering_id, objective_id, node_sequence, created_at, updated_at)
                       VALUES ($1, $2, $3::jsonb, now(), now())"#,
                )
                .bind(&oid)
                .bind(objective_id)
                .bind(serde_json::json!([node_id]))
                .execute(&mut *tx)
                .await
                .map_err(internal_error)?;
            }

            affected_node_id = Some(node_id);
        }
        "absorb_into_node" => {
            let target = req.target_node_id.as_deref().ok_or_else(|| {
                bad_request("target_node_id is required for absorb_into_node")
            })?;
            // Update the node's description to include the absorbed content
            sqlx::query(
                r#"UPDATE roadmap_nodes
                   SET description = description || E'\n\n[Absorbed] ' || $1,
                       revision = revision + 1,
                       updated_at = now()
                   WHERE roadmap_node_id = $2"#,
            )
            .bind(&req.rationale)
            .bind(target)
            .execute(&mut *tx)
            .await
            .map_err(internal_error)?;
        }
        "reprioritize_node" => {
            let target = req.target_node_id.as_deref().ok_or_else(|| {
                bad_request("target_node_id is required for reprioritize_node")
            })?;
            sqlx::query(
                r#"UPDATE roadmap_nodes
                   SET priority = priority + 1,
                       revision = revision + 1,
                       updated_at = now()
                   WHERE roadmap_node_id = $1"#,
            )
            .bind(target)
            .execute(&mut *tx)
            .await
            .map_err(internal_error)?;
        }
        "defer_node" => {
            let target = req.target_node_id.as_deref().ok_or_else(|| {
                bad_request("target_node_id is required for defer_node")
            })?;
            sqlx::query(
                r#"UPDATE roadmap_nodes
                   SET status = 'deferred',
                       revision = revision + 1,
                       updated_at = now()
                   WHERE roadmap_node_id = $1"#,
            )
            .bind(target)
            .execute(&mut *tx)
            .await
            .map_err(internal_error)?;
        }
        "reject_node" => {
            let target = req.target_node_id.as_deref().ok_or_else(|| {
                bad_request("target_node_id is required for reject_node")
            })?;
            sqlx::query(
                r#"UPDATE roadmap_nodes
                   SET status = 'rejected',
                       revision = revision + 1,
                       updated_at = now()
                   WHERE roadmap_node_id = $1"#,
            )
            .bind(target)
            .execute(&mut *tx)
            .await
            .map_err(internal_error)?;
        }
        _ => unreachable!(),
    }

    // Create absorption record in roadmap_absorption_records
    let target_ref = affected_node_id.clone().unwrap_or_default();
    sqlx::query(
        r#"INSERT INTO roadmap_absorption_records (absorption_id, roadmap_node_id, action_kind, source_ref, target_ref, rationale, created_at)
           VALUES ($1, $2, $3, $4, $5, $6, now())"#,
    )
    .bind(&absorption_id)
    .bind(&target_ref)
    .bind(&req.action_kind)
    .bind(&req.source_ref)
    .bind(&target_ref)
    .bind(&req.rationale)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    // RMS-005: Emit action-specific event kind for defer/reject
    let event_kind = match req.action_kind.as_str() {
        "defer_node" => "roadmap_node_deferred",
        "reject_node" => "roadmap_node_rejected",
        "create_node" => "roadmap_node_created",
        "reprioritize_node" => "roadmap_reprioritized",
        _ => "roadmap_absorbed",
    };

    sqlx::query(
        r#"INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           VALUES ($1, 'roadmap_absorption', $2, $3, $4, $5::jsonb, now())
           ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&absorption_id)
    .bind(event_kind)
    .bind(&format!("absorb_{}", absorption_id))
    .bind(serde_json::json!({
        "absorption_id": absorption_id,
        "action_kind": req.action_kind,
        "event_kind": event_kind,
        "source_ref": req.source_ref,
        "affected_node_id": affected_node_id,
        "rationale": req.rationale,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    Ok(Json(AbsorbRoadmapResponse {
        absorption_id,
        action_kind: req.action_kind,
        affected_node_id,
        created_at: chrono::Utc::now().to_rfc3339(),
    }))
}

// ── Roadmap Reorder ─────────────────────────────────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ReorderRoadmapRequest {
    pub objective_id: String,
    pub node_sequence: Vec<String>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ReorderRoadmapResponse {
    pub objective_id: String,
    pub node_sequence: Vec<String>,
    pub updated: bool,
}

/// POST /api/roadmap/reorder
#[utoipa::path(
    post,
    path = "/api/roadmap/reorder",
    request_body = ReorderRoadmapRequest,
    responses(
        (status = 200, description = "Reorder result", body = ReorderRoadmapResponse)
    )
)]
pub async fn reorder_roadmap(
    State(state): State<AppState>,
    Json(req): Json<ReorderRoadmapRequest>,
) -> ApiResult<ReorderRoadmapResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;

    let sequence_json = serde_json::json!(req.node_sequence);

    // Upsert roadmap_ordering
    let existing: Option<String> = sqlx::query_scalar(
        "SELECT ordering_id FROM roadmap_ordering WHERE objective_id = $1 LIMIT 1",
    )
    .bind(&req.objective_id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    if let Some(ordering_id) = existing {
        sqlx::query(
            r#"UPDATE roadmap_ordering
               SET node_sequence = $1::jsonb, updated_at = now()
               WHERE ordering_id = $2"#,
        )
        .bind(&sequence_json)
        .bind(&ordering_id)
        .execute(&mut *tx)
        .await
        .map_err(internal_error)?;
    } else {
        let oid = Uuid::now_v7().to_string();
        sqlx::query(
            r#"INSERT INTO roadmap_ordering (ordering_id, objective_id, node_sequence, created_at, updated_at)
               VALUES ($1, $2, $3::jsonb, now(), now())"#,
        )
        .bind(&oid)
        .bind(&req.objective_id)
        .bind(&sequence_json)
        .execute(&mut *tx)
        .await
        .map_err(internal_error)?;
    }

    // Emit event
    sqlx::query(
        r#"INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           VALUES ($1, 'roadmap_ordering', $2, 'roadmap_reordered', $3, $4::jsonb, now())
           ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&req.objective_id)
    .bind(&format!("reorder_{}_{}", req.objective_id, Uuid::now_v7()))
    .bind(serde_json::json!({
        "objective_id": req.objective_id,
        "node_sequence": req.node_sequence,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    Ok(Json(ReorderRoadmapResponse {
        objective_id: req.objective_id,
        node_sequence: req.node_sequence,
        updated: true,
    }))
}

// ── Roadmap Track Change ────────────────────────────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub struct ChangeTrackRequest {
    pub track: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct ChangeTrackResponse {
    pub roadmap_node_id: String,
    pub title: String,
    pub track: String,
    pub status: String,
    pub updated_at: String,
    pub updated: bool,
}

/// PATCH /api/roadmap/nodes/{id}/track
///
/// RMS-004 / RMS-009: Updates roadmap_nodes.track, emits both
/// `roadmap_track_changed` and `roadmap_reprioritized` events,
/// and returns the full updated node.
#[utoipa::path(
    patch,
    path = "/api/roadmap/nodes/{id}/track",
    params(("id" = String, Path, description = "Roadmap node ID")),
    request_body = ChangeTrackRequest,
    responses(
        (status = 200, description = "Track change result", body = ChangeTrackResponse)
    )
)]
pub async fn change_track(
    State(state): State<AppState>,
    Path(id): Path<String>,
    Json(req): Json<ChangeTrackRequest>,
) -> ApiResult<ChangeTrackResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;

    // Fetch old track so we can include it in the event payload
    let old_row = sqlx::query(
        "SELECT roadmap_node_id, track FROM roadmap_nodes WHERE roadmap_node_id = $1",
    )
    .bind(&id)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    let Some(old_row) = old_row else {
        return Err(not_found("roadmap node not found"));
    };
    let old_track: String = old_row.try_get("track").map_err(internal_error)?;

    // Update the track
    let updated_row = sqlx::query(
        r#"UPDATE roadmap_nodes
           SET track = $1, revision = revision + 1, updated_at = now()
           WHERE roadmap_node_id = $2
           RETURNING roadmap_node_id, title, track, status, updated_at"#,
    )
    .bind(&req.track)
    .bind(&id)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    let updated_at: chrono::DateTime<chrono::Utc> =
        updated_row.try_get("updated_at").map_err(internal_error)?;

    // Emit roadmap_track_changed event
    sqlx::query(
        r#"INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           VALUES ($1, 'roadmap_node', $2, 'roadmap_track_changed', $3, $4::jsonb, now())
           ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&id)
    .bind(&format!("track_change_{}_{}", id, Uuid::now_v7()))
    .bind(serde_json::json!({
        "roadmap_node_id": id,
        "old_track": old_track,
        "new_track": req.track,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    // RMS-009: Also emit roadmap_reprioritized event
    sqlx::query(
        r#"INSERT INTO event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           VALUES ($1, 'roadmap_node', $2, 'roadmap_reprioritized', $3, $4::jsonb, now())
           ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&id)
    .bind(&format!("reprioritize_{}_{}", id, Uuid::now_v7()))
    .bind(serde_json::json!({
        "roadmap_node_id": id,
        "change_type": "track_change",
        "old_track": old_track,
        "new_track": req.track,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    Ok(Json(ChangeTrackResponse {
        roadmap_node_id: updated_row.try_get("roadmap_node_id").map_err(internal_error)?,
        title: updated_row.try_get("title").map_err(internal_error)?,
        track: updated_row.try_get("track").map_err(internal_error)?,
        status: updated_row.try_get("status").map_err(internal_error)?,
        updated_at: updated_at.to_rfc3339(),
        updated: true,
    }))
}

// ── RMS-010: Roadmap projection endpoint ─────────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub struct RoadmapProjectionQuery {
    /// Filter by objective_id (optional).
    pub objective_id: Option<String>,
}

/// A roadmap node with its ordering position.
#[derive(Serialize, utoipa::ToSchema)]
pub struct RoadmapProjectionNode {
    pub roadmap_node_id: String,
    pub title: String,
    pub track: String,
    pub status: String,
    pub objective_id: String,
    /// Position in the ordering sequence (null if not in any ordering).
    pub ordering_position: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

/// A group of nodes belonging to the same track, in priority order.
#[derive(Serialize, utoipa::ToSchema)]
pub struct TrackGroup {
    pub track: String,
    pub nodes: Vec<RoadmapProjectionNode>,
    pub count: i64,
}

/// Complete roadmap projection response.
#[derive(Serialize, utoipa::ToSchema)]
pub struct RoadmapProjectionResponse {
    pub nodes: Vec<RoadmapProjectionNode>,
    /// Nodes grouped by track with priority ordering within each group.
    pub tracks: Vec<TrackGroup>,
    pub total_count: i64,
    pub open_count: i64,
    pub deferred_count: i64,
    pub rejected_count: i64,
}

/// GET /api/projections/roadmap
///
/// RMS-010: Returns roadmap nodes with ordering and status.
/// Nodes are returned sorted by their ordering position within each
/// objective, falling back to creation time for unordered nodes.
#[utoipa::path(
    get,
    path = "/api/projections/roadmap",
    responses(
        (status = 200, description = "Roadmap projection", body = RoadmapProjectionResponse)
    )
)]
pub async fn roadmap_projection(
    State(state): State<AppState>,
    Query(query): Query<RoadmapProjectionQuery>,
) -> ApiResult<RoadmapProjectionResponse> {
    // Fetch nodes with optional objective_id filter
    let nodes = if let Some(ref obj_id) = query.objective_id {
        sqlx::query(
            r#"SELECT rn.roadmap_node_id, rn.title, rn.track, rn.status,
                      rn.objective_id, rn.created_at, rn.updated_at
               FROM roadmap_nodes rn
               WHERE rn.objective_id = $1
               ORDER BY rn.created_at"#,
        )
        .bind(obj_id)
        .fetch_all(&state.pool)
        .await
        .map_err(internal_error)?
    } else {
        sqlx::query(
            r#"SELECT rn.roadmap_node_id, rn.title, rn.track, rn.status,
                      rn.objective_id, rn.created_at, rn.updated_at
               FROM roadmap_nodes rn
               ORDER BY rn.created_at"#,
        )
        .fetch_all(&state.pool)
        .await
        .map_err(internal_error)?
    };

    // Fetch ordering sequences to compute positions
    let orderings = if let Some(ref obj_id) = query.objective_id {
        sqlx::query(
            "SELECT objective_id, node_sequence FROM roadmap_ordering WHERE objective_id = $1",
        )
        .bind(obj_id)
        .fetch_all(&state.pool)
        .await
        .map_err(internal_error)?
    } else {
        sqlx::query("SELECT objective_id, node_sequence FROM roadmap_ordering")
            .fetch_all(&state.pool)
            .await
            .map_err(internal_error)?
    };

    // Build a map: node_id -> position
    let mut position_map = std::collections::HashMap::<String, i64>::new();
    for ord_row in &orderings {
        let seq: serde_json::Value = ord_row.try_get("node_sequence").map_err(internal_error)?;
        if let Some(arr) = seq.as_array() {
            for (idx, val) in arr.iter().enumerate() {
                if let Some(nid) = val.as_str() {
                    position_map.insert(nid.to_string(), idx as i64);
                }
            }
        }
    }

    let mut result_nodes = Vec::with_capacity(nodes.len());
    let mut open_count: i64 = 0;
    let mut deferred_count: i64 = 0;
    let mut rejected_count: i64 = 0;

    for row in &nodes {
        let node_id: String = row.try_get("roadmap_node_id").map_err(internal_error)?;
        let status: String = row.try_get("status").map_err(internal_error)?;
        let created_at: chrono::DateTime<chrono::Utc> =
            row.try_get("created_at").map_err(internal_error)?;
        let updated_at: chrono::DateTime<chrono::Utc> =
            row.try_get("updated_at").map_err(internal_error)?;

        match status.as_str() {
            "open" => open_count += 1,
            "deferred" => deferred_count += 1,
            "rejected" => rejected_count += 1,
            _ => {}
        }

        let ordering_position = position_map.get(&node_id).copied();

        result_nodes.push(RoadmapProjectionNode {
            roadmap_node_id: node_id,
            title: row.try_get("title").map_err(internal_error)?,
            track: row.try_get("track").map_err(internal_error)?,
            status,
            objective_id: row.try_get("objective_id").map_err(internal_error)?,
            ordering_position,
            created_at: created_at.to_rfc3339(),
            updated_at: updated_at.to_rfc3339(),
        });
    }

    // Sort by ordering position (nodes with position first, then by created_at)
    result_nodes.sort_by(|a, b| {
        match (a.ordering_position, b.ordering_position) {
            (Some(pa), Some(pb)) => pa.cmp(&pb),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.created_at.cmp(&b.created_at),
        }
    });

    let total_count = result_nodes.len() as i64;

    // RMS-010: Build track-grouped view
    let mut track_map = std::collections::BTreeMap::<String, Vec<RoadmapProjectionNode>>::new();
    for node in &result_nodes {
        track_map
            .entry(node.track.clone())
            .or_default()
            .push(RoadmapProjectionNode {
                roadmap_node_id: node.roadmap_node_id.clone(),
                title: node.title.clone(),
                track: node.track.clone(),
                status: node.status.clone(),
                objective_id: node.objective_id.clone(),
                ordering_position: node.ordering_position,
                created_at: node.created_at.clone(),
                updated_at: node.updated_at.clone(),
            });
    }
    let tracks: Vec<TrackGroup> = track_map
        .into_iter()
        .map(|(track, nodes)| {
            let count = nodes.len() as i64;
            TrackGroup { track, nodes, count }
        })
        .collect();

    Ok(Json(RoadmapProjectionResponse {
        nodes: result_nodes,
        tracks,
        total_count,
        open_count,
        deferred_count,
        rejected_count,
    }))
}
