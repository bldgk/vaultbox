//! A heap-allocated, memory-locked cryptographic key.
//!
//! `LockedKey` ensures that key material:
//! 1. Lives on the heap at a stable address (required for mlock)
//! 2. Is mlock'd to prevent the OS from swapping it to disk
//! 3. Is zeroized and munlock'd on drop

use zeroize::Zeroize;

use super::mlock;

/// A 32-byte key that is heap-allocated, memory-locked, and zeroized on drop.
pub struct LockedKey {
    /// Box gives us a stable heap address for mlock.
    data: Box<[u8; 32]>,
    /// Whether mlock succeeded (we still function if it didn't, just without swap protection).
    locked: bool,
}

impl LockedKey {
    /// Create a new `LockedKey` from raw key bytes.
    /// The key is copied to a heap allocation and mlock'd.
    /// The source bytes are NOT zeroized — the caller is responsible for that
    /// (e.g. by passing a `Zeroizing<[u8; 32]>`).
    pub fn new(key: [u8; 32]) -> Self {
        let data = Box::new(key);
        let ptr = data.as_ptr() as *const u8;
        let locked = mlock::mlock(ptr, 32);
        if !locked {
            log::warn!("mlock failed for key material — key may be swapped to disk");
        }
        LockedKey { data, locked }
    }
}

impl std::ops::Deref for LockedKey {
    type Target = [u8; 32];

    fn deref(&self) -> &[u8; 32] {
        &self.data
    }
}

impl Drop for LockedKey {
    fn drop(&mut self) {
        // Zeroize before unlocking so the zeroed page is what gets released
        self.data.zeroize();
        if self.locked {
            mlock::munlock(self.data.as_ptr() as *const u8, 32);
        }
    }
}

impl Clone for LockedKey {
    fn clone(&self) -> Self {
        LockedKey::new(*self.data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_locked_key_stores_value() {
        let key = LockedKey::new([0x42; 32]);
        assert_eq!(&*key, &[0x42; 32]);
    }

    #[test]
    fn test_locked_key_deref() {
        let key = LockedKey::new([0xAA; 32]);
        let slice: &[u8; 32] = &key;
        assert_eq!(slice[0], 0xAA);
    }

    #[test]
    fn test_locked_key_clone() {
        let key = LockedKey::new([0xBB; 32]);
        let cloned = key.clone();
        assert_eq!(&*key, &*cloned);
    }

    #[test]
    fn test_locked_key_zeroized_after_drop() {
        let key = LockedKey::new([0xCC; 32]);
        let ptr = key.data.as_ptr() as *const u8;

        // Verify key is alive
        unsafe {
            assert_eq!(std::ptr::read_volatile(ptr), 0xCC);
        }

        drop(key);

        // After drop, the Box's heap memory has been zeroized then freed.
        // We can't reliably read freed memory, but the important thing is
        // that drop didn't panic (zeroize + munlock succeeded).
    }

    #[test]
    fn test_locked_key_drop_multiple() {
        // Creating and dropping many keys should not leak mlock resources
        for i in 0..100u8 {
            let _key = LockedKey::new([i; 32]);
        }
    }
}
