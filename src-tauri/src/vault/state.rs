use std::path::PathBuf;
use std::time::Instant;

use parking_lot::{Mutex, RwLock};
use zeroize::Zeroizing;

use crate::crypto::config::GocryptfsConfig;
use crate::vault::cache::PlaintextCache;

pub struct VaultState {
    inner: RwLock<VaultInner>,
    media_cache: Mutex<PlaintextCache>,
}

struct VaultInner {
    status: VaultStatus,
    vault_path: Option<PathBuf>,
    config: Option<GocryptfsConfig>,
    master_key: Option<Zeroizing<[u8; 32]>>,
    content_key: Option<Zeroizing<[u8; 32]>>,
    filename_key: Option<Zeroizing<[u8; 32]>>,
    last_activity: Option<Instant>,
    auto_lock_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum VaultStatus {
    Locked,
    Unlocked,
}

impl VaultState {
    pub fn new() -> Self {
        VaultState {
            inner: RwLock::new(VaultInner {
                status: VaultStatus::Locked,
                vault_path: None,
                config: None,
                master_key: None,
                content_key: None,
                filename_key: None,
                last_activity: None,
                auto_lock_seconds: 600, // 10 min default
            }),
            media_cache: Mutex::new(PlaintextCache::new()),
        }
    }

    pub fn unlock(
        &self,
        vault_path: PathBuf,
        config: GocryptfsConfig,
        master_key: Zeroizing<[u8; 32]>,
        content_key: Zeroizing<[u8; 32]>,
        filename_key: Zeroizing<[u8; 32]>,
    ) {
        let mut inner = self.inner.write();
        inner.status = VaultStatus::Unlocked;
        inner.vault_path = Some(vault_path);
        inner.config = Some(config);
        inner.master_key = Some(master_key);
        inner.content_key = Some(content_key);
        inner.filename_key = Some(filename_key);
        inner.last_activity = Some(Instant::now());
    }

    pub fn lock(&self) {
        let mut inner = self.inner.write();
        inner.status = VaultStatus::Locked;
        inner.vault_path = None;
        inner.config = None;
        // Keys are zeroized on drop via Zeroizing wrapper
        inner.master_key = None;
        inner.content_key = None;
        inner.filename_key = None;
        inner.last_activity = None;
        drop(inner);
        self.media_cache.lock().clear();
    }

    pub fn status(&self) -> VaultStatus {
        self.inner.read().status
    }

    pub fn touch(&self) {
        let mut inner = self.inner.write();
        inner.last_activity = Some(Instant::now());
    }

    pub fn should_auto_lock(&self) -> bool {
        let inner = self.inner.read();
        if inner.status == VaultStatus::Locked {
            return false;
        }
        if let Some(last) = inner.last_activity {
            last.elapsed().as_secs() >= inner.auto_lock_seconds
        } else {
            false
        }
    }

    pub fn vault_path(&self) -> Option<PathBuf> {
        self.inner.read().vault_path.clone()
    }

    pub fn config(&self) -> Option<GocryptfsConfig> {
        self.inner.read().config.clone()
    }

    pub fn with_content_key<R>(&self, f: impl FnOnce(&[u8; 32]) -> R) -> Option<R> {
        let inner = self.inner.read();
        inner.content_key.as_ref().map(|k| f(k))
    }

    pub fn with_filename_key<R>(&self, f: impl FnOnce(&[u8; 32]) -> R) -> Option<R> {
        let inner = self.inner.read();
        inner.filename_key.as_ref().map(|k| f(k))
    }

    pub fn set_auto_lock_seconds(&self, seconds: u64) {
        self.inner.write().auto_lock_seconds = seconds;
    }

    pub fn get_cached_media(&self, path: &str) -> Option<Vec<u8>> {
        self.media_cache.lock().get(path).cloned()
    }

    pub fn cache_media(&self, path: String, data: Vec<u8>) {
        self.media_cache.lock().put(path, data);
    }
}

impl Default for VaultState {
    fn default() -> Self {
        Self::new()
    }
}
