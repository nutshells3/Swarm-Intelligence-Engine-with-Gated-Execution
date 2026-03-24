use std::path::PathBuf;
use std::sync::Arc;

use sqlx::PgPool;

use crate::config::{ScalingConfig, ScalingTier};
use crate::event_bus::EventBus;
use crate::nats_event_bus::NatsEventBus;
use crate::pg_event_bus::PgEventBus;
use crate::pooled_worktree::PooledWorktreeIsolation;
use crate::worker_isolation::WorkerIsolation;
use crate::worktree_isolation::WorktreeIsolation;

/// Runtime context created from ScalingConfig.
pub struct ScalingContext {
    pub event_bus: Arc<dyn EventBus>,
    pub isolation: Arc<dyn WorkerIsolation>,
    pub config: ScalingConfig,
}

impl ScalingContext {
    pub async fn from_config(
        config: ScalingConfig,
        pool: PgPool,
        repo_root: PathBuf,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let (event_bus, isolation): (Arc<dyn EventBus>, Arc<dyn WorkerIsolation>) =
            match config.tier {
                ScalingTier::Standalone => {
                    tracing::info!("Scaling tier: standalone (PG direct + worktree)");
                    (
                        Arc::new(PgEventBus::new(pool.clone())),
                        Arc::new(WorktreeIsolation::new(repo_root)),
                    )
                }
                ScalingTier::Clustered => {
                    tracing::info!(
                        nats_url = %config.nats_url,
                        pool_size = config.worktree_pool_size,
                        "Scaling tier: clustered (NATS batch + worktree pool)"
                    );
                    (
                        Arc::new(NatsEventBus::new(pool.clone(), config.nats_url.clone())),
                        Arc::new(
                            PooledWorktreeIsolation::new(repo_root, config.worktree_pool_size)
                                .await?,
                        ),
                    )
                }
                ScalingTier::Distributed => {
                    tracing::info!(
                        nats_url = %config.nats_url,
                        shard = %format!("{}/{}", config.shard_id, config.shard_count),
                        "Scaling tier: distributed (NATS + container-ready)"
                    );
                    // For now, distributed uses same impls as clustered.
                    // Container isolation will replace PooledWorktreeIsolation later.
                    (
                        Arc::new(NatsEventBus::new(pool.clone(), config.nats_url.clone())),
                        Arc::new(
                            PooledWorktreeIsolation::new(repo_root, config.worktree_pool_size)
                                .await?,
                        ),
                    )
                }
            };

        Ok(Self {
            event_bus,
            isolation,
            config,
        })
    }
}
