//! LRU cache for decrypted file contents with zeroize on eviction.
//!
//! Cache entries are mlock'd to prevent plaintext from being swapped to disk,
//! and zeroized on eviction/drop.

use lru::LruCache;
use std::num::NonZeroUsize;
use zeroize::Zeroize;

use crate::security::mlock;

const DEFAULT_MAX_SIZE: usize = 100 * 1024 * 1024; // 100 MB

pub struct PlaintextCache {
    cache: LruCache<String, CacheEntry>,
    current_size: usize,
    max_size: usize,
}

struct CacheEntry {
    data: Vec<u8>,
    locked: bool,
}

impl CacheEntry {
    fn new(data: Vec<u8>) -> Self {
        let locked = if !data.is_empty() {
            mlock::mlock(data.as_ptr(), data.len())
        } else {
            false
        };
        CacheEntry { data, locked }
    }
}

impl Drop for CacheEntry {
    fn drop(&mut self) {
        self.data.zeroize();
        if self.locked {
            // munlock the (now zeroed) pages so the OS can reclaim them
            mlock::munlock(self.data.as_ptr(), self.data.capacity());
        }
    }
}

impl PlaintextCache {
    pub fn new() -> Self {
        PlaintextCache {
            cache: LruCache::new(NonZeroUsize::new(1000).unwrap()),
            current_size: 0,
            max_size: DEFAULT_MAX_SIZE,
        }
    }

    pub fn get(&mut self, path: &str) -> Option<&Vec<u8>> {
        self.cache.get(path).map(|e| &e.data)
    }

    pub fn put(&mut self, path: String, data: Vec<u8>) {
        let size = data.len();

        // Evict until we have room
        while self.current_size + size > self.max_size {
            if let Some((_, evicted)) = self.cache.pop_lru() {
                self.current_size -= evicted.data.len();
                // CacheEntry::drop will zeroize
            } else {
                break;
            }
        }

        if let Some((_, old)) = self.cache.push(path, CacheEntry::new(data)) {
            self.current_size -= old.data.len();
        }
        self.current_size += size;
    }

    pub fn remove(&mut self, path: &str) {
        if let Some(entry) = self.cache.pop(path) {
            self.current_size -= entry.data.len();
        }
    }

    pub fn clear(&mut self) {
        self.cache.clear();
        self.current_size = 0;
    }

    pub fn current_size(&self) -> usize {
        self.current_size
    }
}

impl Default for PlaintextCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_cache_empty() {
        let cache = PlaintextCache::new();
        assert_eq!(cache.current_size(), 0);
    }

    #[test]
    fn test_put_and_get() {
        let mut cache = PlaintextCache::new();
        cache.put("file1".into(), vec![1, 2, 3]);
        let data = cache.get("file1").unwrap();
        assert_eq!(data, &vec![1, 2, 3]);
    }

    #[test]
    fn test_get_missing_returns_none() {
        let mut cache = PlaintextCache::new();
        assert!(cache.get("nonexistent").is_none());
    }

    #[test]
    fn test_size_tracking() {
        let mut cache = PlaintextCache::new();
        cache.put("a".into(), vec![0; 100]);
        assert_eq!(cache.current_size(), 100);

        cache.put("b".into(), vec![0; 200]);
        assert_eq!(cache.current_size(), 300);
    }

    #[test]
    fn test_remove() {
        let mut cache = PlaintextCache::new();
        cache.put("a".into(), vec![0; 100]);
        cache.put("b".into(), vec![0; 200]);
        assert_eq!(cache.current_size(), 300);

        cache.remove("a");
        assert_eq!(cache.current_size(), 200);
        assert!(cache.get("a").is_none());
        assert!(cache.get("b").is_some());
    }

    #[test]
    fn test_remove_nonexistent() {
        let mut cache = PlaintextCache::new();
        cache.put("a".into(), vec![0; 100]);
        cache.remove("nonexistent"); // should not panic
        assert_eq!(cache.current_size(), 100);
    }

    #[test]
    fn test_clear() {
        let mut cache = PlaintextCache::new();
        cache.put("a".into(), vec![0; 100]);
        cache.put("b".into(), vec![0; 200]);
        cache.clear();
        assert_eq!(cache.current_size(), 0);
        assert!(cache.get("a").is_none());
        assert!(cache.get("b").is_none());
    }

    #[test]
    fn test_overwrite_existing_key() {
        let mut cache = PlaintextCache::new();
        cache.put("a".into(), vec![0; 100]);
        assert_eq!(cache.current_size(), 100);

        cache.put("a".into(), vec![0; 50]);
        assert_eq!(cache.current_size(), 50);

        let data = cache.get("a").unwrap();
        assert_eq!(data.len(), 50);
    }

    #[test]
    fn test_lru_eviction_on_size_limit() {
        // Create a cache with a small max_size to test eviction
        let mut cache = PlaintextCache {
            cache: LruCache::new(NonZeroUsize::new(1000).unwrap()),
            current_size: 0,
            max_size: 500, // 500 bytes max
        };

        cache.put("a".into(), vec![0; 200]);
        cache.put("b".into(), vec![0; 200]);
        assert_eq!(cache.current_size(), 400);

        // This should evict "a" (LRU) to make room
        cache.put("c".into(), vec![0; 200]);
        assert!(cache.get("a").is_none(), "LRU entry 'a' should be evicted");
        assert!(cache.get("b").is_some() || cache.get("c").is_some());
    }

    #[test]
    fn test_lru_access_updates_recency() {
        let mut cache = PlaintextCache {
            cache: LruCache::new(NonZeroUsize::new(1000).unwrap()),
            current_size: 0,
            max_size: 500,
        };

        cache.put("a".into(), vec![0; 200]);
        cache.put("b".into(), vec![0; 200]);

        // Access "a" to make it recently used
        let _ = cache.get("a");

        // Now "b" is LRU and should be evicted first
        cache.put("c".into(), vec![0; 200]);
        assert!(cache.get("a").is_some(), "'a' was recently accessed, should survive");
        assert!(cache.get("b").is_none(), "'b' was LRU, should be evicted");
    }

    #[test]
    fn test_cache_entry_zeroize_on_drop() {
        // Verify CacheEntry drop doesn't panic (it zeroizes + munlocks data)
        let entry = CacheEntry::new(vec![0xAA; 100]);
        assert!(entry.data.iter().all(|&b| b == 0xAA));
        drop(entry);
    }

    #[test]
    fn test_many_entries() {
        let mut cache = PlaintextCache::new();
        for i in 0..100 {
            cache.put(format!("file_{}", i), vec![i as u8; 10]);
        }
        assert_eq!(cache.current_size(), 1000);

        for i in 0..100 {
            let data = cache.get(&format!("file_{}", i)).unwrap();
            assert_eq!(data, &vec![i as u8; 10]);
        }
    }

    #[test]
    fn test_default() {
        let cache = PlaintextCache::default();
        assert_eq!(cache.current_size(), 0);
    }

    #[test]
    fn test_cache_entry_is_mlocked() {
        let entry = CacheEntry::new(vec![0xAA; 4096]);
        // On most systems mlock should succeed for a 4KB buffer
        // (mlock may fail under strict ulimits, so we check but don't assert)
        if entry.locked {
            // Data should still be accessible when locked
            assert_eq!(entry.data[0], 0xAA);
            assert_eq!(entry.data[4095], 0xAA);
        }
        drop(entry);
        // Drop zeroizes + munlocks without panic
    }

    #[test]
    fn test_cache_entry_empty_is_not_mlocked() {
        let entry = CacheEntry::new(vec![]);
        assert!(!entry.locked);
        drop(entry);
    }

    #[test]
    fn test_cache_entry_zeroizes_before_munlock() {
        // Verify zeroize works on the buffer while still allocated
        let mut data = vec![0xBB; 256];
        let ptr = data.as_ptr();
        use zeroize::Zeroize;
        data.zeroize();
        // Buffer is zeroed while still allocated (before dealloc)
        unsafe {
            for i in 0..256 {
                assert_eq!(
                    std::ptr::read_volatile(ptr.add(i)),
                    0,
                    "Byte {} not zeroed",
                    i
                );
            }
        }
    }

    #[test]
    fn test_cache_eviction_munlocks() {
        // Evicting many mlocked entries should not exhaust mlock limits
        let mut cache = PlaintextCache {
            cache: LruCache::new(NonZeroUsize::new(1000).unwrap()),
            current_size: 0,
            max_size: 8192, // small limit to force evictions
        };

        // Insert enough data to cause multiple evictions
        for i in 0..20u8 {
            cache.put(format!("file_{}", i), vec![i; 4096]);
        }

        // Cache should still be functional (evictions released mlock'd pages)
        assert!(cache.current_size() <= 8192);
        // Most recent entries should be accessible
        assert!(cache.get("file_19").is_some());
    }

    #[test]
    fn test_cache_clear_munlocks_all() {
        let mut cache = PlaintextCache::new();
        // Insert several mlocked entries
        for i in 0..10 {
            cache.put(format!("file_{}", i), vec![0xCC; 4096]);
        }
        assert!(cache.current_size() > 0);

        // Clear should munlock + zeroize all entries
        cache.clear();
        assert_eq!(cache.current_size(), 0);

        // We should be able to insert new entries (mlock resources freed)
        cache.put("new".into(), vec![0xDD; 4096]);
        assert!(cache.get("new").is_some());
    }
}
