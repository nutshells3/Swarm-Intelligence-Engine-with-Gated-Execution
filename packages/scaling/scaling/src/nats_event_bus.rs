use crate::event_bus::{Event, EventBus, EventBusError};
use crate::pg_event_bus::PgEventBus;

/// NATS JetStream event bus for clustered/distributed tiers.
///
/// Current implementation: delegates to PgEventBus with batch optimization.
/// When async-nats is wired, this will publish to NATS JetStream subjects
/// and use a consumer to replay into PostgreSQL for query support.
pub struct NatsEventBus {
    /// Fallback: all events still go to PG for query support.
    pg_fallback: PgEventBus,
    /// NATS connection URL (stored for when async-nats is wired).
    #[allow(dead_code)]
    nats_url: String,
    /// Batch buffer for high-throughput publishing.
    batch_buffer: tokio::sync::Mutex<Vec<Event>>,
    /// Max buffer size before auto-flush.
    max_batch_size: usize,
}

impl NatsEventBus {
    pub fn new(pg_pool: sqlx::PgPool, nats_url: String) -> Self {
        tracing::info!(
            nats_url,
            "NatsEventBus initialized (PG fallback mode until async-nats wired)"
        );
        Self {
            pg_fallback: PgEventBus::new(pg_pool),
            nats_url,
            batch_buffer: tokio::sync::Mutex::new(Vec::with_capacity(100)),
            max_batch_size: 100,
        }
    }
}

#[async_trait::async_trait]
impl EventBus for NatsEventBus {
    async fn publish(&self, event: Event) -> Result<(), EventBusError> {
        let mut buffer = self.batch_buffer.lock().await;
        buffer.push(event);
        if buffer.len() >= self.max_batch_size {
            let batch = std::mem::take(&mut *buffer);
            drop(buffer);
            self.pg_fallback.publish_batch(batch).await?;
        }
        Ok(())
    }

    async fn publish_batch(&self, events: Vec<Event>) -> Result<(), EventBusError> {
        self.pg_fallback.publish_batch(events).await
    }

    async fn query_by_aggregate(
        &self,
        aggregate_kind: &str,
        aggregate_id: &str,
        limit: i64,
    ) -> Result<Vec<Event>, EventBusError> {
        self.pg_fallback
            .query_by_aggregate(aggregate_kind, aggregate_id, limit)
            .await
    }

    async fn count(
        &self,
        aggregate_kind: &str,
        aggregate_id: &str,
        event_kind: &str,
    ) -> Result<i64, EventBusError> {
        self.pg_fallback
            .count(aggregate_kind, aggregate_id, event_kind)
            .await
    }
}
