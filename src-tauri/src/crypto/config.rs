use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ConfigError {
    #[error("Failed to read gocryptfs.conf: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse gocryptfs.conf: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Unsupported config version: {0}")]
    UnsupportedVersion(u32),
    #[error("Unsupported feature flag: {0}")]
    UnsupportedFlag(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GocryptfsConfig {
    pub creator: String,
    pub encrypted_key: String,
    pub scrypt_object: ScryptObject,
    pub version: u32,
    pub feature_flags: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct ScryptObject {
    pub salt: String,
    #[serde(rename = "N")]
    pub n: u64,
    #[serde(rename = "R")]
    pub r: u32,
    #[serde(rename = "P")]
    pub p: u32,
    pub key_len: u32,
}

const SUPPORTED_FLAGS: &[&str] = &[
    "GCMIV128",
    "DirIV",
    "EMENames",
    "LongNames",
    "HKDF",
    "Raw64",
];

impl GocryptfsConfig {
    pub fn load(vault_path: &Path) -> Result<Self, ConfigError> {
        let conf_path = vault_path.join("gocryptfs.conf");
        Self::load_from(&conf_path)
    }

    pub fn load_from(conf_path: &Path) -> Result<Self, ConfigError> {
        let data = fs::read_to_string(conf_path)?;
        let config: GocryptfsConfig = serde_json::from_str(&data)?;

        if config.version != 2 {
            return Err(ConfigError::UnsupportedVersion(config.version));
        }

        for flag in &config.feature_flags {
            if !SUPPORTED_FLAGS.contains(&flag.as_str()) {
                return Err(ConfigError::UnsupportedFlag(flag.clone()));
            }
        }

        Ok(config)
    }

    pub fn has_flag(&self, flag: &str) -> bool {
        self.feature_flags.iter().any(|f| f == flag)
    }

    pub fn uses_hkdf(&self) -> bool {
        self.has_flag("HKDF")
    }

    pub fn uses_long_names(&self) -> bool {
        self.has_flag("LongNames")
    }

    pub fn uses_raw64(&self) -> bool {
        self.has_flag("Raw64")
    }

    pub fn uses_dir_iv(&self) -> bool {
        self.has_flag("DirIV")
    }

    pub fn uses_eme_names(&self) -> bool {
        self.has_flag("EMENames")
    }
}
