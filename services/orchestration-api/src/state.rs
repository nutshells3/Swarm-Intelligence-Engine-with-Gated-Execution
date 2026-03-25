use std::sync::Arc;

use sqlx::PgPool;

use agent_adapters::AdapterRegistry;

#[derive(Clone)]
pub struct AppState {
    pub database_url: String,
    pub pool: PgPool,
    /// Shared adapter registry for agent-based extraction.
    /// Wrapped in Arc because AdapterRegistry is not Clone.
    pub adapter_registry: Arc<AdapterRegistry>,
}
