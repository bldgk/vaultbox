//! Integration tests for vault file operations.
//!
//! These tests create real encrypted vault structures on disk and test
//! the full read/write/list/search/copy/delete pipeline.

use std::fs;

use vaultbox_lib::crypto::diriv;
use vaultbox_lib::vault::ops;

/// Create a minimal vault directory with a root diriv.
/// Returns (vault_path, filename_key, content_key).
fn setup_vault() -> (tempfile::TempDir, [u8; 32], [u8; 32]) {
    let dir = tempfile::tempdir().unwrap();
    diriv::create_diriv(dir.path()).unwrap();
    let filename_key = [0x42u8; 32];
    let content_key = [0x43u8; 32];
    (dir, filename_key, content_key)
}

#[test]
fn test_list_empty_directory() {
    let (vault, fk, ck) = setup_vault();
    let entries = ops::list_directory(vault.path(), "", &fk, &ck, true).unwrap();
    assert!(entries.is_empty());
}

#[test]
fn test_create_and_list_file() {
    let (vault, fk, ck) = setup_vault();

    ops::create_file(vault.path(), "", "hello.txt", &fk, &ck, true).unwrap();

    let entries = ops::list_directory(vault.path(), "", &fk, &ck, true).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "hello.txt");
    assert!(!entries[0].is_dir);
}

#[test]
fn test_create_and_list_multiple_files() {
    let (vault, fk, ck) = setup_vault();

    ops::create_file(vault.path(), "", "b.txt", &fk, &ck, true).unwrap();
    ops::create_file(vault.path(), "", "a.txt", &fk, &ck, true).unwrap();
    ops::create_file(vault.path(), "", "c.txt", &fk, &ck, true).unwrap();

    let entries = ops::list_directory(vault.path(), "", &fk, &ck, true).unwrap();
    assert_eq!(entries.len(), 3);
    // Should be sorted alphabetically
    assert_eq!(entries[0].name, "a.txt");
    assert_eq!(entries[1].name, "b.txt");
    assert_eq!(entries[2].name, "c.txt");
}

#[test]
fn test_write_and_read_file() {
    let (vault, fk, ck) = setup_vault();

    ops::create_file(vault.path(), "", "data.txt", &fk, &ck, true).unwrap();
    ops::write_file(vault.path(), "data.txt", b"hello encrypted world", &fk, &ck, true).unwrap();

    let data = ops::read_file(vault.path(), "data.txt", &fk, &ck, true).unwrap();
    assert_eq!(data.as_slice(), b"hello encrypted world");
}

#[test]
fn test_write_read_binary_data() {
    let (vault, fk, ck) = setup_vault();

    let binary_data: Vec<u8> = (0..256).map(|i| i as u8).collect();
    ops::create_file(vault.path(), "", "binary.bin", &fk, &ck, true).unwrap();
    ops::write_file(vault.path(), "binary.bin", &binary_data, &fk, &ck, true).unwrap();

    let data = ops::read_file(vault.path(), "binary.bin", &fk, &ck, true).unwrap();
    assert_eq!(*data, binary_data);
}

#[test]
fn test_write_overwrite_file() {
    let (vault, fk, ck) = setup_vault();

    ops::create_file(vault.path(), "", "test.txt", &fk, &ck, true).unwrap();
    ops::write_file(vault.path(), "test.txt", b"version 1", &fk, &ck, true).unwrap();
    ops::write_file(vault.path(), "test.txt", b"version 2", &fk, &ck, true).unwrap();

    let data = ops::read_file(vault.path(), "test.txt", &fk, &ck, true).unwrap();
    assert_eq!(data.as_slice(), b"version 2");
}

#[test]
fn test_create_directory() {
    let (vault, fk, ck) = setup_vault();

    ops::create_directory(vault.path(), "", "subdir", &fk, true).unwrap();

    let entries = ops::list_directory(vault.path(), "", &fk, &ck, true).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "subdir");
    assert!(entries[0].is_dir);
}

#[test]
fn test_directories_listed_before_files() {
    let (vault, fk, ck) = setup_vault();

    ops::create_file(vault.path(), "", "aaa.txt", &fk, &ck, true).unwrap();
    ops::create_directory(vault.path(), "", "zzz_dir", &fk, true).unwrap();

    let entries = ops::list_directory(vault.path(), "", &fk, &ck, true).unwrap();
    assert_eq!(entries.len(), 2);
    // Directory should come first even though its name sorts after
    assert!(entries[0].is_dir);
    assert_eq!(entries[0].name, "zzz_dir");
    assert!(!entries[1].is_dir);
    assert_eq!(entries[1].name, "aaa.txt");
}

#[test]
fn test_nested_directory_operations() {
    let (vault, fk, ck) = setup_vault();

    // Create nested structure: /parent/child/
    ops::create_directory(vault.path(), "", "parent", &fk, true).unwrap();
    ops::create_directory(vault.path(), "parent", "child", &fk, true).unwrap();

    // Create file in nested dir
    ops::create_file(vault.path(), "parent/child", "deep.txt", &fk, &ck, true).unwrap();
    ops::write_file(
        vault.path(),
        "parent/child/deep.txt",
        b"deep content",
        &fk,
        &ck,
        true,
    )
    .unwrap();

    // Read it back
    let data = ops::read_file(vault.path(), "parent/child/deep.txt", &fk, &ck, true).unwrap();
    assert_eq!(data.as_slice(), b"deep content");

    // List nested dir
    let entries = ops::list_directory(vault.path(), "parent/child", &fk, &ck, true).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "deep.txt");
}

#[test]
fn test_rename_file() {
    let (vault, fk, ck) = setup_vault();

    ops::create_file(vault.path(), "", "old.txt", &fk, &ck, true).unwrap();
    ops::write_file(vault.path(), "old.txt", b"content", &fk, &ck, true).unwrap();

    ops::rename_entry(vault.path(), "old.txt", "new.txt", &fk, true).unwrap();

    let entries = ops::list_directory(vault.path(), "", &fk, &ck, true).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "new.txt");

    // Content should still be readable under new name
    let data = ops::read_file(vault.path(), "new.txt", &fk, &ck, true).unwrap();
    assert_eq!(data.as_slice(), b"content");
}

#[test]
fn test_delete_file() {
    let (vault, fk, ck) = setup_vault();

    ops::create_file(vault.path(), "", "delete-me.txt", &fk, &ck, true).unwrap();
    let entries = ops::list_directory(vault.path(), "", &fk, &ck, true).unwrap();
    assert_eq!(entries.len(), 1);

    ops::delete_entry(vault.path(), "delete-me.txt", &fk, true).unwrap();

    let entries = ops::list_directory(vault.path(), "", &fk, &ck, true).unwrap();
    assert!(entries.is_empty());
}

#[test]
fn test_delete_directory_recursive() {
    let (vault, fk, ck) = setup_vault();

    ops::create_directory(vault.path(), "", "to-delete", &fk, true).unwrap();
    ops::create_file(vault.path(), "to-delete", "inner.txt", &fk, &ck, true).unwrap();
    ops::create_directory(vault.path(), "to-delete", "inner-dir", &fk, true).unwrap();

    ops::delete_entry(vault.path(), "to-delete", &fk, true).unwrap();

    let entries = ops::list_directory(vault.path(), "", &fk, &ck, true).unwrap();
    assert!(entries.is_empty());
}

#[test]
fn test_copy_file() {
    let (vault, fk, ck) = setup_vault();

    ops::create_file(vault.path(), "", "original.txt", &fk, &ck, true).unwrap();
    ops::write_file(vault.path(), "original.txt", b"copy me", &fk, &ck, true).unwrap();

    ops::copy_entry(vault.path(), "original.txt", "", "copied.txt", &fk, &ck, true).unwrap();

    // Both files should exist
    let entries = ops::list_directory(vault.path(), "", &fk, &ck, true).unwrap();
    assert_eq!(entries.len(), 2);

    // Both should have same content
    let orig = ops::read_file(vault.path(), "original.txt", &fk, &ck, true).unwrap();
    let copied = ops::read_file(vault.path(), "copied.txt", &fk, &ck, true).unwrap();
    assert_eq!(*orig, *copied);
    assert_eq!(orig.as_slice(), b"copy me");
}

#[test]
fn test_copy_file_to_subdirectory() {
    let (vault, fk, ck) = setup_vault();

    ops::create_file(vault.path(), "", "source.txt", &fk, &ck, true).unwrap();
    ops::write_file(vault.path(), "source.txt", b"file data", &fk, &ck, true).unwrap();
    ops::create_directory(vault.path(), "", "dest", &fk, true).unwrap();

    ops::copy_entry(vault.path(), "source.txt", "dest", "source.txt", &fk, &ck, true).unwrap();

    let data = ops::read_file(vault.path(), "dest/source.txt", &fk, &ck, true).unwrap();
    assert_eq!(data.as_slice(), b"file data");
}

#[test]
fn test_copy_directory_recursive() {
    let (vault, fk, ck) = setup_vault();

    // Create source structure
    ops::create_directory(vault.path(), "", "src-dir", &fk, true).unwrap();
    ops::create_file(vault.path(), "src-dir", "file1.txt", &fk, &ck, true).unwrap();
    ops::write_file(vault.path(), "src-dir/file1.txt", b"content1", &fk, &ck, true).unwrap();
    ops::create_file(vault.path(), "src-dir", "file2.txt", &fk, &ck, true).unwrap();
    ops::write_file(vault.path(), "src-dir/file2.txt", b"content2", &fk, &ck, true).unwrap();

    // Copy directory
    ops::copy_entry(vault.path(), "src-dir", "", "dst-dir", &fk, &ck, true).unwrap();

    // Verify copy
    let entries = ops::list_directory(vault.path(), "dst-dir", &fk, &ck, true).unwrap();
    assert_eq!(entries.len(), 2);

    let data1 = ops::read_file(vault.path(), "dst-dir/file1.txt", &fk, &ck, true).unwrap();
    assert_eq!(data1.as_slice(), b"content1");
    let data2 = ops::read_file(vault.path(), "dst-dir/file2.txt", &fk, &ck, true).unwrap();
    assert_eq!(data2.as_slice(), b"content2");
}

#[test]
fn test_search_files() {
    let (vault, fk, ck) = setup_vault();

    ops::create_file(vault.path(), "", "readme.md", &fk, &ck, true).unwrap();
    ops::create_file(vault.path(), "", "config.json", &fk, &ck, true).unwrap();
    ops::create_directory(vault.path(), "", "subdir", &fk, true).unwrap();
    ops::create_file(vault.path(), "subdir", "readme.txt", &fk, &ck, true).unwrap();

    // Search for "readme" — should find both
    let results = ops::search_files(vault.path(), "readme", &fk, &ck, true).unwrap();
    assert_eq!(results.len(), 2);

    // Search for "config"
    let results = ops::search_files(vault.path(), "config", &fk, &ck, true).unwrap();
    assert_eq!(results.len(), 1);
    assert!(results[0].name.contains("config"));
}

#[test]
fn test_search_case_insensitive() {
    let (vault, fk, ck) = setup_vault();

    ops::create_file(vault.path(), "", "README.md", &fk, &ck, true).unwrap();

    let results = ops::search_files(vault.path(), "readme", &fk, &ck, true).unwrap();
    assert_eq!(results.len(), 1);

    let results = ops::search_files(vault.path(), "README", &fk, &ck, true).unwrap();
    assert_eq!(results.len(), 1);
}

#[test]
fn test_search_no_results() {
    let (vault, fk, ck) = setup_vault();

    ops::create_file(vault.path(), "", "hello.txt", &fk, &ck, true).unwrap();

    let results = ops::search_files(vault.path(), "nonexistent", &fk, &ck, true).unwrap();
    assert!(results.is_empty());
}

#[test]
fn test_read_nonexistent_file() {
    let (vault, fk, ck) = setup_vault();
    let result = ops::read_file(vault.path(), "does-not-exist.txt", &fk, &ck, true);
    assert!(result.is_err());
}

#[test]
fn test_file_size_metadata() {
    let (vault, fk, ck) = setup_vault();

    let content = b"Hello, World!"; // 13 bytes
    ops::create_file(vault.path(), "", "sized.txt", &fk, &ck, true).unwrap();
    ops::write_file(vault.path(), "sized.txt", content, &fk, &ck, true).unwrap();

    let entries = ops::list_directory(vault.path(), "", &fk, &ck, true).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].size, 13);
}

#[test]
fn test_large_file_roundtrip() {
    let (vault, fk, ck) = setup_vault();

    // 100 KB of data spanning many blocks
    let large_data: Vec<u8> = (0..100_000).map(|i| (i % 251) as u8).collect();
    ops::create_file(vault.path(), "", "large.bin", &fk, &ck, true).unwrap();
    ops::write_file(vault.path(), "large.bin", &large_data, &fk, &ck, true).unwrap();

    let read_back = ops::read_file(vault.path(), "large.bin", &fk, &ck, true).unwrap();
    assert_eq!(*read_back, large_data);
}

#[test]
fn test_file_encrypted_on_disk() {
    let (vault, fk, ck) = setup_vault();

    let plaintext = b"this should be encrypted on disk";
    ops::create_file(vault.path(), "", "secret.txt", &fk, &ck, true).unwrap();
    ops::write_file(vault.path(), "secret.txt", plaintext, &fk, &ck, true).unwrap();

    // The actual filename on disk should be encrypted (not "secret.txt")
    let fs_entries: Vec<_> = fs::read_dir(vault.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n != "gocryptfs.diriv")
        .collect();
    assert_eq!(fs_entries.len(), 1);
    assert_ne!(fs_entries[0], "secret.txt");

    // The file content on disk should not contain the plaintext
    let encrypted_path = vault.path().join(&fs_entries[0]);
    let raw = fs::read(&encrypted_path).unwrap();
    // Search for plaintext bytes in raw data (should not be found)
    let found = raw
        .windows(plaintext.len())
        .any(|w| w == plaintext);
    assert!(!found, "Plaintext found in encrypted file on disk!");
}

#[test]
fn test_filename_encrypted_on_disk() {
    let (vault, fk, ck) = setup_vault();

    ops::create_file(vault.path(), "", "my-secret-file.txt", &fk, &ck, true).unwrap();

    // Check that "my-secret-file.txt" does not appear as a filename on disk
    let fs_entries: Vec<_> = fs::read_dir(vault.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();

    for entry in &fs_entries {
        assert_ne!(entry, "my-secret-file.txt");
        assert!(!entry.contains("my-secret"));
    }
}

#[test]
fn test_create_file_unicode_name() {
    let (vault, fk, ck) = setup_vault();

    ops::create_file(vault.path(), "", "日本語.txt", &fk, &ck, true).unwrap();

    let entries = ops::list_directory(vault.path(), "", &fk, &ck, true).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "日本語.txt");
}

#[test]
fn test_rename_directory() {
    let (vault, fk, ck) = setup_vault();

    ops::create_directory(vault.path(), "", "old-dir", &fk, true).unwrap();
    ops::create_file(vault.path(), "old-dir", "inner.txt", &fk, &ck, true).unwrap();

    ops::rename_entry(vault.path(), "old-dir", "new-dir", &fk, true).unwrap();

    let entries = ops::list_directory(vault.path(), "", &fk, &ck, true).unwrap();
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "new-dir");
    assert!(entries[0].is_dir);
}
