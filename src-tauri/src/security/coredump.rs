//! Disable core dumps and debugger attachment to prevent key material from leaking.

/// Disable core dumps on Unix and mark process as non-dumpable on Linux
/// (prevents ptrace attach from non-root processes).
#[cfg(unix)]
pub fn disable_core_dumps() -> bool {
    let rlimit_ok = unsafe {
        let rlimit = libc::rlimit {
            rlim_cur: 0,
            rlim_max: 0,
        };
        libc::setrlimit(libc::RLIMIT_CORE, &rlimit) == 0
    };

    // On Linux, also set PR_SET_DUMPABLE to 0 which:
    // 1. Prevents core dumps (belt-and-suspenders with setrlimit)
    // 2. Restricts /proc/pid/mem access to root only
    // 3. Prevents ptrace attach from same-user processes
    #[cfg(target_os = "linux")]
    {
        unsafe {
            libc::prctl(libc::PR_SET_DUMPABLE, 0, 0, 0, 0);
        }
    }

    rlimit_ok
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
