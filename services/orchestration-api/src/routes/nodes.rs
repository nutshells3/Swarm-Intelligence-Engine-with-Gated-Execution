use axum::extract::{Path, State};
use axum::response::Json;
use serde::{Deserialize, Serialize};
use sqlx::Row;
use uuid::Uuid;

use crate::error::{ApiResult, internal_error, not_found};
use crate::state::AppState;

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateNodeRequest {
    pub objective_id: String,
    pub title: String,
    pub statement: String,
    pub lane: String,
    pub idempotency_key: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct NodeResponse {
    pub node_id: String,
    pub objective_id: String,
    pub title: String,
    pub statement: String,
    pub lane: String,
    pub lifecycle: String,
    pub created_at: String,
    pub updated_at: String,
    pub duplicated: bool,
}

#[utoipa::path(
    post,
    path = "/api/nodes",
    request_body = CreateNodeRequest,
    responses(
        (status = 200, description = "Created node", body = NodeResponse)
    )
)]
pub async fn create_node(
    State(state): State<AppState>,
    Json(req): Json<CreateNodeRequest>,
) -> ApiResult<NodeResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    let node_id = Uuid::now_v7().to_string();

    // Scoped idempotency check
    let duplicate: Option<String> = sqlx::query_scalar(
        "select aggregate_id from event_journal where aggregate_kind = 'node' and idempotency_key = $1 limit 1",
    )
    .bind(&req.idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    if let Some(existing_id) = duplicate {
        let row = sqlx::query(
            "select node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at from nodes where node_id = $1",
        )
        .bind(&existing_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal_error)?;

        tx.commit().await.map_err(internal_error)?;

        let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
        let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;

        return Ok(Json(NodeResponse {
            node_id: row.try_get("node_id").map_err(internal_error)?,
            objective_id: row.try_get("objective_id").map_err(internal_error)?,
            title: row.try_get("title").map_err(internal_error)?,
            statement: row.try_get("statement").map_err(internal_error)?,
            lane: row.try_get("lane").map_err(internal_error)?,
            lifecycle: row.try_get("lifecycle").map_err(internal_error)?,
            created_at: created_at.to_rfc3339(),
            updated_at: updated_at.to_rfc3339(),
            duplicated: true,
        }));
    }

    // Server derives lifecycle (BND-003)
    let lifecycle = "proposed";

    let row = sqlx::query(
        r#"insert into nodes (node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at)
           values ($1, $2, $3, $4, $5, $6, now(), now())
           returning node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at"#,
    )
    .bind(&node_id)
    .bind(&req.objective_id)
    .bind(&req.title)
    .bind(&req.statement)
    .bind(&req.lane)
    .bind(lifecycle)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    sqlx::query(
        r#"insert into event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           values ($1, 'node', $2, 'roadmap_node_created', $3, $4::jsonb, now())
           on conflict (aggregate_kind, aggregate_id, idempotency_key) do nothing"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&node_id)
    .bind(&req.idempotency_key)
    .bind(serde_json::json!({
        "node_id": node_id,
        "objective_id": req.objective_id,
        "title": req.title,
        "lane": req.lane,
        "lifecycle": lifecycle,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
    let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;

    Ok(Json(NodeResponse {
        node_id: row.try_get("node_id").map_err(internal_error)?,
        objective_id: row.try_get("objective_id").map_err(internal_error)?,
        title: row.try_get("title").map_err(internal_error)?,
        statement: row.try_get("statement").map_err(internal_error)?,
        lane: row.try_get("lane").map_err(internal_error)?,
        lifecycle: row.try_get("lifecycle").map_err(internal_error)?,
        created_at: created_at.to_rfc3339(),
        updated_at: updated_at.to_rfc3339(),
        duplicated: false,
    }))
}

#[utoipa::path(
    get,
    path = "/api/nodes/{id}",
    params(("id" = String, Path, description = "Node ID")),
    responses(
        (status = 200, description = "Node details", body = NodeResponse)
    )
)]
pub async fn get_node(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> ApiResult<NodeResponse> {
    let row = sqlx::query(
        "select node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at from nodes where node_id = $1",
    )
    .bind(&id)
    .fetch_optional(&state.pool)
    .await
    .map_err(internal_error)?;

    let Some(row) = row else {
        return Err(not_found("node not found"));
    };

    let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
    let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;

    Ok(Json(NodeResponse {
        node_id: row.try_get("node_id").map_err(internal_error)?,
        objective_id: row.try_get("objective_id").map_err(internal_error)?,
        title: row.try_get("title").map_err(internal_error)?,
        statement: row.try_get("statement").map_err(internal_error)?,
        lane: row.try_get("lane").map_err(internal_error)?,
        lifecycle: row.try_get("lifecycle").map_err(internal_error)?,
        created_at: created_at.to_rfc3339(),
        updated_at: updated_at.to_rfc3339(),
        duplicated: false,
    }))
}

#[utoipa::path(
    get,
    path = "/api/nodes",
    responses(
        (status = 200, description = "List of nodes", body = Vec<NodeResponse>)
    )
)]
pub async fn list_nodes(
    State(state): State<AppState>,
) -> ApiResult<Vec<NodeResponse>> {
    let rows = sqlx::query(
        "select node_id, objective_id, title, statement, lane, lifecycle, created_at, updated_at from nodes order by created_at desc",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let created_at: chrono::DateTime<chrono::Utc> = row.try_get("created_at").map_err(internal_error)?;
        let updated_at: chrono::DateTime<chrono::Utc> = row.try_get("updated_at").map_err(internal_error)?;
        results.push(NodeResponse {
            node_id: row.try_get("node_id").map_err(internal_error)?,
            objective_id: row.try_get("objective_id").map_err(internal_error)?,
            title: row.try_get("title").map_err(internal_error)?,
            statement: row.try_get("statement").map_err(internal_error)?,
            lane: row.try_get("lane").map_err(internal_error)?,
            lifecycle: row.try_get("lifecycle").map_err(internal_error)?,
            created_at: created_at.to_rfc3339(),
            updated_at: updated_at.to_rfc3339(),
            duplicated: false,
        });
    }

    Ok(Json(results))
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct CreateNodeEdgeRequest {
    pub from_node_id: String,
    pub to_node_id: String,
    pub edge_kind: String,
    pub idempotency_key: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct NodeEdgeResponse {
    pub edge_id: String,
    pub from_node_id: String,
    pub to_node_id: String,
    pub edge_kind: String,
    pub duplicated: bool,
}

#[utoipa::path(
    post,
    path = "/api/node-edges",
    request_body = CreateNodeEdgeRequest,
    responses(
        (status = 200, description = "Created node edge", body = NodeEdgeResponse)
    )
)]
pub async fn create_node_edge(
    State(state): State<AppState>,
    Json(req): Json<CreateNodeEdgeRequest>,
) -> ApiResult<NodeEdgeResponse> {
    let mut tx = state.pool.begin().await.map_err(internal_error)?;
    let edge_id = Uuid::now_v7().to_string();

    // Scoped idempotency check
    let duplicate: Option<String> = sqlx::query_scalar(
        "select aggregate_id from event_journal where aggregate_kind = 'node_edge' and idempotency_key = $1 limit 1",
    )
    .bind(&req.idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .map_err(internal_error)?;

    if let Some(existing_id) = duplicate {
        let row = sqlx::query(
            "select edge_id, from_node_id, to_node_id, edge_kind from node_edges where edge_id = $1",
        )
        .bind(&existing_id)
        .fetch_one(&mut *tx)
        .await
        .map_err(internal_error)?;

        tx.commit().await.map_err(internal_error)?;

        return Ok(Json(NodeEdgeResponse {
            edge_id: row.try_get("edge_id").map_err(internal_error)?,
            from_node_id: row.try_get("from_node_id").map_err(internal_error)?,
            to_node_id: row.try_get("to_node_id").map_err(internal_error)?,
            edge_kind: row.try_get("edge_kind").map_err(internal_error)?,
            duplicated: true,
        }));
    }

    let row = sqlx::query(
        r#"insert into node_edges (edge_id, from_node_id, to_node_id, edge_kind)
           values ($1, $2, $3, $4)
           returning edge_id, from_node_id, to_node_id, edge_kind"#,
    )
    .bind(&edge_id)
    .bind(&req.from_node_id)
    .bind(&req.to_node_id)
    .bind(&req.edge_kind)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    sqlx::query(
        r#"insert into event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           values ($1, 'node_edge', $2, 'node_edge_created', $3, $4::jsonb, now())
           on conflict (aggregate_kind, aggregate_id, idempotency_key) do nothing"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&edge_id)
    .bind(&req.idempotency_key)
    .bind(serde_json::json!({
        "edge_id": edge_id,
        "from_node_id": req.from_node_id,
        "to_node_id": req.to_node_id,
        "edge_kind": req.edge_kind,
    }))
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    Ok(Json(NodeEdgeResponse {
        edge_id: row.try_get("edge_id").map_err(internal_error)?,
        from_node_id: row.try_get("from_node_id").map_err(internal_error)?,
        to_node_id: row.try_get("to_node_id").map_err(internal_error)?,
        edge_kind: row.try_get("edge_kind").map_err(internal_error)?,
        duplicated: false,
    }))
}

#[utoipa::path(
    get,
    path = "/api/node-edges",
    responses(
        (status = 200, description = "List of node edges", body = Vec<NodeEdgeResponse>)
    )
)]
pub async fn list_node_edges(
    State(state): State<AppState>,
) -> ApiResult<Vec<NodeEdgeResponse>> {
    let rows = sqlx::query(
        "select edge_id, from_node_id, to_node_id, edge_kind from node_edges",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        results.push(NodeEdgeResponse {
            edge_id: row.try_get("edge_id").map_err(internal_error)?,
            from_node_id: row.try_get("from_node_id").map_err(internal_error)?,
            to_node_id: row.try_get("to_node_id").map_err(internal_error)?,
            edge_kind: row.try_get("edge_kind").map_err(internal_error)?,
            duplicated: false,
        });
    }

    Ok(Json(results))
}
