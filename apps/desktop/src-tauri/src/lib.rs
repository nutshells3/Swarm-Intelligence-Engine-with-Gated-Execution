use std::sync::atomic::{AtomicBool, Ordering};
use tauri::Emitter;

mod commands;

/// Shared flag: true once the embedded API is listening.
pub(crate) static API_READY: AtomicBool = AtomicBool::new(false);

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .setup(|app| {
            // Start the orchestration API server in a background task
            let handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                if let Err(msg) = start_api_server().await {
                    eprintln!("{}", msg);
                    let _ = handle.emit("api-start-failed", msg);
                }
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::open_in_editor,
            commands::get_app_info,
            commands::pick_workspace,
            commands::get_health,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

/// Build the orchestration API, bind to 127.0.0.1:8845, and serve.
///
/// Returns `Err(message)` if the API cannot be built or the port is busy.
/// This function is deliberately structured so that `Box<dyn Error>` from
/// `build_app` does not live across an await point (which would break Send).
async fn start_api_server() -> Result<(), String> {
    let db_url = std::env::var("ORCHESTRATION_DATABASE_URL")
        .unwrap_or_else(|_| "postgres://localhost/development_swarm".to_string());

    // Phase 1: build the app (convert non-Send error immediately)
    let (router, _pool) = orchestration_api::build_app(&db_url)
        .await
        .map_err(|e| format!("Failed to build orchestration API: {}", e))?;

    // Phase 2: bind the port
    let addr: std::net::SocketAddr = "127.0.0.1:8845".parse().unwrap();
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| format!("Port {} unavailable ({}). Is another instance running?", addr, e))?;

    // Phase 3: serve
    API_READY.store(true, Ordering::SeqCst);
    println!("Orchestration API started on {}", addr);

    if let Err(e) = axum::serve(listener, router).await {
        API_READY.store(false, Ordering::SeqCst);
        return Err(format!("Orchestration API exited with error: {}", e));
    }

    Ok(())
}
