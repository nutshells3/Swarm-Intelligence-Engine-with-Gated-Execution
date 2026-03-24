use sqlx::postgres::PgPoolOptions;
use std::path::PathBuf;
use std::{env, time::Duration};
use tracing_subscriber::EnvFilter;

use loop_runner::tick;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let database_url = env::var("ORCHESTRATION_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/development_swarm".to_string());

    let repo_root = env::var("SWARM_REPO_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| env::current_dir().unwrap());

    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(5))
        .connect(&database_url)
        .await?;

    sqlx::migrate!("../../db/migrations").run(&pool).await?;

    let scaling_config = scaling::config::load_scaling_config();
    let scaling_ctx = scaling::ScalingContext::from_config(scaling_config, pool.clone(), repo_root)
        .await
        .expect("Failed to build ScalingContext");

    tracing::info!("Loop runner started");

    loop {
        match tick::tick(&pool, &scaling_ctx).await {
            Ok(actions) => {
                if actions > 0 {
                    tracing::info!(actions, "Tick completed");
                }
            }
            Err(e) => {
                tracing::error!(error = %e, "Tick failed");
            }
        }
        tokio::time::sleep(Duration::from_secs(2)).await;
    }
}
