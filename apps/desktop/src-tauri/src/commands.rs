use std::sync::atomic::Ordering;
use tauri::command;
use tauri_plugin_dialog::DialogExt;

#[command]
pub async fn open_in_editor(path: String) -> Result<String, String> {
    // Open file in the OS default editor
    open::that(&path).map_err(|e| e.to_string())?;
    Ok(format!("Opened {}", path))
}

#[command]
pub fn get_app_info() -> serde_json::Value {
    serde_json::json!({
        "name": "Development Swarm IDE",
        "version": env!("CARGO_PKG_VERSION"),
        "api_url": "http://127.0.0.1:8845",
    })
}

/// Show a native folder-picker and return the selected path (or None if cancelled).
#[command]
pub async fn pick_workspace(app: tauri::AppHandle) -> Result<Option<String>, String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    app.dialog()
        .file()
        .set_title("Select Workspace Folder")
        .pick_folder(move |folder_path| {
            let _ = tx.send(folder_path.map(|p| p.to_string()));
        });
    rx.await.map_err(|e| format!("dialog cancelled: {}", e))
}

/// Quick liveness check: is the embedded API flag set and does the port respond?
#[command]
pub async fn get_health() -> serde_json::Value {
    let flag_ready = crate::API_READY.load(Ordering::SeqCst);

    // Attempt a lightweight TCP connect to confirm the port is live.
    let port_open = tokio::net::TcpStream::connect("127.0.0.1:8845")
        .await
        .is_ok();

    serde_json::json!({
        "api_ready": flag_ready,
        "api_port_open": port_open,
        "api_url": "http://127.0.0.1:8845",
    })
}
