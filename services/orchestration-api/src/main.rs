use std::{env, net::SocketAddr};
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse()?))
        .init();

    let database_url = env::var("ORCHESTRATION_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/development_swarm".to_string());

    let (app, _pool) = orchestration_api::build_app(&database_url).await?;

    let addr: SocketAddr = "127.0.0.1:8845".parse()?;
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("listening on {addr}");
    axum::serve(listener, app).await?;
    Ok(())
}
