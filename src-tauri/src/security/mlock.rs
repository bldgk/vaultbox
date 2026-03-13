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
