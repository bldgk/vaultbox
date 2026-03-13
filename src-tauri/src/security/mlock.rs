//! Memory locking to prevent key material from being swapped to disk.

/// Lock a memory region to prevent it from being swapped.
#[cfg(unix)]
pub fn mlock(ptr: *const u8, len: usize) -> bool {
    unsafe { libc::mlock(ptr as *const libc::c_void, len) == 0 }
}

#[cfg(windows)]
pub fn mlock(ptr: *const u8, len: usize) -> bool {
    unsafe {
        windows_sys::Win32::System::Memory::VirtualLock(
            ptr as *mut _,
            len,
        ) != 0
    }
}

/// Unlock a previously locked memory region.
#[cfg(unix)]
pub fn munlock(ptr: *const u8, len: usize) -> bool {
    unsafe { libc::munlock(ptr as *const libc::c_void, len) == 0 }
}

#[cfg(windows)]
pub fn munlock(ptr: *const u8, len: usize) -> bool {
    unsafe {
        windows_sys::Win32::System::Memory::VirtualUnlock(
            ptr as *mut _,
            len,
        ) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mlock_and_munlock() {
        let data = [0u8; 4096]; // page-aligned size
        let ptr = data.as_ptr();
        let len = data.len();

        // mlock may fail due to resource limits, that's ok
        let locked = mlock(ptr, len);
        if locked {
            let unlocked = munlock(ptr, len);
            assert!(unlocked, "munlock should succeed after mlock");
        }
    }

    #[test]
    fn test_mlock_small_buffer() {
        let data = [0xAA; 32]; // 32 bytes (key-sized)
        let locked = mlock(data.as_ptr(), data.len());
        if locked {
            assert!(munlock(data.as_ptr(), data.len()));
        }
    }

    #[test]
    fn test_mlock_heap_allocated() {
        let data: Vec<u8> = vec![0u8; 4096];
        let locked = mlock(data.as_ptr(), data.len());
        if locked {
            assert!(munlock(data.as_ptr(), data.len()));
        }
    }

    #[test]
    fn test_mlock_zero_length() {
        let data = [0u8; 1];
        // mlock with zero length — behavior is platform-specific but shouldn't panic
        let _ = mlock(data.as_ptr(), 0);
    }
}
