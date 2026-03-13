use std::path::PathBuf;
use std::sync::Arc;

use serde::Serialize;
use tauri::State;

use crate::crypto::config::GocryptfsConfig;
use crate::crypto::kdf;
use crate::vault::state::{VaultState, VaultStatus};

#[derive(Debug, Serialize)]
pub struct VaultInfo {
    pub path: String,
    pub version: u32,
    pub feature_flags: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct VaultStatusResponse {
    pub status: VaultStatus,
    pub path: Option<String>,
}

#[tauri::command]
pub async fn open_vault(
    path: String,
    password: String,
    config_path: Option<String>,
    state: State<'_, Arc<VaultState>>,
) -> Result<VaultInfo, String> {
    let vault_path = PathBuf::from(&path);

    let config = match config_path {
        Some(cp) => GocryptfsConfig::load_from(&PathBuf::from(&cp))
            .map_err(|e| format!("Failed to load external config: {}", e))?,
        None => GocryptfsConfig::load(&vault_path)
            .map_err(|e| format!("Failed to load config: {}", e))?,
    };

    let master_key = kdf::derive_master_key(&password, &config)
        .map_err(|e| format!("Failed to derive key: {}", e))?;

    // Derive sub-keys
    let content_key = if config.uses_hkdf() {
        kdf::derive_content_key(&master_key)
            .map_err(|e| format!("Failed to derive content key: {}", e))?
    } else {
        master_key.clone()
    };

    let filename_key = if config.uses_hkdf() {
        kdf::derive_filename_key(&master_key)
            .map_err(|e| format!("Failed to derive filename key: {}", e))?
    } else {
        master_key.clone()
    };

    let info = VaultInfo {
        path: path.clone(),
        version: config.version,
        feature_flags: config.feature_flags.clone(),
    };

    state.unlock(vault_path, config, master_key, content_key, filename_key);

    Ok(info)
}

#[tauri::command]
pub async fn create_vault(
    path: String,
    password: String,
    state: State<'_, Arc<VaultState>>,
) -> Result<VaultInfo, String> {
    use aes_gcm::{
        aead::{Aead, KeyInit},
        Aes256Gcm, Nonce,
    };
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use rand::RngCore;
    use zeroize::Zeroizing;

    let vault_path = PathBuf::from(&path);
    std::fs::create_dir_all(&vault_path)
        .map_err(|e| format!("Failed to create directory: {}", e))?;

    // Generate random master key
    let mut master_key = Zeroizing::new([0u8; 32]);
    rand::rng().fill_bytes(master_key.as_mut());

    // Generate scrypt salt
    let mut salt = [0u8; 32];
    rand::rng().fill_bytes(&mut salt);

    // Derive wrapping key from password
    let scrypt_n: u64 = 65536;
    let scrypt_r: u32 = 8;
    let scrypt_p: u32 = 1;

    let scrypt_params = scrypt::Params::new(16, scrypt_r, scrypt_p, 32)
        .map_err(|e| format!("scrypt params error: {}", e))?;

    let mut wrapping_key = Zeroizing::new([0u8; 32]);
    scrypt::scrypt(
        password.as_bytes(),
        &salt,
        &scrypt_params,
        wrapping_key.as_mut(),
    )
    .map_err(|e| format!("scrypt error: {}", e))?;

    // Encrypt master key with wrapping key
    let cipher = Aes256Gcm::new_from_slice(wrapping_key.as_ref())
        .map_err(|_| "Failed to create cipher")?;

    let mut nonce_bytes = [0u8; 12];
    rand::rng().fill_bytes(&mut nonce_bytes);
    let nonce = Nonce::from_slice(&nonce_bytes);

    let encrypted_master = cipher
        .encrypt(nonce, master_key.as_ref() as &[u8])
        .map_err(|_| "Failed to encrypt master key")?;

    // Combine nonce + encrypted key
    let mut encrypted_key_full = Vec::with_capacity(12 + encrypted_master.len());
    encrypted_key_full.extend_from_slice(&nonce_bytes);
    encrypted_key_full.extend_from_slice(&encrypted_master);

    let feature_flags = vec![
        "GCMIV128".to_string(),
        "DirIV".to_string(),
        "EMENames".to_string(),
        "LongNames".to_string(),
        "HKDF".to_string(),
        "Raw64".to_string(),
    ];

    let config = GocryptfsConfig {
        creator: "vaultbox".to_string(),
        encrypted_key: STANDARD.encode(&encrypted_key_full),
        scrypt_object: crate::crypto::config::ScryptObject {
            salt: STANDARD.encode(&salt),
            n: scrypt_n,
            r: scrypt_r,
            p: scrypt_p,
            key_len: 32,
        },
        version: 2,
        feature_flags: feature_flags.clone(),
    };

    let config_json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    std::fs::write(vault_path.join("gocryptfs.conf"), &config_json)
        .map_err(|e| format!("Failed to write config: {}", e))?;

    // Create root diriv
    crate::crypto::diriv::create_diriv(&vault_path)
        .map_err(|e| format!("Failed to create diriv: {}", e))?;

    // Derive sub-keys and unlock
    let content_key = kdf::derive_content_key(&master_key)
        .map_err(|e| format!("Failed to derive content key: {}", e))?;
    let filename_key = kdf::derive_filename_key(&master_key)
        .map_err(|e| format!("Failed to derive filename key: {}", e))?;

    let info = VaultInfo {
        path: path.clone(),
        version: 2,
        feature_flags,
    };

    state.unlock(vault_path, config, master_key, content_key, filename_key);

    Ok(info)
}

#[tauri::command]
pub async fn lock_vault(state: State<'_, Arc<VaultState>>) -> Result<(), String> {
    state.lock();
    Ok(())
}

#[tauri::command]
pub async fn get_vault_status(
    state: State<'_, Arc<VaultState>>,
) -> Result<VaultStatusResponse, String> {
    Ok(VaultStatusResponse {
        status: state.status(),
        path: state.vault_path().map(|p| p.to_string_lossy().to_string()),
    })
}
