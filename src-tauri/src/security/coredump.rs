//! Disable core dumps to prevent key material from leaking.

/// Disable core dumps on Unix.
#[cfg(unix)]
pub fn disable_core_dumps() -> bool {
    unsafe {
        let rlimit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        libc::setrlimit(libc::RLIMIT_CORE, &rlimit) == 0
    }
}

/// Disable core dumps on Windows (no-op, handled differently).
#[cfg(windows)]
pub fn disable_core_dumps() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_disable_core_dumps_succeeds() {
        // Should succeed (or at least not panic)
        let result = disable_core_dumps();
        assert!(result);
    }

    #[test]
    fn test_disable_core_dumps_idempotent() {
        // Calling twice should still succeed
        assert!(disable_core_dumps());
        assert!(disable_core_dumps());
    }
}
