use std::path::Path;
use std::sync::Arc;

use serde::Serialize;
use tauri::State;
use zeroize::Zeroizing;

use crate::vault::ops::{self, FileEntry};
use crate::vault::state::{VaultState, VaultStatus};

/// Reject paths that point to sensitive system locations.
/// Import/export should only access user-chosen files, not system internals.
fn validate_external_path(path: &Path) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("Path must be absolute".to_string());
    }

    let path_str = path.to_string_lossy();

    // Block sensitive system directories
    #[cfg(unix)]
    {
        const BLOCKED: &[&str] = &[
            "/etc", "/var", "/private/etc", "/private/var",
            "/System", "/Library",
            "/usr", "/bin", "/sbin", "/dev", "/proc", "/sys",
        ];
        for blocked in BLOCKED {
            if path_str.starts_with(blocked) {
                return Err(format!("Access to {} is not allowed", blocked));
            }
        }
    }

    #[cfg(windows)]
    {
        let lower = path_str.to_lowercase();
        const BLOCKED: &[&str] = &[
            "c:\\windows", "c:\\program files", "c:\\program files (x86)",
            "c:\\programdata", "c:\\$recycle.bin",
        ];
        for blocked in BLOCKED {
            if lower.starts_with(blocked) {
                return Err(format!("Access to {} is not allowed", blocked));
            }
        }
    }

    // Block hidden/dot directories (e.g. .ssh, .gnupg, .aws, .config)
    for component in path.components() {
        if let std::path::Component::Normal(name) = component {
            let name_str = name.to_string_lossy();
            if name_str.starts_with('.') && name_str.len() > 1 {
                return Err(format!("Access to hidden path '{}' is not allowed", name_str));
            }
        }
    }

    Ok(())
}

#[derive(Debug, Serialize)]
#[serde(tag = "type", content = "data")]
pub enum FileContent {
    Text(String),
    Binary(String), // base64 encoded
}

/// Maximum file size for Tauri IPC. WebKit's WTF::StringImpl uses 32-bit length
/// fields, so files at or above 2 GB cannot be serialized safely.
const MAX_IPC_FILE_SIZE: usize = 2 * 1024 * 1024 * 1024;

fn ensure_unlocked(state: &VaultState) -> Result<(), String> {
    if state.status() != VaultStatus::Unlocked {
        return Err("Vault is locked".to_string());
    }
    Ok(())
}

fn use_raw64(state: &VaultState) -> bool {
    state
        .config()
        .map(|c| c.uses_raw64())
        .unwrap_or(true)
}

#[tauri::command]
pub async fn list_dir(
    path: String,
    state: State<'_, Arc<VaultState>>,
) -> Result<Vec<FileEntry>, String> {
    ensure_unlocked(&state)?;
    state.touch();

    let vault_path = state.vault_path().ok_or("No vault path")?;
    let raw64 = use_raw64(&state);

    let filename_key = state
        .with_filename_key(|k| Zeroizing::new(*k))
        .ok_or("No filename key")?;
    let content_key = state
        .with_content_key(|k| Zeroizing::new(*k))
        .ok_or("No content key")?;

    ops::list_directory(&vault_path, &path, &filename_key, &content_key, raw64)
        .map_err(|e| format!("Failed to list directory: {}", e))
}

#[tauri::command]
pub async fn read_file(
    path: String,
    state: State<'_, Arc<VaultState>>,
) -> Result<FileContent, String> {
    ensure_unlocked(&state)?;
    state.touch();

    let vault_path = state.vault_path().ok_or("No vault path")?;
    let raw64 = use_raw64(&state);

    let filename_key = state
        .with_filename_key(|k| Zeroizing::new(*k))
        .ok_or("No filename key")?;
    let content_key = state
        .with_content_key(|k| Zeroizing::new(*k))
        .ok_or("No content key")?;

    let mut data = ops::read_file(&vault_path, &path, &filename_key, &content_key, raw64)
        .map_err(|e| format!("Failed to read file: {}", e))?;

    if data.len() >= MAX_IPC_FILE_SIZE {
        let size_mb = data.len() / (1024 * 1024);
        return Err(format!(
            "File is too large to open in-app ({} MB). Use File → Export to save it locally and open with an external application.",
            size_mb,
        ));
    }

    // Take ownership of plaintext from Zeroizing wrapper (avoids clone)
    let bytes = std::mem::take(&mut *data);
    match String::from_utf8(bytes) {
        Ok(text) => Ok(FileContent::Text(text)),
        Err(e) => {
            use base64::engine::general_purpose::STANDARD;
            use base64::Engine;
            let mut bytes = e.into_bytes();
            let encoded = STANDARD.encode(&bytes);
            zeroize::Zeroize::zeroize(&mut bytes);
            Ok(FileContent::Binary(encoded))
        }
    }
}

#[tauri::command]
pub async fn write_file(
    path: String,
    content: Vec<u8>,
    state: State<'_, Arc<VaultState>>,
) -> Result<(), String> {
    ensure_unlocked(&state)?;
    state.touch();

    let vault_path = state.vault_path().ok_or("No vault path")?;
    let raw64 = use_raw64(&state);

    let filename_key = state
        .with_filename_key(|k| Zeroizing::new(*k))
        .ok_or("No filename key")?;
    let content_key = state
        .with_content_key(|k| Zeroizing::new(*k))
        .ok_or("No content key")?;

    ops::write_file(
        &vault_path,
        &path,
        &content,
        &filename_key,
        &content_key,
        raw64,
    )
    .map_err(|e| format!("Failed to write file: {}", e))
}

#[tauri::command]
pub async fn create_file(
    dir: String,
    name: String,
    state: State<'_, Arc<VaultState>>,
) -> Result<(), String> {
    ensure_unlocked(&state)?;
    state.touch();

    let vault_path = state.vault_path().ok_or("No vault path")?;
    let raw64 = use_raw64(&state);

    let filename_key = state
        .with_filename_key(|k| Zeroizing::new(*k))
        .ok_or("No filename key")?;
    let content_key = state
        .with_content_key(|k| Zeroizing::new(*k))
        .ok_or("No content key")?;

    ops::create_file(&vault_path, &dir, &name, &filename_key, &content_key, raw64)
        .map_err(|e| format!("Failed to create file: {}", e))
}

#[tauri::command]
pub async fn create_dir(
    parent: String,
    name: String,
    state: State<'_, Arc<VaultState>>,
) -> Result<(), String> {
    ensure_unlocked(&state)?;
    state.touch();

    let vault_path = state.vault_path().ok_or("No vault path")?;
    let raw64 = use_raw64(&state);

    let filename_key = state
        .with_filename_key(|k| Zeroizing::new(*k))
        .ok_or("No filename key")?;

    ops::create_directory(&vault_path, &parent, &name, &filename_key, raw64)
        .map_err(|e| format!("Failed to create directory: {}", e))
}

#[tauri::command]
pub async fn rename_entry(
    old_path: String,
    new_name: String,
    state: State<'_, Arc<VaultState>>,
) -> Result<(), String> {
    ensure_unlocked(&state)?;
    state.touch();

    let vault_path = state.vault_path().ok_or("No vault path")?;
    let raw64 = use_raw64(&state);

    let filename_key = state
        .with_filename_key(|k| Zeroizing::new(*k))
        .ok_or("No filename key")?;

    ops::rename_entry(&vault_path, &old_path, &new_name, &filename_key, raw64)
        .map_err(|e| format!("Failed to rename: {}", e))
}

#[tauri::command]
pub async fn delete_entry(
    path: String,
    _permanent: bool,
    state: State<'_, Arc<VaultState>>,
) -> Result<(), String> {
    ensure_unlocked(&state)?;
    state.touch();

    let vault_path = state.vault_path().ok_or("No vault path")?;
    let raw64 = use_raw64(&state);

    let filename_key = state
        .with_filename_key(|k| Zeroizing::new(*k))
        .ok_or("No filename key")?;

    ops::delete_entry(&vault_path, &path, &filename_key, raw64)
        .map_err(|e| format!("Failed to delete: {}", e))
}

#[tauri::command]
pub async fn copy_entry(
    source_path: String,
    dest_dir: String,
    dest_name: String,
    state: State<'_, Arc<VaultState>>,
) -> Result<(), String> {
    ensure_unlocked(&state)?;
    state.touch();

    let vault_path = state.vault_path().ok_or("No vault path")?;
    let raw64 = use_raw64(&state);

    let filename_key = state
        .with_filename_key(|k| Zeroizing::new(*k))
        .ok_or("No filename key")?;
    let content_key = state
        .with_content_key(|k| Zeroizing::new(*k))
        .ok_or("No content key")?;

    ops::copy_entry(
        &vault_path,
        &source_path,
        &dest_dir,
        &dest_name,
        &filename_key,
        &content_key,
        raw64,
    )
    .map_err(|e| format!("Failed to copy: {}", e))
}

#[tauri::command]
pub async fn search_files(
    query: String,
    state: State<'_, Arc<VaultState>>,
) -> Result<Vec<FileEntry>, String> {
    ensure_unlocked(&state)?;
    state.touch();

    let vault_path = state.vault_path().ok_or("No vault path")?;
    let raw64 = use_raw64(&state);

    let filename_key = state
        .with_filename_key(|k| Zeroizing::new(*k))
        .ok_or("No filename key")?;
    let content_key = state
        .with_content_key(|k| Zeroizing::new(*k))
        .ok_or("No content key")?;

    ops::search_files(&vault_path, &query, &filename_key, &content_key, raw64)
        .map_err(|e| format!("Failed to search: {}", e))
}

#[tauri::command]
pub async fn import_files(
    external_paths: Vec<String>,
    vault_dir: String,
    state: State<'_, Arc<VaultState>>,
) -> Result<(), String> {
    ensure_unlocked(&state)?;
    state.touch();

    let vault_path = state.vault_path().ok_or("No vault path")?;
    let raw64 = use_raw64(&state);

    let filename_key = state
        .with_filename_key(|k| Zeroizing::new(*k))
        .ok_or("No filename key")?;
    let content_key = state
        .with_content_key(|k| Zeroizing::new(*k))
        .ok_or("No content key")?;

    for ext_path in &external_paths {
        let source = std::path::Path::new(ext_path);
        validate_external_path(source)?;
        let file_name = source
            .file_name()
            .ok_or("Invalid filename")?
            .to_string_lossy()
            .to_string();

        let data = std::fs::read(source)
            .map_err(|e| format!("Failed to read source file: {}", e))?;

        ops::create_file(
            &vault_path,
            &vault_dir,
            &file_name,
            &filename_key,
            &content_key,
            raw64,
        )
        .map_err(|e| format!("Failed to create file in vault: {}", e))?;

        // Now write the actual content
        let file_path = if vault_dir.is_empty() {
            file_name
        } else {
            format!("{}/{}", vault_dir, file_name)
        };

        ops::write_file(
            &vault_path,
            &file_path,
            &data,
            &filename_key,
            &content_key,
            raw64,
        )
        .map_err(|e| format!("Failed to write imported file: {}", e))?;
    }

    Ok(())
}

#[tauri::command]
pub async fn export_file(
    vault_path_str: String,
    external_dest: String,
    state: State<'_, Arc<VaultState>>,
) -> Result<(), String> {
    ensure_unlocked(&state)?;
    state.touch();

    let dest_path = std::path::Path::new(&external_dest);
    validate_external_path(dest_path)?;

    let vault_path = state.vault_path().ok_or("No vault path")?;
    let raw64 = use_raw64(&state);

    let filename_key = state
        .with_filename_key(|k| Zeroizing::new(*k))
        .ok_or("No filename key")?;
    let content_key = state
        .with_content_key(|k| Zeroizing::new(*k))
        .ok_or("No content key")?;

    let data = ops::read_file(
        &vault_path,
        &vault_path_str,
        &filename_key,
        &content_key,
        raw64,
    )
    .map_err(|e| format!("Failed to read file: {}", e))?;

    std::fs::write(&external_dest, &*data)
        .map_err(|e| format!("Failed to write exported file: {}", e))?;
    // data is Zeroizing<Vec<u8>> — auto-zeroized on drop

    Ok(())
}
