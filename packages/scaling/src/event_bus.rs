use async_trait::async_trait;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub event_id: String,
    pub aggregate_kind: String,
    pub aggregate_id: String,
    pub event_kind: String,
    pub idempotency_key: String,
    pub payload: serde_json::Value,
}

#[async_trait]
pub trait EventBus: Send + Sync {
    /// Publish an event. Idempotent -- duplicate idempotency_key is silently ignored.
    async fn publish(&self, event: Event) -> Result<(), EventBusError>;

    /// Publish a batch of events in one round-trip.
    async fn publish_batch(&self, events: Vec<Event>) -> Result<(), EventBusError>;

    /// Flush any internally buffered events to durable storage.
    ///
    /// Implementations that buffer events (e.g. NatsEventBus) must flush
    /// their buffers here. Unbuffered implementations (PgEventBus) are
    /// no-ops. Callers must invoke flush() before shutdown to avoid
    /// silent data loss (playbook rule 2: no silent fallbacks).
    async fn flush(&self) -> Result<(), EventBusError> {
        // Default: no-op for unbuffered implementations.
        Ok(())
    }

    /// Query recent events by aggregate.
    async fn query_by_aggregate(
        &self,
        aggregate_kind: &str,
        aggregate_id: &str,
        limit: i64,
    ) -> Result<Vec<Event>, EventBusError>;

    /// Count events matching a filter.
    async fn count(
        &self,
        aggregate_kind: &str,
        aggregate_id: &str,
        event_kind: &str,
    ) -> Result<i64, EventBusError>;
}

#[derive(Debug, thiserror::Error)]
pub enum EventBusError {
    #[error("database error: {0}")]
    Database(String),
    #[error("transport error: {0}")]
    Transport(String),
    #[error("serialization error: {0}")]
    Serialization(String),
}
