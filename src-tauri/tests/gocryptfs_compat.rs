//! Cross-validation tests with the real gocryptfs binary.
//!
//! Tests that require FUSE mount are marked #[ignore] — run them explicitly:
//!   cargo test --test gocryptfs_compat -- --ignored
//!
//! Tests that only need the gocryptfs binary (no mount) run normally.
//! Tests that use the existing cipher/ vault also run normally.

use std::path::Path;
use std::process::Command;

use vaultbox_lib::crypto::{config::GocryptfsConfig, content, diriv, filename, kdf};

fn gocryptfs_bin() -> String {
    std::env::var("GOCRYPTFS_BIN")
        .unwrap_or_else(|_| "gocryptfs".to_string()) // fallback to PATH
}

fn has_gocryptfs() -> bool {
    // Check PATH or explicit env var
    Command::new(gocryptfs_bin()).arg("--version").output().is_ok()
}

fn gocryptfs_init(cipher_dir: &Path, password: &str) {
    let output = Command::new(gocryptfs_bin())
        .args(["-init", "-extpass", &format!("echo {}", password), "-scryptn", "10", cipher_dir.to_str().unwrap()])
        .output()
        .expect("Failed to run gocryptfs -init");
    assert!(output.status.success(), "gocryptfs -init failed: {}", String::from_utf8_lossy(&output.stderr));
}

// ============================================================================
// NO FUSE — only needs gocryptfs binary for -init
// ============================================================================

/// Create vault with gocryptfs, derive master key with Rust — must match.
#[test]
fn test_rust_derives_same_master_key_as_gocryptfs_init() {
    if !has_gocryptfs() {
        eprintln!("SKIP: gocryptfs binary not found");
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let cipher = dir.path().join("cipher");
    std::fs::create_dir(&cipher).unwrap();

    let password = "cross-validation-pw";
    gocryptfs_init(&cipher, password);

    // Rust derives master key
    let config = GocryptfsConfig::load(&cipher).unwrap();
    let master = kdf::derive_master_key(password, &config).unwrap();
    assert_eq!(master.len(), 32);

    // Derive sub-keys — should not panic
    let ck = kdf::derive_content_key(&master).unwrap();
    let fk = kdf::derive_filename_key(&master).unwrap();
    assert_ne!(*ck, *fk);

    // Decrypt empty root directory — should work (only gocryptfs.conf + gocryptfs.diriv)
    let entries = vaultbox_lib::vault::ops::list_directory(
        &cipher, "", &fk, &ck, config.uses_raw64(),
    ).unwrap();
    assert!(entries.is_empty(), "Fresh vault should have no user files");
}

/// Create vault with gocryptfs, verify Rust can read config correctly.
#[test]
fn test_rust_reads_gocryptfs_config() {
    if !has_gocryptfs() {
        eprintln!("SKIP: gocryptfs binary not found");
        return;
    }

    let dir = tempfile::tempdir().unwrap();
    let cipher = dir.path().join("cipher");
    std::fs::create_dir(&cipher).unwrap();
    gocryptfs_init(&cipher, "testpw");

    let config = GocryptfsConfig::load(&cipher).unwrap();
    assert_eq!(config.version, 2);
    assert!(config.uses_hkdf());
    assert!(config.uses_raw64());
    assert!(config.uses_dir_iv());
    assert!(config.uses_eme_names());
    assert!(config.has_flag("GCMIV128"));
    assert!(config.uses_long_names());
}

// ============================================================================
// Uses existing cipher/ vault — no gocryptfs binary needed
// ============================================================================

/// Read the real cipher vault that was created by gocryptfs.
#[test]
fn test_decrypt_existing_cipher_vault() {
    let cipher_env = std::env::var("VAULTBOX_TEST_CIPHER").unwrap_or_else(|_| "cipher".into());
    let cipher = Path::new(&cipher_env);
    if !cipher.exists() {
        eprintln!("SKIP: cipher vault not found");
        return;
    }

    let config = GocryptfsConfig::load(cipher).unwrap();
    let master = kdf::derive_master_key("123456123", &config).unwrap();

    // Known master key
    let hex: String = master.iter().map(|b| format!("{:02x}", b)).collect();
    assert_eq!(hex, "b0cb316549ae0f38192e9a978e1dea41bee1ca5e6ae066b26cbf145404be994c");

    let ck = kdf::derive_content_key(&master).unwrap();
    let fk = kdf::derive_filename_key(&master).unwrap();

    let entries = vaultbox_lib::vault::ops::list_directory(
        cipher, "", &fk, &ck, config.uses_raw64(),
    ).unwrap();
    assert!(!entries.is_empty(), "Cipher vault should have files");

    // Read each file — must not error
    for entry in &entries {
        if entry.is_dir { continue; }
        let data = vaultbox_lib::vault::ops::read_file(
            cipher, &entry.name, &fk, &ck, config.uses_raw64(),
        ).unwrap();
        assert_eq!(data.len() as u64, entry.size, "Size mismatch for '{}'", entry.name);
    }
}

// ============================================================================
// FUSE MOUNT tests — require macFUSE/FUSE installed, run with --ignored
// ============================================================================

/// Create files with gocryptfs mount, decrypt with Rust.
#[test]
#[ignore = "requires FUSE mount — run with: cargo test --test gocryptfs_compat -- --ignored"]
fn test_cross_decrypt_filenames_via_fuse() {
    if !has_gocryptfs() {
        panic!("gocryptfs binary not found. Set GOCRYPTFS_BIN env var or add to PATH");
    }

    let dir = tempfile::tempdir().unwrap();
    let cipher = dir.path().join("cipher");
    let plain = dir.path().join("plain");
    std::fs::create_dir_all(&cipher).unwrap();
    std::fs::create_dir_all(&plain).unwrap();

    let password = "fuse-test-pw";
    gocryptfs_init(&cipher, password);

    let output = Command::new(gocryptfs_bin())
        .args(["-extpass", &format!("echo {}", password), cipher.to_str().unwrap(), plain.to_str().unwrap()])
        .output().unwrap();
    assert!(output.status.success(), "Mount failed: {}", String::from_utf8_lossy(&output.stderr));

    // Create files via FUSE
    std::fs::write(plain.join("hello.txt"), "Hello from gocryptfs!").unwrap();
    std::fs::write(plain.join("photo.png"), vec![0x89, 0x50, 0x4E, 0x47]).unwrap(); // PNG header
    std::fs::create_dir(plain.join("subdir")).unwrap();
    std::fs::write(plain.join("subdir/nested.md"), "# Nested file").unwrap();

    // Unmount
    #[cfg(target_os = "macos")]
    { Command::new("umount").arg(plain.to_str().unwrap()).output().ok(); }
    #[cfg(target_os = "linux")]
    { Command::new("fusermount").args(["-u", plain.to_str().unwrap()]).output().ok(); }

    // Decrypt filenames with Rust
    let config = GocryptfsConfig::load(&cipher).unwrap();
    let master = kdf::derive_master_key(password, &config).unwrap();
    let fk = kdf::derive_filename_key(&master).unwrap();
    let ck = kdf::derive_content_key(&master).unwrap();

    let entries = vaultbox_lib::vault::ops::list_directory(&cipher, "", &fk, &ck, config.uses_raw64()).unwrap();
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert!(names.contains(&"hello.txt"), "Missing hello.txt in {:?}", names);
    assert!(names.contains(&"photo.png"), "Missing photo.png in {:?}", names);
    assert!(names.contains(&"subdir"), "Missing subdir in {:?}", names);
}

/// Create files with gocryptfs mount, decrypt file content with Rust.
#[test]
#[ignore = "requires FUSE mount — run with: cargo test --test gocryptfs_compat -- --ignored"]
fn test_cross_decrypt_content_via_fuse() {
    if !has_gocryptfs() {
        panic!("gocryptfs binary not found. Set GOCRYPTFS_BIN env var or add to PATH");
    }

    let dir = tempfile::tempdir().unwrap();
    let cipher = dir.path().join("cipher");
    let plain = dir.path().join("plain");
    std::fs::create_dir_all(&cipher).unwrap();
    std::fs::create_dir_all(&plain).unwrap();

    let password = "content-fuse-pw";
    gocryptfs_init(&cipher, password);

    let output = Command::new(gocryptfs_bin())
        .args(["-extpass", &format!("echo {}", password), cipher.to_str().unwrap(), plain.to_str().unwrap()])
        .output().unwrap();
    assert!(output.status.success(), "Mount failed");

    let test_data: Vec<(&str, Vec<u8>)> = vec![
        ("empty.txt", vec![]),
        ("one_byte.bin", vec![0x42]),
        ("hello.txt", b"Hello from gocryptfs!".to_vec()),
        ("exact_block.bin", vec![0xAB; 4096]),
        ("two_blocks.bin", vec![0xCD; 8192]),
        ("multi.bin", (0..10000u32).map(|i| (i % 251) as u8).collect()),
    ];

    for (name, data) in &test_data {
        std::fs::write(plain.join(name), data).unwrap();
    }

    #[cfg(target_os = "macos")]
    { Command::new("umount").arg(plain.to_str().unwrap()).output().ok(); }
    #[cfg(target_os = "linux")]
    { Command::new("fusermount").args(["-u", plain.to_str().unwrap()]).output().ok(); }

    let config = GocryptfsConfig::load(&cipher).unwrap();
    let master = kdf::derive_master_key(password, &config).unwrap();
    let fk = kdf::derive_filename_key(&master).unwrap();
    let ck = kdf::derive_content_key(&master).unwrap();

    for (name, expected) in &test_data {
        let data = vaultbox_lib::vault::ops::read_file(&cipher, name, &fk, &ck, config.uses_raw64()).unwrap();
        assert_eq!(data.as_slice(), expected.as_slice(), "Content mismatch for '{}'", name);
    }
}
