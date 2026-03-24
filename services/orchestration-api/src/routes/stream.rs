use axum::extract::State;
use axum::response::sse::{Event, Sse};
use serde_json::Value;
use sqlx::Row;
use std::convert::Infallible;

use crate::state::AppState;

/// SSE endpoint that polls event_journal for new events every 2 seconds.
///
/// In production this would use PostgreSQL LISTEN/NOTIFY; for now polling
/// is good enough for development dashboards.
pub async fn event_stream(
    State(state): State<AppState>,
) -> Sse<impl tokio_stream::Stream<Item = Result<Event, Infallible>>> {
    let stream = async_stream::stream! {
        let mut last_seen: Option<String> = None;

        loop {
            let query = match &last_seen {
                Some(cursor) => {
                    sqlx::query(
                        "SELECT event_id, aggregate_kind, aggregate_id, event_kind, payload, created_at \
                         FROM event_journal \
                         WHERE event_id > $1 \
                         ORDER BY event_id ASC \
                         LIMIT 50"
                    )
                    .bind(cursor)
                }
                None => {
                    sqlx::query(
                        "SELECT event_id, aggregate_kind, aggregate_id, event_kind, payload, created_at \
                         FROM event_journal \
                         ORDER BY created_at ASC \
                         LIMIT 20"
                    )
                }
            };

            match query.fetch_all(&state.pool).await {
                Ok(rows) => {
                    for row in &rows {
                        let event_id: String = row.get("event_id");
                        let event_kind: String = row.get("event_kind");
                        let payload: Value = row.get("payload");

                        let data = serde_json::json!({
                            "event_id": event_id,
                            "aggregate_kind": row.get::<String, _>("aggregate_kind"),
                            "aggregate_id": row.get::<String, _>("aggregate_id"),
                            "event_kind": &event_kind,
                            "payload": payload,
                        });

                        last_seen = Some(event_id);

                        yield Ok(Event::default()
                            .event(&event_kind)
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
