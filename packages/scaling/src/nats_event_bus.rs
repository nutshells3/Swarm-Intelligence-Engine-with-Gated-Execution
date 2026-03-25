use crate::event_bus::{Event, EventBus, EventBusError};
use crate::pg_event_bus::PgEventBus;

/// NATS JetStream event bus for clustered/distributed tiers.
///
/// Enable with `--features nats`. Requires the `async-nats` crate (not
/// yet added as an active dependency — see the commented-out line in
/// `scaling/Cargo.toml`). Current implementation delegates all
/// operations to [`PgEventBus`], so the system is fully functional
/// without a running NATS server.
///
/// ## Feature gate
///
/// The `nats` feature is defined in `Cargo.toml` but does not yet pull
/// in `async-nats` as a real dependency. When the NATS transport is
/// wired in a future milestone:
///
/// 1. Uncomment `async-nats` in `[dependencies]`.
/// 2. Replace the `PgEventBus` delegation in `publish()` with
///    JetStream `publish` calls.
/// 3. Keep the PG fallback for `query_by_aggregate` and `count` so
///    that SQL-based read models continue to work.
///
/// ## Why PG fallback?
///
/// NATS JetStream provides at-least-once delivery and stream replay,
/// but the SQL projection layer needs PostgreSQL for complex queries.
/// The intended architecture is: publish to both NATS (for fan-out)
/// and PG (for query); the PG path is the authoritative read model.
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

    /// Flush any buffered events to PG. Must be called before the bus is
    /// dropped or events may be lost. This surfaces errors explicitly
    /// rather than silently discarding buffered events (playbook rule 2).
    async fn flush_buffer(&self) -> Result<(), EventBusError> {
        let mut buffer = self.batch_buffer.lock().await;
        if !buffer.is_empty() {
            let batch = std::mem::take(&mut *buffer);
            drop(buffer);
            self.pg_fallback.publish_batch(batch).await?;
        }
        Ok(())
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

    async fn flush(&self) -> Result<(), EventBusError> {
        self.flush_buffer().await
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
