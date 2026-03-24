use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ScalingTier {
    Standalone,
    Clustered,
    Distributed,
}

impl Default for ScalingTier {
    fn default() -> Self {
        Self::Standalone
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScalingConfig {
    #[serde(default)]
    pub tier: ScalingTier,

    #[serde(default = "default_max_connections")]
    pub max_db_connections: u32,

    #[serde(default = "default_nats_url")]
    pub nats_url: String,

    #[serde(default = "default_pool_size")]
    pub worktree_pool_size: usize,

    #[serde(default)]
    pub shard_id: u32,

    #[serde(default = "default_shard_count")]
    pub shard_count: u32,
}

fn default_max_connections() -> u32 {
    10
}
fn default_nats_url() -> String {
    "nats://127.0.0.1:4222".into()
}
fn default_pool_size() -> usize {
    20
}
fn default_shard_count() -> u32 {
    1
}

impl Default for ScalingConfig {
    fn default() -> Self {
        Self {
            tier: ScalingTier::Standalone,
            max_db_connections: 10,
            nats_url: default_nats_url(),
            worktree_pool_size: 20,
            shard_id: 0,
            shard_count: 1,
        }
    }
}

/// Load scaling config from environment / config file / defaults.
///
/// Priority:
/// 1. `SIEGE_SCALING_TIER` env var overrides the tier field
/// 2. `siege.toml` `[scaling]` section if the file exists
/// 3. `ScalingConfig::default()` (standalone)
pub fn load_scaling_config() -> ScalingConfig {
    // Start with file-based config or default
    let mut config = match std::fs::read_to_string("siege.toml") {
        Ok(contents) => {
            #[derive(Deserialize)]
            struct SiegeToml {
                #[serde(default)]
                scaling: ScalingConfig,
            }
            match toml::from_str::<SiegeToml>(&contents) {
                Ok(parsed) => parsed.scaling,
                Err(e) => {
                    tracing::warn!(error = %e, "Failed to parse siege.toml [scaling], using defaults");
                    ScalingConfig::default()
                }
            }
        }
        Err(_) => ScalingConfig::default(),
    };

    // Env var override for the tier
    if let Ok(tier_str) = std::env::var("SIEGE_SCALING_TIER") {
        match tier_str.to_lowercase().as_str() {
            "standalone" => config.tier = ScalingTier::Standalone,
            "clustered" => config.tier = ScalingTier::Clustered,
            "distributed" => config.tier = ScalingTier::Distributed,
            other => {
                tracing::warn!(
                    tier = other,
                    "Unknown SIEGE_SCALING_TIER value, keeping config file value"
                );
            }
        }
    }

    config
}
