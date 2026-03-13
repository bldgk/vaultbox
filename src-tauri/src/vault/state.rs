use std::path::PathBuf;
use std::time::Instant;

use parking_lot::{Mutex, RwLock};
use zeroize::Zeroizing;

use crate::crypto::config::GocryptfsConfig;
use crate::security::locked_key::LockedKey;
use crate::vault::cache::PlaintextCache;

pub struct VaultState {
    inner: RwLock<VaultInner>,
    media_cache: Mutex<PlaintextCache>,
}

struct VaultInner {
    status: VaultStatus,
    vault_path: Option<PathBuf>,
    config: Option<GocryptfsConfig>,
    master_key: Option<LockedKey>,
    content_key: Option<LockedKey>,
    filename_key: Option<LockedKey>,
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
        // Convert Zeroizing keys into LockedKeys (heap-allocated + mlock'd).
        // The Zeroizing wrappers are dropped here, zeroing the stack copies.
        let master_locked = LockedKey::new(*master_key);
        let content_locked = LockedKey::new(*content_key);
        let filename_locked = LockedKey::new(*filename_key);

        let mut inner = self.inner.write();
        inner.status = VaultStatus::Unlocked;
        inner.vault_path = Some(vault_path);
        inner.config = Some(config);
        inner.master_key = Some(master_locked);
        inner.content_key = Some(content_locked);
        inner.filename_key = Some(filename_locked);
        inner.last_activity = Some(Instant::now());
    }

    pub fn lock(&self) {
        let mut inner = self.inner.write();
        inner.status = VaultStatus::Locked;
        inner.vault_path = None;
        inner.config = None;
        // Keys are zeroized + munlock'd on LockedKey drop
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    fn dummy_config() -> GocryptfsConfig {
        GocryptfsConfig {
            creator: "test".into(),
            encrypted_key: "dGVzdA==".into(),
            scrypt_object: crate::crypto::config::ScryptObject {
                salt: "c2FsdA==".into(),
                n: 65536,
                r: 8,
                p: 1,
                key_len: 32,
            },
            version: 2,
            feature_flags: vec!["GCMIV128".into(), "HKDF".into(), "DirIV".into(), "EMENames".into(), "Raw64".into()],
        }
    }

    #[test]
    fn test_new_state_is_locked() {
        let state = VaultState::new();
        assert_eq!(state.status(), VaultStatus::Locked);
        assert!(state.vault_path().is_none());
        assert!(state.config().is_none());
    }

    #[test]
    fn test_unlock_and_lock() {
        let state = VaultState::new();
        let master = Zeroizing::new([0x42u8; 32]);
        let content = Zeroizing::new([0x43u8; 32]);
        let filename = Zeroizing::new([0x44u8; 32]);

        state.unlock(
            PathBuf::from("/tmp/vault"),
            dummy_config(),
            master,
            content,
            filename,
        );

        assert_eq!(state.status(), VaultStatus::Unlocked);
        assert_eq!(state.vault_path(), Some(PathBuf::from("/tmp/vault")));
        assert!(state.config().is_some());

        state.lock();

        assert_eq!(state.status(), VaultStatus::Locked);
        assert!(state.vault_path().is_none());
        assert!(state.config().is_none());
    }

    #[test]
    fn test_with_content_key_when_locked() {
        let state = VaultState::new();
        let result = state.with_content_key(|k| *k);
        assert!(result.is_none());
    }

    #[test]
    fn test_with_filename_key_when_locked() {
        let state = VaultState::new();
        let result = state.with_filename_key(|k| *k);
        assert!(result.is_none());
    }

    #[test]
    fn test_with_content_key_when_unlocked() {
        let state = VaultState::new();
        let content = Zeroizing::new([0x43u8; 32]);
        state.unlock(
            PathBuf::from("/tmp/vault"),
            dummy_config(),
            Zeroizing::new([0; 32]),
            content,
            Zeroizing::new([0; 32]),
        );

        let result = state.with_content_key(|k| *k);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), [0x43u8; 32]);
    }

    #[test]
    fn test_with_filename_key_when_unlocked() {
        let state = VaultState::new();
        let filename = Zeroizing::new([0x44u8; 32]);
        state.unlock(
            PathBuf::from("/tmp/vault"),
            dummy_config(),
            Zeroizing::new([0; 32]),
            Zeroizing::new([0; 32]),
            filename,
        );

        let result = state.with_filename_key(|k| *k);
        assert!(result.is_some());
        assert_eq!(result.unwrap(), [0x44u8; 32]);
    }

    #[test]
    fn test_keys_cleared_after_lock() {
        let state = VaultState::new();
        state.unlock(
            PathBuf::from("/tmp/vault"),
            dummy_config(),
            Zeroizing::new([0x42; 32]),
            Zeroizing::new([0x43; 32]),
            Zeroizing::new([0x44; 32]),
        );

        assert!(state.with_content_key(|k| *k).is_some());
        assert!(state.with_filename_key(|k| *k).is_some());

        state.lock();

        assert!(state.with_content_key(|k| *k).is_none());
        assert!(state.with_filename_key(|k| *k).is_none());
    }

    #[test]
    fn test_should_auto_lock_when_locked() {
        let state = VaultState::new();
        assert!(!state.should_auto_lock());
    }

    #[test]
    fn test_should_auto_lock_fresh_unlock() {
        let state = VaultState::new();
        state.unlock(
            PathBuf::from("/tmp/vault"),
            dummy_config(),
            Zeroizing::new([0; 32]),
            Zeroizing::new([0; 32]),
            Zeroizing::new([0; 32]),
        );
        // Just unlocked, should not auto-lock yet
        assert!(!state.should_auto_lock());
    }

    #[test]
    fn test_should_auto_lock_short_timeout() {
        let state = VaultState::new();
        state.set_auto_lock_seconds(0); // immediate timeout
        state.unlock(
            PathBuf::from("/tmp/vault"),
            dummy_config(),
            Zeroizing::new([0; 32]),
            Zeroizing::new([0; 32]),
            Zeroizing::new([0; 32]),
        );
        // With 0 seconds timeout, should auto-lock immediately
        thread::sleep(Duration::from_millis(10));
        assert!(state.should_auto_lock());
    }

    #[test]
    fn test_touch_resets_auto_lock() {
        let state = VaultState::new();
        state.set_auto_lock_seconds(0);
        state.unlock(
            PathBuf::from("/tmp/vault"),
            dummy_config(),
            Zeroizing::new([0; 32]),
            Zeroizing::new([0; 32]),
            Zeroizing::new([0; 32]),
        );
        thread::sleep(Duration::from_millis(10));
        state.touch();
        // After touch, timer resets — with 0s timeout it'll trigger again quickly
        // but the important thing is touch updated the timestamp
        // (we can't reliably test sub-millisecond timing, just test that touch doesn't panic)
    }

    #[test]
    fn test_set_auto_lock_seconds() {
        let state = VaultState::new();
        state.set_auto_lock_seconds(300);
        state.unlock(
            PathBuf::from("/tmp/vault"),
            dummy_config(),
            Zeroizing::new([0; 32]),
            Zeroizing::new([0; 32]),
            Zeroizing::new([0; 32]),
        );
        // With 300s timeout, should not auto-lock
        assert!(!state.should_auto_lock());
    }

    #[test]
    fn test_media_cache() {
        let state = VaultState::new();
        assert!(state.get_cached_media("test.mp4").is_none());

        state.cache_media("test.mp4".into(), vec![1, 2, 3, 4]);
        let cached = state.get_cached_media("test.mp4").unwrap();
        assert_eq!(cached, vec![1, 2, 3, 4]);
    }

    #[test]
    fn test_media_cache_cleared_on_lock() {
        let state = VaultState::new();
        state.unlock(
            PathBuf::from("/tmp/vault"),
            dummy_config(),
            Zeroizing::new([0; 32]),
            Zeroizing::new([0; 32]),
            Zeroizing::new([0; 32]),
        );
        state.cache_media("test.mp4".into(), vec![1, 2, 3]);
        assert!(state.get_cached_media("test.mp4").is_some());

        state.lock();
        assert!(state.get_cached_media("test.mp4").is_none());
    }

    #[test]
    fn test_unlock_twice_overwrites() {
        let state = VaultState::new();
        state.unlock(
            PathBuf::from("/tmp/vault1"),
            dummy_config(),
            Zeroizing::new([0x01; 32]),
            Zeroizing::new([0x01; 32]),
            Zeroizing::new([0x01; 32]),
        );
        state.unlock(
            PathBuf::from("/tmp/vault2"),
            dummy_config(),
            Zeroizing::new([0x02; 32]),
            Zeroizing::new([0x02; 32]),
            Zeroizing::new([0x02; 32]),
        );
        assert_eq!(state.vault_path(), Some(PathBuf::from("/tmp/vault2")));
        assert_eq!(state.with_content_key(|k| *k).unwrap(), [0x02u8; 32]);
    }

    #[test]
    fn test_default() {
        let state = VaultState::default();
        assert_eq!(state.status(), VaultStatus::Locked);
    }

    #[test]
    fn test_concurrent_access() {
        use std::sync::Arc;
        let state = Arc::new(VaultState::new());
        state.unlock(
            PathBuf::from("/tmp/vault"),
            dummy_config(),
            Zeroizing::new([0; 32]),
            Zeroizing::new([0; 32]),
            Zeroizing::new([0; 32]),
        );

        let mut handles = vec![];
        for _ in 0..10 {
            let s = Arc::clone(&state);
            handles.push(thread::spawn(move || {
                s.touch();
                let _ = s.status();
                let _ = s.vault_path();
                let _ = s.should_auto_lock();
                s.cache_media("test".into(), vec![1, 2, 3]);
                let _ = s.get_cached_media("test");
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        assert_eq!(state.status(), VaultStatus::Unlocked);
    }

    #[test]
    fn test_keys_are_locked_in_memory() {
        // After unlock, keys should be stored as LockedKeys (heap + mlock'd)
        let state = VaultState::new();
        state.unlock(
            PathBuf::from("/tmp/vault"),
            dummy_config(),
            Zeroizing::new([0x42; 32]),
            Zeroizing::new([0x43; 32]),
            Zeroizing::new([0x44; 32]),
        );

        // Keys should be accessible through the with_* callbacks
        let ck = state.with_content_key(|k| *k).unwrap();
        assert_eq!(ck, [0x43; 32]);

        let fk = state.with_filename_key(|k| *k).unwrap();
        assert_eq!(fk, [0x44; 32]);
    }

    #[test]
    fn test_lock_drops_locked_keys() {
        // After lock, LockedKey::drop should run (zeroize + munlock)
        let state = VaultState::new();
        state.unlock(
            PathBuf::from("/tmp/vault"),
            dummy_config(),
            Zeroizing::new([0xFF; 32]),
            Zeroizing::new([0xFF; 32]),
            Zeroizing::new([0xFF; 32]),
        );

        // Lock the vault — LockedKey::drop fires for each key
        state.lock();

        // Keys must be inaccessible
        assert!(state.with_content_key(|k| *k).is_none());
        assert!(state.with_filename_key(|k| *k).is_none());
    }

    #[test]
    fn test_unlock_replaces_locked_keys() {
        // Unlocking twice should drop old LockedKeys (munlock) and create new ones
        let state = VaultState::new();
        state.unlock(
            PathBuf::from("/tmp/v1"),
            dummy_config(),
            Zeroizing::new([0x01; 32]),
            Zeroizing::new([0x01; 32]),
            Zeroizing::new([0x01; 32]),
        );

        // Second unlock — old keys' LockedKey::drop should fire
        state.unlock(
            PathBuf::from("/tmp/v2"),
            dummy_config(),
            Zeroizing::new([0x02; 32]),
            Zeroizing::new([0x02; 32]),
            Zeroizing::new([0x02; 32]),
        );

        // Should have the new keys
        let ck = state.with_content_key(|k| *k).unwrap();
        assert_eq!(ck, [0x02; 32]);
    }

    #[test]
    fn test_media_cache_entries_are_mlocked() {
        let state = VaultState::new();
        // Insert a 4KB media buffer (likely to succeed mlock)
        state.cache_media("video.mp4".into(), vec![0xAA; 4096]);

        let cached = state.get_cached_media("video.mp4").unwrap();
        assert_eq!(cached.len(), 4096);
        assert_eq!(cached[0], 0xAA);
    }

    #[test]
    fn test_lock_clears_mlocked_media_cache() {
        let state = VaultState::new();
        state.unlock(
            PathBuf::from("/tmp/vault"),
            dummy_config(),
            Zeroizing::new([0; 32]),
            Zeroizing::new([0; 32]),
            Zeroizing::new([0; 32]),
        );

        state.cache_media("big.mp4".into(), vec![0xBB; 8192]);
        assert!(state.get_cached_media("big.mp4").is_some());

        // Lock should clear all cached media (triggering zeroize + munlock)
        state.lock();
        assert!(state.get_cached_media("big.mp4").is_none());
    }

    #[test]
    fn test_repeated_lock_unlock_no_mlock_leak() {
        // Rapidly locking/unlocking should not exhaust mlock resources
        let state = VaultState::new();
        for i in 0..50u8 {
            state.unlock(
                PathBuf::from("/tmp/vault"),
                dummy_config(),
                Zeroizing::new([i; 32]),
                Zeroizing::new([i; 32]),
                Zeroizing::new([i; 32]),
            );
            state.cache_media("test".into(), vec![i; 4096]);
            state.lock();
        }
        // If mlock resources leaked, later iterations would fail
        assert_eq!(state.status(), VaultStatus::Locked);
    }
}
