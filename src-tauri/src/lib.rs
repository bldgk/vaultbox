mod commands;
pub mod crypto;
pub mod security;
pub mod vault;

use std::sync::Arc;
use tauri::Manager;
use vault::state::{VaultState, VaultStatus};
use zeroize::Zeroizing;

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
    if total == 0 {
        return None;
    }
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
        .header("Access-Control-Allow-Origin", "tauri://localhost")
        .body(Vec::new())
        .unwrap()
}

/// Size threshold for streaming vs full-read: files > 10 MB use streaming.
const STREAMING_THRESHOLD: u64 = 10 * 1024 * 1024;

fn serve_media(
    state: &VaultState,
    path: &str,
    range_header: Option<&str>,
) -> tauri::http::Response<Vec<u8>> {
    if state.status() != VaultStatus::Unlocked {
        return error_response(403);
    }

    let vault_path = match state.vault_path() {
        Some(p) => p,
        None => return error_response(404),
    };
    let raw64 = state.config().map(|c| c.uses_raw64()).unwrap_or(true);
    let filename_key = match state.with_filename_key(|k| Zeroizing::new(*k)) {
        Some(k) => k,
        None => return error_response(500),
    };
    let content_key = match state.with_content_key(|k| Zeroizing::new(*k)) {
        Some(k) => k,
        None => return error_response(500),
    };

    // Resolve plaintext path → encrypted path on disk
    let encrypted_path = match vault::ops::resolve_encrypted_path(
        &vault_path, path, &filename_key, raw64,
    ) {
        Ok(p) => p,
        Err(_) => return error_response(404),
    };

    // Check file size to decide: stream large files, fully read small ones
    let file_size = match std::fs::metadata(&encrypted_path) {
        Ok(m) => m.len(),
        Err(_) => return error_response(404),
    };

    let plaintext_total = crypto::content::plaintext_size(file_size) as usize;
    let mime = mime_from_path(path);

    if file_size > STREAMING_THRESHOLD {
        // Large file: use StreamingReader — only decrypt requested range
        return serve_media_streaming(&encrypted_path, &content_key, mime, plaintext_total, range_header);
    }

    // Small file: check cache, or decrypt fully
    let data = if let Some(cached) = state.get_cached_media(path) {
        cached
    } else {
        match vault::ops::read_file(&vault_path, path, &filename_key, &content_key, raw64) {
            Ok(mut data) => {
                let bytes = std::mem::take(&mut *data);
                state.cache_media(path.to_string(), bytes.clone());
                bytes
            }
            Err(e) => {
                eprintln!("vaultmedia: failed to decrypt: {}", e);
                return error_response(404);
            }
        }
    };

    let total = data.len();

    if let Some(range) = range_header {
        if let Some((start, end)) = parse_range(range, total) {
            let chunk = data[start..=end].to_vec();
            return tauri::http::Response::builder()
                .status(206)
                .header("Content-Type", mime)
                .header("Accept-Ranges", "bytes")
                .header("Content-Range", format!("bytes {}-{}/{}", start, end, total))
                .header("Content-Length", chunk.len().to_string())
                .header("Access-Control-Allow-Origin", "tauri://localhost")
                .body(chunk)
                .unwrap();
        }
    }

    tauri::http::Response::builder()
        .status(200)
        .header("Content-Type", mime)
        .header("Accept-Ranges", "bytes")
        .header("Content-Length", total.to_string())
        .header("Access-Control-Allow-Origin", "tauri://localhost")
        .body(data)
        .unwrap()
}

/// Serve large files using StreamingReader — only decrypts the requested range.
fn serve_media_streaming(
    encrypted_path: &std::path::Path,
    content_key: &Zeroizing<[u8; 32]>,
    mime: &str,
    plaintext_total: usize,
    range_header: Option<&str>,
) -> tauri::http::Response<Vec<u8>> {
    use std::io::{Read, Seek, SeekFrom};

    let mut reader = match crypto::streaming::StreamingReader::open(encrypted_path, content_key) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("vaultmedia: streaming open failed: {}", e);
            return error_response(404);
        }
    };

    let total = reader.plaintext_size() as usize;

    if let Some(range) = range_header {
        if let Some((start, end)) = parse_range(range, total) {
            let len = end - start + 1;
            reader.seek(SeekFrom::Start(start as u64)).ok();
            let mut buf = vec![0u8; len];
            let n = reader.read(&mut buf).unwrap_or(0);
            buf.truncate(n);

            return tauri::http::Response::builder()
                .status(206)
                .header("Content-Type", mime)
                .header("Accept-Ranges", "bytes")
                .header("Content-Range", format!("bytes {}-{}/{}", start, start + n - 1, total))
                .header("Content-Length", n.to_string())
                .header("Access-Control-Allow-Origin", "tauri://localhost")
                .body(buf)
                .unwrap();
        }
    }

    // Full read fallback (browser didn't send Range)
    let mut buf = Vec::with_capacity(total);
    let _ = reader.read_to_end(&mut buf);

    tauri::http::Response::builder()
        .status(200)
        .header("Content-Type", mime)
        .header("Accept-Ranges", "bytes")
        .header("Content-Length", buf.len().to_string())
        .header("Access-Control-Allow-Origin", "tauri://localhost")
        .body(buf)
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- mime_from_path tests ---

    #[test]
    fn test_mime_video_types() {
        assert_eq!(mime_from_path("video.mp4"), "video/mp4");
        assert_eq!(mime_from_path("video.m4v"), "video/mp4");
        assert_eq!(mime_from_path("video.mov"), "video/quicktime");
        assert_eq!(mime_from_path("video.webm"), "video/webm");
        assert_eq!(mime_from_path("video.ogg"), "video/ogg");
        assert_eq!(mime_from_path("video.ogv"), "video/ogg");
        assert_eq!(mime_from_path("video.avi"), "video/x-msvideo");
        assert_eq!(mime_from_path("video.mkv"), "video/x-matroska");
        assert_eq!(mime_from_path("video.3gp"), "video/3gpp");
        assert_eq!(mime_from_path("video.ts"), "video/mp2t");
    }

    #[test]
    fn test_mime_audio_types() {
        assert_eq!(mime_from_path("audio.mp3"), "audio/mpeg");
        assert_eq!(mime_from_path("audio.wav"), "audio/wav");
        assert_eq!(mime_from_path("audio.m4a"), "audio/mp4");
        assert_eq!(mime_from_path("audio.aac"), "audio/mp4");
        assert_eq!(mime_from_path("audio.flac"), "audio/flac");
    }

    #[test]
    fn test_mime_image_types() {
        assert_eq!(mime_from_path("image.png"), "image/png");
        assert_eq!(mime_from_path("image.jpg"), "image/jpeg");
        assert_eq!(mime_from_path("image.jpeg"), "image/jpeg");
        assert_eq!(mime_from_path("image.gif"), "image/gif");
        assert_eq!(mime_from_path("image.webp"), "image/webp");
        assert_eq!(mime_from_path("image.svg"), "image/svg+xml");
        assert_eq!(mime_from_path("image.bmp"), "image/bmp");
        assert_eq!(mime_from_path("image.ico"), "image/x-icon");
        assert_eq!(mime_from_path("image.avif"), "image/avif");
    }

    #[test]
    fn test_mime_other_types() {
        assert_eq!(mime_from_path("document.pdf"), "application/pdf");
    }

    #[test]
    fn test_mime_unknown_extension() {
        assert_eq!(mime_from_path("file.xyz"), "application/octet-stream");
        assert_eq!(mime_from_path("file.bin"), "application/octet-stream");
        assert_eq!(mime_from_path("noextension"), "application/octet-stream");
    }

    #[test]
    fn test_mime_case_insensitive() {
        assert_eq!(mime_from_path("VIDEO.MP4"), "video/mp4");
        assert_eq!(mime_from_path("image.PNG"), "image/png");
        assert_eq!(mime_from_path("audio.FLAC"), "audio/flac");
    }

    #[test]
    fn test_mime_path_with_directories() {
        assert_eq!(mime_from_path("path/to/video.mp4"), "video/mp4");
        assert_eq!(mime_from_path("some/deep/path/image.jpg"), "image/jpeg");
    }

    #[test]
    fn test_mime_dotfile() {
        assert_eq!(mime_from_path(".hidden"), "application/octet-stream");
    }

    #[test]
    fn test_mime_multiple_dots() {
        assert_eq!(mime_from_path("archive.tar.gz"), "application/octet-stream"); // "gz" not recognized
        assert_eq!(mime_from_path("photo.backup.jpg"), "image/jpeg");
    }

    // --- parse_range tests ---

    #[test]
    fn test_parse_range_full() {
        assert_eq!(parse_range("bytes=0-99", 1000), Some((0, 99)));
    }

    #[test]
    fn test_parse_range_open_end() {
        assert_eq!(parse_range("bytes=500-", 1000), Some((500, 999)));
    }

    #[test]
    fn test_parse_range_suffix() {
        // Last 500 bytes of a 1000-byte file
        assert_eq!(parse_range("bytes=-500", 1000), Some((500, 999)));
    }

    #[test]
    fn test_parse_range_suffix_larger_than_file() {
        // Request last 2000 bytes of 1000-byte file → entire file
        assert_eq!(parse_range("bytes=-2000", 1000), Some((0, 999)));
    }

    #[test]
    fn test_parse_range_single_byte() {
        assert_eq!(parse_range("bytes=0-0", 1000), Some((0, 0)));
    }

    #[test]
    fn test_parse_range_last_byte() {
        assert_eq!(parse_range("bytes=999-999", 1000), Some((999, 999)));
    }

    #[test]
    fn test_parse_range_end_beyond_file() {
        // End exceeds file size → clamped to total-1
        assert_eq!(parse_range("bytes=0-5000", 1000), Some((0, 999)));
    }

    #[test]
    fn test_parse_range_start_at_total() {
        // Start at file size → invalid
        assert_eq!(parse_range("bytes=1000-1000", 1000), None);
    }

    #[test]
    fn test_parse_range_start_after_end() {
        assert_eq!(parse_range("bytes=500-100", 1000), None);
    }

    #[test]
    fn test_parse_range_invalid_format() {
        assert_eq!(parse_range("invalid", 1000), None);
        assert_eq!(parse_range("bytes=abc-def", 1000), None);
        assert_eq!(parse_range("", 1000), None);
    }

    #[test]
    fn test_parse_range_no_prefix() {
        assert_eq!(parse_range("0-99", 1000), None);
    }

    #[test]
    fn test_parse_range_zero_length_file() {
        // All ranges on zero-length files should return None
        assert_eq!(parse_range("bytes=0-", 0), None);
        assert_eq!(parse_range("bytes=-0", 0), None);
        assert_eq!(parse_range("bytes=0-0", 0), None);
    }

    #[test]
    fn test_parse_range_middle_of_file() {
        assert_eq!(parse_range("bytes=100-199", 1000), Some((100, 199)));
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Disable core dumps to prevent key material leakage
    security::coredump::disable_core_dumps();

    let vault_state = Arc::new(VaultState::new());

    tauri::Builder::default()
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
            commands::clipboard::copy_to_clipboard,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
