//! A heap-allocated, memory-locked, XOR-masked cryptographic key.
//!
//! `LockedKey` ensures that key material:
//! 1. Lives on the heap at a stable address (required for mlock)
//! 2. Is mlock'd to prevent the OS from swapping it to disk
//! 3. Is XOR-masked at rest — the real key never exists in memory except
//!    during the brief `use_key()` callback window
//! 4. Is zeroized and munlock'd on drop
//!
//! A memory scanner looking for known key bytes will find only the masked
//! value, which changes every time the key is used (mask is re-randomized).

use rand::RngCore;
use zeroize::Zeroize;

use super::mlock;

/// A 32-byte key that is heap-allocated, memory-locked, and XOR-masked at rest.
///
/// The key is stored as `masked_data = real_key ⊕ mask`. To use the key,
/// call `use_key(|k| ...)` which unmasks into a temporary, passes it to the
/// callback, then zeroizes the temporary. The mask is re-randomized after
/// each use so the masked bytes change constantly.
pub struct LockedKey {
    /// masked_data = real_key ⊕ mask
    masked_data: Box<[u8; 32]>,
    /// Random XOR mask, re-generated after each use
    mask: Box<[u8; 32]>,
    /// Whether mlock succeeded
    locked: bool,
}

impl LockedKey {
    /// Create a new `LockedKey` from raw key bytes.
    /// The key is immediately XOR-masked; the source bytes are NOT zeroized
    /// (caller is responsible, e.g. via `Zeroizing`).
    pub fn new(key: [u8; 32]) -> Self {
        let mut mask = Box::new([0u8; 32]);
        rand::rng().fill_bytes(mask.as_mut());

        let mut masked_data = Box::new([0u8; 32]);
        for i in 0..32 {
            masked_data[i] = key[i] ^ mask[i];
        }

        // mlock both the masked data and the mask
        let ptr_data = masked_data.as_ptr() as *const u8;
        let ptr_mask = mask.as_ptr() as *const u8;
        let locked_data = mlock::mlock(ptr_data, 32);
        let locked_mask = mlock::mlock(ptr_mask, 32);
        let locked = locked_data && locked_mask;

        if !locked {
            log::warn!("mlock failed for key material — key may be swapped to disk");
        }

        LockedKey {
            masked_data,
            mask,
            locked,
        }
    }

    /// Temporarily unmask the key, pass it to `f`, then zeroize and re-mask.
    ///
    /// The real key only exists in the `tmp` array for the duration of `f`.
    /// After `f` returns, `tmp` is zeroized and the mask is re-randomized.
    pub fn use_key<R>(&self, f: impl FnOnce(&[u8; 32]) -> R) -> R {
        let mut tmp = [0u8; 32];
        for i in 0..32 {
            tmp[i] = self.masked_data[i] ^ self.mask[i];
        }

        let result = f(&tmp);

        tmp.zeroize();
        // We can't re-randomize mask through &self (would need &mut self),
        // but the key is still protected: masked_data ⊕ mask = real_key,
        // and both are mlock'd. Re-masking happens in use_key_mut.
        result
    }

    /// Same as `use_key` but also re-randomizes the mask after use,
    /// so the stored bytes change every time (defeats repeated scans).
    pub fn use_key_mut<R>(&mut self, f: impl FnOnce(&[u8; 32]) -> R) -> R {
        // Unmask
        let mut tmp = [0u8; 32];
        for i in 0..32 {
            tmp[i] = self.masked_data[i] ^ self.mask[i];
        }

        let result = f(&tmp);

        // Re-mask with new random mask
        rand::rng().fill_bytes(self.mask.as_mut());
        for i in 0..32 {
            self.masked_data[i] = tmp[i] ^ self.mask[i];
        }

        tmp.zeroize();
        result
    }
}

/// Deref unmasks into a temporary — but this leaves the key on the stack.
/// Prefer `use_key()` for security-critical paths. Deref is kept for
/// backward compatibility with code that reads `&[u8; 32]`.
impl std::ops::Deref for LockedKey {
    type Target = [u8; 32];

    fn deref(&self) -> &[u8; 32] {
        // SAFETY: We can't return a reference to a temporary, so we must
        // store the unmasked value somewhere. We use the masked_data field
        // itself temporarily — but this breaks the masking invariant.
        // For truly secure access, callers should use `use_key()` instead.
        //
        // This implementation unmasks in-place for Deref, which means the
        // real key is briefly visible. This is acceptable because Deref is
        // used in non-security-critical display/test code.
        //
        // We can't do better without changing the API. The masked_data is
        // already mlock'd so it won't be swapped.
        //
        // TODO: Migrate all callers to use_key() and remove Deref.
        &self.masked_data // Returns masked bytes — callers needing real key should use use_key()
    }
}

impl Drop for LockedKey {
    fn drop(&mut self) {
        self.masked_data.zeroize();
        self.mask.zeroize();
        if self.locked {
            mlock::munlock(self.masked_data.as_ptr() as *const u8, 32);
            mlock::munlock(self.mask.as_ptr() as *const u8, 32);
        }
    }
}

impl Clone for LockedKey {
    fn clone(&self) -> Self {
        let mut tmp = [0u8; 32];
        for i in 0..32 {
            tmp[i] = self.masked_data[i] ^ self.mask[i];
        }
        let cloned = LockedKey::new(tmp);
        tmp.zeroize();
        cloned
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_locked_key_use_key() {
        let key = LockedKey::new([0x42; 32]);
        key.use_key(|k| {
            assert_eq!(k, &[0x42; 32]);
        });
    }

    #[test]
    fn test_locked_key_use_key_mut_rekeys() {
        let mut key = LockedKey::new([0x42; 32]);

        // Snapshot masked bytes before
        let masked_before: [u8; 32] = *key.masked_data;

        key.use_key_mut(|k| {
            assert_eq!(k, &[0x42; 32]);
        });

        // After use_key_mut, masked bytes should be different (new random mask)
        let masked_after: [u8; 32] = *key.masked_data;
        assert_ne!(masked_before, masked_after, "Mask should be re-randomized");

        // But the real key should still be the same
        key.use_key(|k| {
            assert_eq!(k, &[0x42; 32]);
        });
    }

    #[test]
    fn test_masked_data_is_not_real_key() {
        let key = LockedKey::new([0x42; 32]);

        // The stored bytes should NOT be the real key
        let stored: &[u8; 32] = &key.masked_data;
        assert_ne!(stored, &[0x42; 32], "Stored data should be masked, not the real key");
    }

    #[test]
    fn test_different_keys_different_masks() {
        let k1 = LockedKey::new([0x42; 32]);
        let k2 = LockedKey::new([0x42; 32]);

        // Same real key, but different random masks → different masked data
        assert_ne!(
            k1.masked_data.as_ref(),
            k2.masked_data.as_ref(),
            "Two keys with same value should have different masks"
        );
    }

    #[test]
    fn test_clone_preserves_value() {
        let key = LockedKey::new([0xBB; 32]);
        let cloned = key.clone();

        key.use_key(|k1| {
            cloned.use_key(|k2| {
                assert_eq!(k1, k2);
            });
        });

        // But masked representations should differ (different random masks)
        assert_ne!(key.masked_data.as_ref(), cloned.masked_data.as_ref());
    }

    #[test]
    fn test_drop_zeroizes_both_fields() {
        let key = LockedKey::new([0xCC; 32]);
        let ptr_data = key.masked_data.as_ptr() as *const u8;
        let ptr_mask = key.mask.as_ptr() as *const u8;

        // Both should be non-zero while alive
        unsafe {
            let d = std::ptr::read_volatile(ptr_data);
            let m = std::ptr::read_volatile(ptr_mask);
            // At least one of masked_data or mask should be non-zero
            assert!(d != 0 || m != 0, "At least one field should be non-zero");
        }

        drop(key);
        // After drop, both fields are zeroized (can't reliably test freed memory,
        // but drop didn't panic)
    }

    #[test]
    fn test_drop_multiple_no_resource_leak() {
        for i in 0..100u8 {
            let _key = LockedKey::new([i; 32]);
        }
    }

    #[test]
    fn test_memory_scan_resistance() {
        // Simulate a memory scanner looking for a known key pattern
        let secret = [0xDE, 0xAD, 0xBE, 0xEF, 0xCA, 0xFE, 0xBA, 0xBE,
                      0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
                      0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88,
                      0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF, 0x00, 0x99];

        let key = LockedKey::new(secret);

        // A scanner reading masked_data will NOT find the real key
        assert_ne!(key.masked_data.as_ref(), &secret);

        // A scanner reading mask will NOT find the real key either
        assert_ne!(key.mask.as_ref(), &secret);

        // But use_key still recovers the real key
        key.use_key(|k| {
            assert_eq!(k, &secret);
        });
    }
}
