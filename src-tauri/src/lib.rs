mod commands;
mod crypto;
mod security;
mod vault;

use std::sync::Arc;
use tauri::Manager;
use vault::state::{VaultState, VaultStatus};

fn mime_from_path(path: &str) -> &'static str {
    let ext = path.rsplit('.').next().unwrap_or("").to_lowercase();
    match ext.as_str() {
        "mp4" | "m4v" => "video/mp4",
        "mov" => "video/quicktime",
        "webm" => "video/webm",
        "ogg" | "ogv" => "video/ogg",
        "avi" => "video/x-msvideo",
        "mkv" => "video/x-matroska",
        "3gp" => "video/3gpp",
        "ts" => "video/mp2t",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "m4a" | "aac" => "audio/mp4",
        "flac" => "audio/flac",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "svg" => "image/svg+xml",
        "bmp" => "image/bmp",
        "ico" => "image/x-icon",
        "avif" => "image/avif",
        "pdf" => "application/pdf",
        _ => "application/octet-stream",
    }
}

fn parse_range(range_header: &str, total: usize) -> Option<(usize, usize)> {
    let range = range_header.strip_prefix("bytes=")?;
    let mut parts = range.splitn(2, '-');
    let start_str = parts.next()?;
    let end_str = parts.next()?;

    if start_str.is_empty() {
        // suffix range: bytes=-500 means last 500 bytes
        let suffix: usize = end_str.parse().ok()?;
        let start = total.saturating_sub(suffix);
        Some((start, total - 1))
    } else {
        let start: usize = start_str.parse().ok()?;
        let end: usize = if end_str.is_empty() {
            total - 1
        } else {
            end_str.parse().ok()?
        };
        if start > end || start >= total {
            return None;
        }
        Some((start, end.min(total - 1)))
    }
}

fn error_response(status: u16) -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(status)
        .header("Access-Control-Allow-Origin", "*")
        .body(Vec::new())
        .unwrap()
}

fn serve_media(
    state: &VaultState,
    path: &str,
    range_header: Option<&str>,
) -> tauri::http::Response<Vec<u8>> {
    if state.status() != VaultStatus::Unlocked {
        return error_response(403);
    }

    // Check media cache first, then decrypt
    let data = if let Some(cached) = state.get_cached_media(path) {
        cached
    } else {
        let vault_path = match state.vault_path() {
            Some(p) => p,
            None => return error_response(404),
        };
        let raw64 = state.config().map(|c| c.uses_raw64()).unwrap_or(true);
        let filename_key = match state.with_filename_key(|k| *k) {
            Some(k) => k,
            None => return error_response(500),
        };
        let content_key = match state.with_content_key(|k| *k) {
            Some(k) => k,
            None => return error_response(500),
        };

        match vault::ops::read_file(&vault_path, path, &filename_key, &content_key, raw64) {
            Ok(data) => {
                state.cache_media(path.to_string(), data.clone());
                data
            }
            Err(e) => {
                eprintln!("vaultmedia: failed to read {}: {}", path, e);
                return error_response(404);
            }
        }
    };

    let mime = mime_from_path(path);
    let total = data.len();

    // Handle Range requests (essential for video seeking)
    if let Some(range) = range_header {
        if let Some((start, end)) = parse_range(range, total) {
            let chunk = data[start..=end].to_vec();
            return tauri::http::Response::builder()
                .status(206)
                .header("Content-Type", mime)
                .header("Accept-Ranges", "bytes")
                .header("Content-Range", format!("bytes {}-{}/{}", start, end, total))
                .header("Content-Length", chunk.len().to_string())
                .header("Access-Control-Allow-Origin", "*")
                .body(chunk)
                .unwrap();
        }
    }

    // Full content response
    tauri::http::Response::builder()
        .status(200)
        .header("Content-Type", mime)
        .header("Accept-Ranges", "bytes")
        .header("Content-Length", total.to_string())
        .header("Access-Control-Allow-Origin", "*")
        .body(data)
        .unwrap()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Disable core dumps to prevent key material leakage
    security::coredump::disable_core_dumps();

    let vault_state = Arc::new(VaultState::new());

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(vault_state)
        .register_asynchronous_uri_scheme_protocol("vaultmedia", |ctx, request, responder| {
            let state: Arc<VaultState> = ctx.app_handle().state::<Arc<VaultState>>().inner().clone();

            let uri = request.uri().to_string();
            // URI format: vaultmedia://localhost/<encoded-path>
            let raw_path = uri
                .strip_prefix("vaultmedia://localhost/")
                .or_else(|| uri.strip_prefix("vaultmedia://localhost"))
                .unwrap_or("")
                .to_string();
            let path = urlencoding::decode(&raw_path)
                .unwrap_or_else(|_| raw_path.clone().into())
                .to_string();

            let range_header = request
                .headers()
                .get("range")
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            std::thread::spawn(move || {
                let response = serve_media(&state, &path, range_header.as_deref());
                let _ = responder.respond(response);
            });
        })
        .invoke_handler(tauri::generate_handler![
            commands::vault::open_vault,
            commands::vault::create_vault,
            commands::vault::lock_vault,
            commands::vault::get_vault_status,
            commands::files::list_dir,
            commands::files::read_file,
            commands::files::write_file,
            commands::files::create_file,
            commands::files::create_dir,
            commands::files::rename_entry,
            commands::files::delete_entry,
            commands::files::search_files,
            commands::files::copy_entry,
            commands::files::import_files,
            commands::files::export_file,
            commands::clipboard::set_clipboard_timeout,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
