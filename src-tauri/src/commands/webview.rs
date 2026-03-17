use tauri::{webview::WebviewBuilder, LogicalPosition, LogicalSize, Manager, WebviewUrl};

/// Create an isolated child webview for rendering a file viewer.
/// Each webview gets its own V8 heap, so destroying it reclaims all plaintext.
#[tauri::command]
pub async fn create_viewer_webview(
    app: tauri::AppHandle,
    label: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    // get_window returns the underlying Window (not WebviewWindow),
    // which exposes add_child() for multi-webview support.
    let window = app
        .get_window("main")
        .ok_or("Main window not found")?;

    let url = WebviewUrl::App("viewer.html".into());

    let builder = WebviewBuilder::new(&label, url);

    window
        .add_child(
            builder,
            LogicalPosition::new(x, y),
            LogicalSize::new(width, height),
        )
        .map_err(|e| format!("Failed to create viewer webview: {e}"))?;

    Ok(())
}

/// Destroy an isolated viewer webview, reclaiming its entire V8 heap.
#[tauri::command]
pub async fn close_viewer_webview(
    app: tauri::AppHandle,
    label: String,
) -> Result<(), String> {
    if let Some(webview) = app.get_webview(&label) {
        webview
            .close()
            .map_err(|e| format!("Failed to close viewer webview: {e}"))?;
    }
    Ok(())
}

/// Reposition and resize an existing viewer webview.
#[tauri::command]
pub async fn resize_viewer_webview(
    app: tauri::AppHandle,
    label: String,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
) -> Result<(), String> {
    let webview = app
        .get_webview(&label)
        .ok_or_else(|| format!("Webview '{label}' not found"))?;

    webview
        .set_position(LogicalPosition::new(x, y))
        .map_err(|e| format!("Failed to set position: {e}"))?;

    webview
        .set_size(LogicalSize::new(width, height))
        .map_err(|e| format!("Failed to set size: {e}"))?;

    Ok(())
}
