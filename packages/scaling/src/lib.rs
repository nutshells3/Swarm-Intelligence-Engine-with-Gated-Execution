pub mod config;
pub mod event_bus;
pub mod factory;
pub mod nats_event_bus;
pub mod pg_event_bus;
pub mod pooled_worktree;
pub mod worker_isolation;
pub mod worktree_isolation;

pub use config::{ScalingConfig, ScalingTier};
pub use event_bus::{Event, EventBus, EventBusError};
pub use factory::ScalingContext;
pub use worker_isolation::{IsolationError, WorkerIsolation};
pub use worktree_isolation::WorktreeIsolation;
pub use pooled_worktree::PooledWorktreeIsolation;
