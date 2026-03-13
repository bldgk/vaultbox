use std::sync::atomic::{AtomicU64, Ordering};

/// Monotonic counter to identify clipboard write operations.
/// When a clear fires, it only clears if the counter matches (no stale clears).
static CLIPBOARD_GENERATION: AtomicU64 = AtomicU64::new(0);

/// Copy text to system clipboard, then auto-clear after `timeout_seconds`.
#[tauri::command]
pub async fn copy_to_clipboard(
    text: String,
    timeout_seconds: u64,
) -> Result<(), String> {
    let mut clipboard = arboard::Clipboard::new()
        .map_err(|e| format!("Clipboard error: {}", e))?;
    clipboard.set_text(&text)
        .map_err(|e| format!("Failed to copy: {}", e))?;

    if timeout_seconds > 0 {
        let generation = CLIPBOARD_GENERATION.fetch_add(1, Ordering::SeqCst) + 1;

        // Spawn a background task to clear clipboard after timeout
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(timeout_seconds));

            // Only clear if no newer clipboard write has happened
            if CLIPBOARD_GENERATION.load(Ordering::SeqCst) == generation {
                if let Ok(mut cb) = arboard::Clipboard::new() {
                    let _ = cb.set_text("");
                }
            }
        });
    }

    Ok(())
}

/// Legacy stub kept for backward compatibility.
#[tauri::command]
pub async fn set_clipboard_timeout(_seconds: u64) -> Result<(), String> {
    Ok(())
}
