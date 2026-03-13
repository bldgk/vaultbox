//! LRU cache for decrypted file contents with zeroize on eviction.

use lru::LruCache;
use std::num::NonZeroUsize;
use zeroize::Zeroize;

const DEFAULT_MAX_SIZE: usize = 100 * 1024 * 1024; // 100 MB

pub struct PlaintextCache {
    cache: LruCache<String, CacheEntry>,
    current_size: usize,
    max_size: usize,
}

struct CacheEntry {
    data: Vec<u8>,
}

impl Drop for CacheEntry {
    fn drop(&mut self) {
        self.data.zeroize();
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

        if let Some((_, old)) = self.cache.push(path, CacheEntry { data }) {
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
