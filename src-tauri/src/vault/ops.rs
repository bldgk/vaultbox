//! File operations on the encrypted vault.

use std::fs;
use std::path::{Path, PathBuf};

use crate::crypto::{content, diriv, filename};
use thiserror::Error;

#[derive(Debug, Error)]
pub enum OpsError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Content error: {0}")]
    Content(#[from] content::ContentError),
    #[error("Filename error: {0}")]
    Filename(#[from] filename::FilenameError),
    #[error("DirIV error: {0}")]
    DirIv(#[from] diriv::DirIvError),
    #[error("Vault is locked")]
    VaultLocked,
    #[error("File not found: {0}")]
    NotFound(String),
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct FileEntry {
    pub name: String,
    pub is_dir: bool,
    pub size: u64,
    pub modified: i64,
    pub encrypted_name: String,
}

/// List decrypted directory entries.
pub fn list_directory(
    vault_path: &Path,
    relative_path: &str,
    filename_key: &[u8; 32],
    _content_key: &[u8; 32],
    use_raw64: bool,
) -> Result<Vec<FileEntry>, OpsError> {
    let encrypted_dir = resolve_encrypted_path(vault_path, relative_path, filename_key, use_raw64)?;

    let dir_iv = diriv::read_diriv(&encrypted_dir)?;
    let mut entries = Vec::new();

    for entry in fs::read_dir(&encrypted_dir)? {
        let entry = entry?;
        let os_name = entry.file_name();
        let encrypted_name = os_name.to_string_lossy().to_string();

        // Skip special gocryptfs files
        if encrypted_name == diriv::DIRIV_FILENAME
            || encrypted_name.starts_with("gocryptfs.longname.")
                && encrypted_name.ends_with(".name")
        {
            continue;
        }

        // Handle long names
        let name_to_decrypt = if encrypted_name.starts_with("gocryptfs.longname.") {
            // Read the .name file to get the actual encrypted name
            let name_file = encrypted_dir.join(format!("{}.name", encrypted_name));
            if name_file.exists() {
                fs::read_to_string(&name_file)?
            } else {
                continue;
            }
        } else {
            encrypted_name.clone()
        };

        let decrypted_name = match filename::decrypt_filename(
            filename_key,
            &dir_iv,
            &name_to_decrypt,
            use_raw64,
        ) {
            Ok(name) => name,
            Err(_) => continue, // Skip files we can't decrypt
        };

        let metadata = entry.metadata()?;
        let is_dir = metadata.is_dir();
        let size = if is_dir {
            0
        } else {
            content::plaintext_size(metadata.len())
        };
        let modified = metadata
            .modified()
            .map(|t| {
                t.duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64
            })
            .unwrap_or(0);

        entries.push(FileEntry {
            name: decrypted_name,
            is_dir,
            size,
            modified,
            encrypted_name: encrypted_name.clone(),
        });
    }

    entries.sort_by(|a, b| {
        b.is_dir
            .cmp(&a.is_dir)
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });

    Ok(entries)
}

/// Read and decrypt a file.
pub fn read_file(
    vault_path: &Path,
    relative_path: &str,
    filename_key: &[u8; 32],
    content_key: &[u8; 32],
    use_raw64: bool,
) -> Result<Vec<u8>, OpsError> {
    let encrypted_path =
        resolve_encrypted_path(vault_path, relative_path, filename_key, use_raw64)?;
    let encrypted_data = fs::read(&encrypted_path)?;
    let plaintext = content::decrypt_file(content_key, &encrypted_data)?;
    Ok(plaintext)
}

/// Encrypt and write a file.
pub fn write_file(
    vault_path: &Path,
    relative_path: &str,
    plaintext: &[u8],
    filename_key: &[u8; 32],
    content_key: &[u8; 32],
    use_raw64: bool,
) -> Result<(), OpsError> {
    let encrypted_path =
        resolve_encrypted_path(vault_path, relative_path, filename_key, use_raw64)?;
    let encrypted_data = content::encrypt_file(content_key, plaintext)?;

    // Atomic write: write to .tmp then rename
    let tmp_path = encrypted_path.with_extension("tmp");
    fs::write(&tmp_path, &encrypted_data)?;
    fs::rename(&tmp_path, &encrypted_path)?;

    Ok(())
}

/// Create a new empty file in the vault.
pub fn create_file(
    vault_path: &Path,
    dir_path: &str,
    file_name: &str,
    filename_key: &[u8; 32],
    content_key: &[u8; 32],
    use_raw64: bool,
) -> Result<(), OpsError> {
    let encrypted_dir = resolve_encrypted_path(vault_path, dir_path, filename_key, use_raw64)?;
    let dir_iv = diriv::read_diriv(&encrypted_dir)?;
    let encrypted_name = filename::encrypt_filename(filename_key, &dir_iv, file_name, use_raw64)?;
    let file_path = encrypted_dir.join(&encrypted_name);

    let encrypted_data = content::encrypt_file(content_key, b"")?;
    fs::write(&file_path, &encrypted_data)?;

    Ok(())
}

/// Create a new directory in the vault.
pub fn create_directory(
    vault_path: &Path,
    parent_path: &str,
    dir_name: &str,
    filename_key: &[u8; 32],
    use_raw64: bool,
) -> Result<(), OpsError> {
    let encrypted_parent =
        resolve_encrypted_path(vault_path, parent_path, filename_key, use_raw64)?;
    let parent_iv = diriv::read_diriv(&encrypted_parent)?;
    let encrypted_name =
        filename::encrypt_filename(filename_key, &parent_iv, dir_name, use_raw64)?;
    let dir_path = encrypted_parent.join(&encrypted_name);

    fs::create_dir(&dir_path)?;
    diriv::create_diriv(&dir_path)?;

    Ok(())
}

/// Rename a file or directory.
pub fn rename_entry(
    vault_path: &Path,
    old_path: &str,
    new_name: &str,
    filename_key: &[u8; 32],
    use_raw64: bool,
) -> Result<(), OpsError> {
    let old_encrypted = resolve_encrypted_path(vault_path, old_path, filename_key, use_raw64)?;
    let parent_dir = old_encrypted
        .parent()
        .ok_or_else(|| OpsError::NotFound("No parent directory".into()))?;

    let dir_iv = diriv::read_diriv(parent_dir)?;
    let new_encrypted_name =
        filename::encrypt_filename(filename_key, &dir_iv, new_name, use_raw64)?;
    let new_encrypted = parent_dir.join(&new_encrypted_name);

    fs::rename(&old_encrypted, &new_encrypted)?;

    Ok(())
}

/// Delete a file or directory.
pub fn delete_entry(
    vault_path: &Path,
    relative_path: &str,
    filename_key: &[u8; 32],
    use_raw64: bool,
) -> Result<(), OpsError> {
    let encrypted_path =
        resolve_encrypted_path(vault_path, relative_path, filename_key, use_raw64)?;

    if encrypted_path.is_dir() {
        fs::remove_dir_all(&encrypted_path)?;
    } else {
        fs::remove_file(&encrypted_path)?;
    }

    Ok(())
}

/// Copy a file within the vault (decrypt from source, re-encrypt to destination).
pub fn copy_entry(
    vault_path: &Path,
    source_path: &str,
    dest_dir: &str,
    dest_name: &str,
    filename_key: &[u8; 32],
    content_key: &[u8; 32],
    use_raw64: bool,
) -> Result<(), OpsError> {
    let source_encrypted =
        resolve_encrypted_path(vault_path, source_path, filename_key, use_raw64)?;

    if source_encrypted.is_dir() {
        // Copy directory recursively
        copy_dir_recursive(
            vault_path,
            source_path,
            dest_dir,
            dest_name,
            filename_key,
            content_key,
            use_raw64,
        )?;
    } else {
        // Read, decrypt, then re-encrypt to new location
        let plaintext = read_file(vault_path, source_path, filename_key, content_key, use_raw64)?;
        create_file(
            vault_path,
            dest_dir,
            dest_name,
            filename_key,
            content_key,
            use_raw64,
        )?;
        let dest_path = if dest_dir.is_empty() {
            dest_name.to_string()
        } else {
            format!("{}/{}", dest_dir, dest_name)
        };
        write_file(
            vault_path,
            &dest_path,
            &plaintext,
            filename_key,
            content_key,
            use_raw64,
        )?;
    }

    Ok(())
}

fn copy_dir_recursive(
    vault_path: &Path,
    source_path: &str,
    dest_parent: &str,
    dest_name: &str,
    filename_key: &[u8; 32],
    content_key: &[u8; 32],
    use_raw64: bool,
) -> Result<(), OpsError> {
    // Create the destination directory
    create_directory(vault_path, dest_parent, dest_name, filename_key, use_raw64)?;
    let new_dir = if dest_parent.is_empty() {
        dest_name.to_string()
    } else {
        format!("{}/{}", dest_parent, dest_name)
    };

    // List source directory and copy each entry
    let entries = list_directory(vault_path, source_path, filename_key, content_key, use_raw64)?;
    for entry in &entries {
        let child_source = if source_path.is_empty() {
            entry.name.clone()
        } else {
            format!("{}/{}", source_path, entry.name)
        };
        copy_entry(
            vault_path,
            &child_source,
            &new_dir,
            &entry.name,
            filename_key,
            content_key,
            use_raw64,
        )?;
    }

    Ok(())
}

/// Search for files by decrypted name.
pub fn search_files(
    vault_path: &Path,
    query: &str,
    filename_key: &[u8; 32],
    content_key: &[u8; 32],
    use_raw64: bool,
) -> Result<Vec<FileEntry>, OpsError> {
    let mut results = Vec::new();
    let query_lower = query.to_lowercase();
    search_recursive(
        vault_path,
        "",
        &query_lower,
        filename_key,
        content_key,
        use_raw64,
        &mut results,
    )?;
    Ok(results)
}

fn search_recursive(
    vault_path: &Path,
    relative_path: &str,
    query: &str,
    filename_key: &[u8; 32],
    content_key: &[u8; 32],
    use_raw64: bool,
    results: &mut Vec<FileEntry>,
) -> Result<(), OpsError> {
    let entries = list_directory(vault_path, relative_path, filename_key, content_key, use_raw64)?;

    for entry in &entries {
        let full_path = if relative_path.is_empty() {
            entry.name.clone()
        } else {
            format!("{}/{}", relative_path, entry.name)
        };

        if entry.name.to_lowercase().contains(query) {
            let mut matched = entry.clone();
            matched.name = full_path.clone();
            results.push(matched);
        }

        if entry.is_dir {
            let _ = search_recursive(
                vault_path,
                &full_path,
                query,
                filename_key,
                content_key,
                use_raw64,
                results,
            );
        }
    }

    Ok(())
}

/// Resolve a plaintext relative path to the encrypted filesystem path.
fn resolve_encrypted_path(
    vault_path: &Path,
    relative_path: &str,
    filename_key: &[u8; 32],
    use_raw64: bool,
) -> Result<PathBuf, OpsError> {
    if relative_path.is_empty() || relative_path == "/" {
        return Ok(vault_path.to_path_buf());
    }

    let parts: Vec<&str> = relative_path
        .trim_matches('/')
        .split('/')
        .filter(|s| !s.is_empty())
        .collect();

    let mut current = vault_path.to_path_buf();

    for part in parts {
        let dir_iv = diriv::read_diriv(&current)?;
        let encrypted_name = filename::encrypt_filename(filename_key, &dir_iv, part, use_raw64)?;
        current = current.join(&encrypted_name);
    }

    Ok(current)
}
