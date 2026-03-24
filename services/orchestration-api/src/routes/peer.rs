use axum::extract::{Path, Query, State};
use axum::response::Json;
use axum::response::sse::{Event, Sse};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::Row;
use std::convert::Infallible;
use uuid::Uuid;

use crate::error::{ApiResult, bad_request, internal_error};
use crate::state::AppState;

// ── Request / response types ─────────────────────────────────────────────

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SendPeerMessageRequest {
    pub from_task_id: String,
    pub to_task_id: Option<String>,
    pub topic: String,
    pub kind: String,
    pub payload: Option<Value>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct PeerMessageResponse {
    pub message_id: String,
    pub from_task_id: String,
    pub to_task_id: Option<String>,
    pub topic: String,
    pub kind: String,
    pub payload: Value,
    pub created_at: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PeerMessageQuery {
    pub topic: Option<String>,
    pub since: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct AckRequest {
    pub acknowledged_by: String,
    pub response: Option<Value>,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct AckResponse {
    pub ack_id: String,
    pub message_id: String,
    pub acknowledged_by: String,
    pub response: Option<Value>,
    pub created_at: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct SubscribeRequest {
    pub subscriber_task_id: String,
    pub topic: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct SubscriptionResponse {
    pub subscription_id: String,
    pub subscriber_task_id: String,
    pub topic: String,
    pub created_at: String,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct UnsubscribeRequest {
    pub subscriber_task_id: String,
    pub topic: String,
}

#[derive(Serialize, utoipa::ToSchema)]
pub struct TopicSummary {
    pub topic: String,
    pub message_count: i64,
    pub latest_at: Option<String>,
}

#[derive(Deserialize, utoipa::ToSchema)]
pub struct PeerStreamQuery {
    pub topic: String,
}

// ── POST /api/peer/messages -- send a peer message ────────────────────────

#[utoipa::path(
    post,
    path = "/api/peer/messages",
    request_body = SendPeerMessageRequest,
    responses(
        (status = 200, description = "Sent peer message", body = PeerMessageResponse)
    )
)]
pub async fn send_peer_message(
    State(state): State<AppState>,
    Json(req): Json<SendPeerMessageRequest>,
) -> ApiResult<PeerMessageResponse> {
    if req.topic.is_empty() {
        return Err(bad_request("topic must not be empty"));
    }
    if req.from_task_id.is_empty() {
        return Err(bad_request("from_task_id must not be empty"));
    }

    let message_id = Uuid::now_v7().to_string();
    let payload = req.payload.unwrap_or(serde_json::json!({}));

    let mut tx = state.pool.begin().await.map_err(internal_error)?;

    // Insert into peer_messages
    let row = sqlx::query(
        r#"insert into peer_messages (message_id, from_task_id, to_task_id, topic, kind, payload, created_at)
           values ($1, $2, $3, $4, $5, $6::jsonb, now())
           returning message_id, from_task_id, to_task_id, topic, kind, payload, created_at"#,
    )
    .bind(&message_id)
    .bind(&req.from_task_id)
    .bind(&req.to_task_id)
    .bind(&req.topic)
    .bind(&req.kind)
    .bind(&payload)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Record in event_journal for reproducibility
    let event_payload = serde_json::json!({
        "message_id": message_id,
        "from_task_id": req.from_task_id,
        "to_task_id": req.to_task_id,
        "topic": req.topic,
        "kind": req.kind,
        "payload": payload,
    });

    sqlx::query(
        r#"insert into event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           values ($1, 'peer_message', $2, 'peer_message_sent', $3, $4::jsonb, now())"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&message_id)
    .bind(&format!("peer_msg_{}", message_id))
    .bind(&event_payload)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    let created_at: chrono::DateTime<chrono::Utc> =
        row.try_get("created_at").map_err(internal_error)?;

    Ok(Json(PeerMessageResponse {
        message_id: row.try_get("message_id").map_err(internal_error)?,
        from_task_id: row.try_get("from_task_id").map_err(internal_error)?,
        to_task_id: row.try_get("to_task_id").map_err(internal_error)?,
        topic: row.try_get("topic").map_err(internal_error)?,
        kind: row.try_get("kind").map_err(internal_error)?,
        payload: row.try_get("payload").map_err(internal_error)?,
        created_at: created_at.to_rfc3339(),
    }))
}

// ── GET /api/peer/messages?topic={topic}&since={timestamp} ───────────────

#[utoipa::path(
    get,
    path = "/api/peer/messages",
    responses(
        (status = 200, description = "List of peer messages", body = Vec<PeerMessageResponse>)
    )
)]
pub async fn list_peer_messages(
    State(state): State<AppState>,
    Query(params): Query<PeerMessageQuery>,
) -> ApiResult<Vec<PeerMessageResponse>> {
    let rows = match (&params.topic, &params.since) {
        (Some(topic), Some(since)) => {
            sqlx::query(
                "select message_id, from_task_id, to_task_id, topic, kind, payload, created_at \
                 from peer_messages \
                 where topic = $1 and created_at > $2::timestamptz \
                 order by created_at asc",
            )
            .bind(topic)
            .bind(since)
            .fetch_all(&state.pool)
            .await
        }
        (Some(topic), None) => {
            sqlx::query(
                "select message_id, from_task_id, to_task_id, topic, kind, payload, created_at \
                 from peer_messages \
                 where topic = $1 \
                 order by created_at desc \
                 limit 100",
            )
            .bind(topic)
            .fetch_all(&state.pool)
            .await
        }
        (None, Some(since)) => {
            sqlx::query(
                "select message_id, from_task_id, to_task_id, topic, kind, payload, created_at \
                 from peer_messages \
                 where created_at > $1::timestamptz \
                 order by created_at asc \
                 limit 100",
            )
            .bind(since)
            .fetch_all(&state.pool)
            .await
        }
        (None, None) => {
            sqlx::query(
                "select message_id, from_task_id, to_task_id, topic, kind, payload, created_at \
                 from peer_messages \
                 order by created_at desc \
                 limit 100",
            )
            .fetch_all(&state.pool)
            .await
        }
    }
    .map_err(internal_error)?;

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let created_at: chrono::DateTime<chrono::Utc> =
            row.try_get("created_at").map_err(internal_error)?;
        results.push(PeerMessageResponse {
            message_id: row.try_get("message_id").map_err(internal_error)?,
            from_task_id: row.try_get("from_task_id").map_err(internal_error)?,
            to_task_id: row.try_get("to_task_id").map_err(internal_error)?,
            topic: row.try_get("topic").map_err(internal_error)?,
            kind: row.try_get("kind").map_err(internal_error)?,
            payload: row.try_get("payload").map_err(internal_error)?,
            created_at: created_at.to_rfc3339(),
        });
    }

    Ok(Json(results))
}

// ── POST /api/peer/messages/{message_id}/ack — acknowledge receipt ───────

#[utoipa::path(
    post,
    path = "/api/peer/messages/{message_id}/ack",
    params(("message_id" = String, Path, description = "Peer message ID")),
    responses(
        (status = 200, description = "Acknowledged message", body = AckResponse)
    )
)]
pub async fn ack_peer_message(
    State(state): State<AppState>,
    Path(message_id): Path<String>,
    Json(req): Json<AckRequest>,
) -> ApiResult<AckResponse> {
    let ack_id = Uuid::now_v7().to_string();

    let mut tx = state.pool.begin().await.map_err(internal_error)?;

    let row = sqlx::query(
        r#"insert into peer_message_acks (ack_id, message_id, acknowledged_by, response, created_at)
           values ($1, $2, $3, $4::jsonb, now())
           returning ack_id, message_id, acknowledged_by, response, created_at"#,
    )
    .bind(&ack_id)
    .bind(&message_id)
    .bind(&req.acknowledged_by)
    .bind(&req.response)
    .fetch_one(&mut *tx)
    .await
    .map_err(internal_error)?;

    // Record acknowledgement in event_journal
    let event_payload = serde_json::json!({
        "ack_id": ack_id,
        "message_id": message_id,
        "acknowledged_by": req.acknowledged_by,
        "response": req.response,
    });

    sqlx::query(
        r#"insert into event_journal (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
           values ($1, 'peer_message', $2, 'peer_message_acknowledged', $3, $4::jsonb, now())"#,
    )
    .bind(Uuid::now_v7().to_string())
    .bind(&message_id)
    .bind(&format!("peer_ack_{}", ack_id))
    .bind(&event_payload)
    .execute(&mut *tx)
    .await
    .map_err(internal_error)?;

    tx.commit().await.map_err(internal_error)?;

    let created_at: chrono::DateTime<chrono::Utc> =
        row.try_get("created_at").map_err(internal_error)?;

    Ok(Json(AckResponse {
        ack_id: row.try_get("ack_id").map_err(internal_error)?,
        message_id: row.try_get("message_id").map_err(internal_error)?,
        acknowledged_by: row.try_get("acknowledged_by").map_err(internal_error)?,
        response: row.try_get("response").map_err(internal_error)?,
        created_at: created_at.to_rfc3339(),
    }))
}

// ── POST /api/peer/subscribe — subscribe to a topic ──────────────────────

#[utoipa::path(
    post,
    path = "/api/peer/subscribe",
    responses(
        (status = 200, description = "Subscribed to topic", body = SubscriptionResponse)
    )
)]
pub async fn subscribe(
    State(state): State<AppState>,
    Json(req): Json<SubscribeRequest>,
) -> ApiResult<SubscriptionResponse> {
    if req.topic.is_empty() {
        return Err(bad_request("topic must not be empty"));
    }

    let subscription_id = Uuid::now_v7().to_string();

    let row = sqlx::query(
        r#"insert into peer_subscriptions (subscription_id, subscriber_task_id, topic, created_at)
           values ($1, $2, $3, now())
           on conflict (subscriber_task_id, topic) do update set created_at = now()
           returning subscription_id, subscriber_task_id, topic, created_at"#,
    )
    .bind(&subscription_id)
    .bind(&req.subscriber_task_id)
    .bind(&req.topic)
    .fetch_one(&state.pool)
    .await
    .map_err(internal_error)?;

    let created_at: chrono::DateTime<chrono::Utc> =
        row.try_get("created_at").map_err(internal_error)?;

    Ok(Json(SubscriptionResponse {
        subscription_id: row.try_get("subscription_id").map_err(internal_error)?,
        subscriber_task_id: row.try_get("subscriber_task_id").map_err(internal_error)?,
        topic: row.try_get("topic").map_err(internal_error)?,
        created_at: created_at.to_rfc3339(),
    }))
}

// ── DELETE /api/peer/subscribe — unsubscribe from a topic ────────────────

#[utoipa::path(
    delete,
    path = "/api/peer/subscribe",
    responses(
        (status = 204, description = "Unsubscribed from topic")
    )
)]
pub async fn unsubscribe(
    State(state): State<AppState>,
    Json(req): Json<UnsubscribeRequest>,
) -> Result<axum::http::StatusCode, (axum::http::StatusCode, String)> {
    sqlx::query(
        "delete from peer_subscriptions where subscriber_task_id = $1 and topic = $2",
    )
    .bind(&req.subscriber_task_id)
    .bind(&req.topic)
    .execute(&state.pool)
    .await
    .map_err(internal_error)?;

    Ok(axum::http::StatusCode::NO_CONTENT)
}

// ── GET /api/peer/topics — list active topics with message counts ────────

#[utoipa::path(
    get,
    path = "/api/peer/topics",
    responses(
        (status = 200, description = "List of active topics", body = Vec<TopicSummary>)
    )
)]
pub async fn list_topics(
    State(state): State<AppState>,
) -> ApiResult<Vec<TopicSummary>> {
    let rows = sqlx::query(
        "select topic, count(*) as message_count, max(created_at) as latest_at \
         from peer_messages \
         group by topic \
         order by latest_at desc \
         limit 100",
    )
    .fetch_all(&state.pool)
    .await
    .map_err(internal_error)?;

    let mut results = Vec::with_capacity(rows.len());
    for row in rows {
        let latest_at: Option<chrono::DateTime<chrono::Utc>> =
            row.try_get("latest_at").map_err(internal_error)?;
        results.push(TopicSummary {
            topic: row.try_get("topic").map_err(internal_error)?,
            message_count: row.try_get("message_count").map_err(internal_error)?,
            latest_at: latest_at.map(|t| t.to_rfc3339()),
        });
    }

    Ok(Json(results))
}

// ── GET /api/peer/stream?topic={topic} — SSE stream for peer messages ────

pub async fn peer_stream(
    State(state): State<AppState>,
    Query(params): Query<PeerStreamQuery>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let topic = params.topic;

    let stream = async_stream::stream! {
        let mut last_seen: Option<String> = None;

        loop {
            let query = match &last_seen {
                Some(cursor) => {
                    sqlx::query(
                        "SELECT message_id, from_task_id, to_task_id, topic, kind, payload, created_at \
                         FROM peer_messages \
                         WHERE topic = $1 AND message_id > $2 \
                         ORDER BY message_id ASC \
                         LIMIT 50"
                    )
                    .bind(&topic)
                    .bind(cursor)
                    .fetch_all(&state.pool)
                    .await
                }
                None => {
                    sqlx::query(
                        "SELECT message_id, from_task_id, to_task_id, topic, kind, payload, created_at \
                         FROM peer_messages \
                         WHERE topic = $1 \
                         ORDER BY created_at DESC \
                         LIMIT 20"
                    )
                    .bind(&topic)
                    .fetch_all(&state.pool)
                    .await
                }
            };

            match query {
                Ok(rows) => {
                    for row in &rows {
                        let message_id: String = row.get("message_id");
                        let kind: String = row.get("kind");
                        let payload: Value = row.get("payload");

                        let data = serde_json::json!({
                            "message_id": message_id,
                            "from_task_id": row.get::<String, _>("from_task_id"),
                            "to_task_id": row.get::<Option<String>, _>("to_task_id"),
                            "topic": row.get::<String, _>("topic"),
                            "kind": &kind,
                            "payload": payload,
                        });

                        last_seen = Some(message_id);

                        yield Ok(Event::default()
                            .event(&format!("peer_message:{kind}"))
                            .data(data.to_string()));
                    }
                }
                Err(e) => {
                    yield Ok(Event::default()
                        .event("error")
                        .data(format!("query failed: {e}")));
                }
            }

            tokio::time::sleep(std::time::Duration::from_secs(2)).await;
        }
    };

    Sse::new(stream)
}
