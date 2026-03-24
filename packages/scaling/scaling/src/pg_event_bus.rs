use sqlx::PgPool;

use crate::event_bus::{Event, EventBus, EventBusError};

pub struct PgEventBus {
    pool: PgPool,
}

impl PgEventBus {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl EventBus for PgEventBus {
    async fn publish(&self, event: Event) -> Result<(), EventBusError> {
        sqlx::query(
            "INSERT INTO event_journal
             (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
             VALUES ($1, $2, $3, $4, $5, $6, now())
             ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
        )
        .bind(&event.event_id)
        .bind(&event.aggregate_kind)
        .bind(&event.aggregate_id)
        .bind(&event.event_kind)
        .bind(&event.idempotency_key)
        .bind(&event.payload)
        .execute(&self.pool)
        .await
        .map_err(|e| EventBusError::Database(e.to_string()))?;
        Ok(())
    }

    async fn publish_batch(&self, events: Vec<Event>) -> Result<(), EventBusError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| EventBusError::Database(e.to_string()))?;
        for event in &events {
            sqlx::query(
                "INSERT INTO event_journal
                 (event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload, created_at)
                 VALUES ($1, $2, $3, $4, $5, $6, now())
                 ON CONFLICT (aggregate_kind, aggregate_id, idempotency_key) DO NOTHING",
            )
            .bind(&event.event_id)
            .bind(&event.aggregate_kind)
            .bind(&event.aggregate_id)
            .bind(&event.event_kind)
            .bind(&event.idempotency_key)
            .bind(&event.payload)
            .execute(&mut *tx)
            .await
            .map_err(|e| EventBusError::Database(e.to_string()))?;
        }
        tx.commit()
            .await
            .map_err(|e| EventBusError::Database(e.to_string()))?;
        Ok(())
    }

    async fn query_by_aggregate(
        &self,
        aggregate_kind: &str,
        aggregate_id: &str,
        limit: i64,
    ) -> Result<Vec<Event>, EventBusError> {
        let rows = sqlx::query(
            "SELECT event_id, aggregate_kind, aggregate_id, event_kind, idempotency_key, payload
             FROM event_journal
             WHERE aggregate_kind = $1 AND aggregate_id = $2
             ORDER BY created_at DESC LIMIT $3",
        )
        .bind(aggregate_kind)
        .bind(aggregate_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| EventBusError::Database(e.to_string()))?;

        use sqlx::Row;
        Ok(rows
            .iter()
            .map(|r| Event {
                event_id: r.try_get("event_id").unwrap_or_default(),
                aggregate_kind: r.try_get("aggregate_kind").unwrap_or_default(),
                aggregate_id: r.try_get("aggregate_id").unwrap_or_default(),
                event_kind: r.try_get("event_kind").unwrap_or_default(),
                idempotency_key: r.try_get("idempotency_key").unwrap_or_default(),
                payload: r.try_get("payload").unwrap_or(serde_json::Value::Null),
            })
            .collect())
    }

    async fn count(
        &self,
        aggregate_kind: &str,
        aggregate_id: &str,
        event_kind: &str,
    ) -> Result<i64, EventBusError> {
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*) FROM event_journal
             WHERE aggregate_kind = $1 AND aggregate_id = $2 AND event_kind = $3",
        )
        .bind(aggregate_kind)
        .bind(aggregate_id)
        .bind(event_kind)
        .fetch_one(&self.pool)
        .await
        .map_err(|e| EventBusError::Database(e.to_string()))?;
        Ok(count)
    }
}
