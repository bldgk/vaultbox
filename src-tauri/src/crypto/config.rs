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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn make_config_json(version: u32, flags: &[&str]) -> String {
        let flags_json: Vec<String> = flags.iter().map(|f| format!("\"{}\"", f)).collect();
        format!(
            r#"{{
                "Creator": "test",
                "EncryptedKey": "dGVzdA==",
                "ScryptObject": {{
                    "Salt": "c2FsdA==",
                    "N": 65536,
                    "R": 8,
                    "P": 1,
                    "KeyLen": 32
                }},
                "Version": {},
                "FeatureFlags": [{}]
            }}"#,
            version,
            flags_json.join(",")
        )
    }

    #[test]
    fn test_load_valid_config() {
        let dir = tempfile::tempdir().unwrap();
        let conf_path = dir.path().join("gocryptfs.conf");
        let json = make_config_json(2, &["GCMIV128", "DirIV", "EMENames", "HKDF", "Raw64"]);
        fs::write(&conf_path, &json).unwrap();

        let config = GocryptfsConfig::load(dir.path()).unwrap();
        assert_eq!(config.version, 2);
        assert_eq!(config.creator, "test");
        assert!(config.uses_hkdf());
        assert!(config.uses_raw64());
        assert!(config.uses_dir_iv());
        assert!(config.uses_eme_names());
    }

    #[test]
    fn test_load_unsupported_version() {
        let dir = tempfile::tempdir().unwrap();
        let conf_path = dir.path().join("gocryptfs.conf");
        let json = make_config_json(1, &["GCMIV128"]);
        fs::write(&conf_path, &json).unwrap();

        let result = GocryptfsConfig::load(dir.path());
        assert!(matches!(result, Err(ConfigError::UnsupportedVersion(1))));
    }

    #[test]
    fn test_load_unsupported_flag() {
        let dir = tempfile::tempdir().unwrap();
        let conf_path = dir.path().join("gocryptfs.conf");
        let json = make_config_json(2, &["GCMIV128", "UnknownFlag"]);
        fs::write(&conf_path, &json).unwrap();

        let result = GocryptfsConfig::load(dir.path());
        assert!(matches!(result, Err(ConfigError::UnsupportedFlag(_))));
    }

    #[test]
    fn test_load_missing_config() {
        let dir = tempfile::tempdir().unwrap();
        let result = GocryptfsConfig::load(dir.path());
        assert!(matches!(result, Err(ConfigError::Io(_))));
    }

    #[test]
    fn test_load_invalid_json() {
        let dir = tempfile::tempdir().unwrap();
        let conf_path = dir.path().join("gocryptfs.conf");
        fs::write(&conf_path, "not valid json").unwrap();

        let result = GocryptfsConfig::load(dir.path());
        assert!(matches!(result, Err(ConfigError::Json(_))));
    }

    #[test]
    fn test_load_from_external_path() {
        let dir = tempfile::tempdir().unwrap();
        let conf_path = dir.path().join("my-custom-config.json");
        let json = make_config_json(2, &["GCMIV128", "HKDF"]);
        fs::write(&conf_path, &json).unwrap();

        let config = GocryptfsConfig::load_from(&conf_path).unwrap();
        assert_eq!(config.version, 2);
        assert!(config.uses_hkdf());
    }

    #[test]
    fn test_has_flag() {
        let dir = tempfile::tempdir().unwrap();
        let conf_path = dir.path().join("gocryptfs.conf");
        let json = make_config_json(2, &["GCMIV128", "DirIV", "LongNames"]);
        fs::write(&conf_path, &json).unwrap();

        let config = GocryptfsConfig::load(dir.path()).unwrap();
        assert!(config.has_flag("GCMIV128"));
        assert!(config.has_flag("DirIV"));
        assert!(config.has_flag("LongNames"));
        assert!(!config.has_flag("HKDF"));
        assert!(!config.has_flag("Raw64"));
        assert!(!config.has_flag("EMENames"));
    }

    #[test]
    fn test_flag_getters() {
        let dir = tempfile::tempdir().unwrap();
        let conf_path = dir.path().join("gocryptfs.conf");
        let json = make_config_json(2, &["GCMIV128", "LongNames"]);
        fs::write(&conf_path, &json).unwrap();

        let config = GocryptfsConfig::load(dir.path()).unwrap();
        assert!(config.uses_long_names());
        assert!(!config.uses_hkdf());
        assert!(!config.uses_raw64());
        assert!(!config.uses_dir_iv());
        assert!(!config.uses_eme_names());
    }

    #[test]
    fn test_all_supported_flags() {
        let dir = tempfile::tempdir().unwrap();
        let conf_path = dir.path().join("gocryptfs.conf");
        let json = make_config_json(
            2,
            &["GCMIV128", "DirIV", "EMENames", "LongNames", "HKDF", "Raw64"],
        );
        fs::write(&conf_path, &json).unwrap();

        let config = GocryptfsConfig::load(dir.path()).unwrap();
        assert!(config.uses_hkdf());
        assert!(config.uses_raw64());
        assert!(config.uses_dir_iv());
        assert!(config.uses_eme_names());
        assert!(config.uses_long_names());
        assert!(config.has_flag("GCMIV128"));
    }

    #[test]
    fn test_scrypt_params_parsed() {
        let dir = tempfile::tempdir().unwrap();
        let conf_path = dir.path().join("gocryptfs.conf");
        let json = make_config_json(2, &["GCMIV128"]);
        fs::write(&conf_path, &json).unwrap();

        let config = GocryptfsConfig::load(dir.path()).unwrap();
        assert_eq!(config.scrypt_object.n, 65536);
        assert_eq!(config.scrypt_object.r, 8);
        assert_eq!(config.scrypt_object.p, 1);
        assert_eq!(config.scrypt_object.key_len, 32);
    }

    #[test]
    fn test_config_clone() {
        let dir = tempfile::tempdir().unwrap();
        let conf_path = dir.path().join("gocryptfs.conf");
        let json = make_config_json(2, &["GCMIV128", "HKDF"]);
        fs::write(&conf_path, &json).unwrap();

        let config = GocryptfsConfig::load(dir.path()).unwrap();
        let cloned = config.clone();
        assert_eq!(config.version, cloned.version);
        assert_eq!(config.creator, cloned.creator);
        assert_eq!(config.feature_flags, cloned.feature_flags);
    }
}
