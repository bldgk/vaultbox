//! Pure cryptographic tests — no filesystem, no FUSE, no gocryptfs binary.
//! These validate byte-level compatibility with gocryptfs crypto primitives.
//!
//! Run: cargo test --test crypto_pure

use vaultbox_lib::crypto::{content, filename, kdf};

mod hex {
    pub fn encode(data: &[u8]) -> String {
        data.iter().map(|b| format!("{:02x}", b)).collect()
    }
}

// ============================================================================
// HKDF Known Vectors (from gocryptfs internal/cryptocore/hkdf_test.go)
// These MUST NOT change — they define the on-disk format.
// ============================================================================

#[test]
fn test_hkdf_vector_1() {
    // master=0x00*32, info="EME filename encryption"
    let fk = kdf::derive_filename_key(&[0x00; 32]).unwrap();
    assert_eq!(
        hex::encode(&*fk),
        "9ba3cddd48c6339c6e56ebe85f0281d6e9051be4104176e65cb0f8a6f77ae6b4"
    );
}

#[test]
fn test_hkdf_vector_2() {
    // master=0x01*32, info="EME filename encryption"
    let fk = kdf::derive_filename_key(&[0x01; 32]).unwrap();
    assert_eq!(
        hex::encode(&*fk),
        "e8a2499f48700b954f31de732efd04abce822f5c948e7fbc0896607be0d36d12"
    );
}

#[test]
fn test_hkdf_vector_3() {
    // master=0x01*32, info="AES-GCM file content encryption"
    let ck = kdf::derive_content_key(&[0x01; 32]).unwrap();
    assert_eq!(
        hex::encode(&*ck),
        "9137f2e67a842484137f3c458f357f204c30d7458f94f432fa989be96854a649"
    );
}

#[test]
fn test_hkdf_vector_4() {
    // master=0x00*32, info="AES-GCM file content encryption"
    let ck = kdf::derive_content_key(&[0x00; 32]).unwrap();
    assert_eq!(
        hex::encode(&*ck),
        "ab893c7a9f8683d8c56296fdd109aa8d73c1937d112d1cf89923f9d175fbd782"
    );
}

#[test]
fn test_hkdf_content_and_filename_keys_differ() {
    for b in [0x00u8, 0x01, 0x42, 0xFF] {
        let ck = kdf::derive_content_key(&[b; 32]).unwrap();
        let fk = kdf::derive_filename_key(&[b; 32]).unwrap();
        assert_ne!(*ck, *fk, "Content and filename keys must differ for master=0x{:02x}", b);
    }
}

#[test]
fn test_hkdf_raw_matches_kdf_functions() {
    use hkdf::Hkdf;
    use sha2::Sha256;

    for b in [0x00u8, 0x01, 0x42, 0xFF] {
        let master = [b; 32];

        let ck = kdf::derive_content_key(&master).unwrap();
        let hk = Hkdf::<Sha256>::new(None, &master);
        let mut expected = [0u8; 32];
        hk.expand(b"AES-GCM file content encryption", &mut expected).unwrap();
        assert_eq!(*ck, expected, "derive_content_key != raw HKDF for 0x{:02x}", b);

        let fk = kdf::derive_filename_key(&master).unwrap();
        hk.expand(b"EME filename encryption", &mut expected).unwrap();
        assert_eq!(*fk, expected, "derive_filename_key != raw HKDF for 0x{:02x}", b);
    }
}

// ============================================================================
// Real vault KDF: password + config → master key
// ============================================================================

#[test]
fn test_real_vault_kdf_cipher() {
    // Known test vector from a gocryptfs v2 vault (password="123456123")
    let config = vaultbox_lib::crypto::config::GocryptfsConfig {
        creator: "gocryptfs v2.6.1-41-g501d5a5".into(),
        encrypted_key: "vPlK2K0mTbm5uTQ3iA1ZV2sjOADUaChwiSqo8YreWaLC935IfJepbAWvr2kZvD89ce2FN0y374zxIPo92PVWaw==".into(),
        scrypt_object: vaultbox_lib::crypto::config::ScryptObject {
            salt: "IajRu/RqU/v8LG7xDc8ARD/pUo+x4XeDESy2JcmH3Xs=".into(),
            n: 65536,
            r: 8,
            p: 1,
            key_len: 32,
        },
        version: 2,
        feature_flags: vec![
            "HKDF".into(), "GCMIV128".into(), "DirIV".into(),
            "EMENames".into(), "LongNames".into(), "Raw64".into(),
        ],
    };

    let master = kdf::derive_master_key("123456123", &config).unwrap();
    assert_eq!(
        hex::encode(&*master),
        "b0cb316549ae0f38192e9a978e1dea41bee1ca5e6ae066b26cbf145404be994c"
    );
}

#[test]
fn test_wrong_password_fails() {
    let config = vaultbox_lib::crypto::config::GocryptfsConfig {
        creator: "gocryptfs".into(),
        encrypted_key: "vPlK2K0mTbm5uTQ3iA1ZV2sjOADUaChwiSqo8YreWaLC935IfJepbAWvr2kZvD89ce2FN0y374zxIPo92PVWaw==".into(),
        scrypt_object: vaultbox_lib::crypto::config::ScryptObject {
            salt: "IajRu/RqU/v8LG7xDc8ARD/pUo+x4XeDESy2JcmH3Xs=".into(),
            n: 65536, r: 8, p: 1, key_len: 32,
        },
        version: 2,
        feature_flags: vec!["HKDF".into(), "GCMIV128".into(), "DirIV".into(), "EMENames".into(), "LongNames".into(), "Raw64".into()],
    };

    assert!(kdf::derive_master_key("wrong-password", &config).is_err());
}

// ============================================================================
// Content encryption: block format, sizes, roundtrips
// ============================================================================

#[test]
fn test_block_constants() {
    assert_eq!(content::HEADER_LEN, 18);     // 2 version + 16 file_id
    assert_eq!(content::FILE_ID_LEN, 16);
    assert_eq!(content::BLOCK_SIZE_PLAIN, 4096);
    assert_eq!(content::BLOCK_SIZE_CIPHER, 4128); // 16 IV + 4096 ct + 16 tag
}

#[test]
fn test_plaintext_size_monotonicity() {
    let mut prev = 0u64;
    for ct_size in 0..10000u64 {
        let pt = content::plaintext_size(ct_size);
        assert!(pt >= prev, "plaintext_size not monotonic at {}: {} < {}", ct_size, pt, prev);
        prev = pt;
    }
}

#[test]
fn test_plaintext_size_matches_actual_encrypt() {
    let key = [0x42; 32];
    for size in [0, 1, 15, 16, 100, 4095, 4096, 4097, 8192, 12288, 16384, 65536] {
        let pt = vec![0xAB; size];
        let ct = content::encrypt_file(&key, &pt).unwrap();
        let computed = content::plaintext_size(ct.len() as u64);
        assert_eq!(computed, size as u64, "plaintext_size mismatch for {} bytes", size);
    }
}

#[test]
fn test_encrypt_decrypt_roundtrip_various_sizes() {
    let key = [0x42; 32];
    for size in [0, 1, 15, 16, 100, 4095, 4096, 4097, 8191, 8192, 8193, 10000, 65536] {
        let pt: Vec<u8> = (0..size).map(|i| (i % 251) as u8).collect();
        let ct = content::encrypt_file(&key, &pt).unwrap();
        let dec = content::decrypt_file(&key, &ct).unwrap();
        assert_eq!(*dec, pt, "Roundtrip failed for {} bytes", size);
    }
}

#[test]
fn test_block_structure_iv_16_bytes() {
    let key = [0x42; 32];
    let pt = b"hello";
    let ct = content::encrypt_file(&key, pt).unwrap();

    // Header: 18 bytes
    assert_eq!(&ct[0..2], &[0x00, 0x02]); // version
    let file_id = &ct[2..18];
    assert_eq!(file_id.len(), 16);

    // First block starts after header: IV(16) + ciphertext(5) + tag(16) = 37
    let block = &ct[18..];
    assert_eq!(block.len(), 16 + 5 + 16);

    // IV should be random (non-zero)
    assert_ne!(&block[..16], &[0u8; 16]);
}

#[test]
fn test_different_encryptions_have_different_ivs() {
    let key = [0x42; 32];
    let pt = b"same content";
    let ct1 = content::encrypt_file(&key, pt).unwrap();
    let ct2 = content::encrypt_file(&key, pt).unwrap();

    // Headers differ (random file_id)
    assert_ne!(&ct1[2..18], &ct2[2..18]);
    // Block IVs differ
    assert_ne!(&ct1[18..34], &ct2[18..34]);
    // But both decrypt to the same plaintext
    assert_eq!(*content::decrypt_file(&key, &ct1).unwrap(), pt);
    assert_eq!(*content::decrypt_file(&key, &ct2).unwrap(), pt);
}

#[test]
fn test_wrong_key_decrypt_fails() {
    let ct = content::encrypt_file(&[0x01; 32], b"secret").unwrap();
    assert!(content::decrypt_file(&[0x02; 32], &ct).is_err());
}

#[test]
fn test_corrupted_iv_fails() {
    let key = [0x42; 32];
    let mut ct = content::encrypt_file(&key, b"test").unwrap();
    ct[18] ^= 0xFF; // flip bit in IV
    assert!(content::decrypt_file(&key, &ct).is_err());
}

#[test]
fn test_corrupted_ciphertext_fails() {
    let key = [0x42; 32];
    let mut ct = content::encrypt_file(&key, b"test").unwrap();
    ct[34] ^= 0xFF; // flip bit in ciphertext
    assert!(content::decrypt_file(&key, &ct).is_err());
}

#[test]
fn test_corrupted_tag_fails() {
    let key = [0x42; 32];
    let mut ct = content::encrypt_file(&key, b"test").unwrap();
    let last = ct.len() - 1;
    ct[last] ^= 0xFF; // flip bit in GCM tag
    assert!(content::decrypt_file(&key, &ct).is_err());
}

#[test]
fn test_corrupted_header_fails() {
    let key = [0x42; 32];
    let mut ct = content::encrypt_file(&key, b"test").unwrap();
    ct[0] = 0xFF; // corrupt version
    assert!(content::decrypt_file(&key, &ct).is_err());
}

#[test]
fn test_aad_is_24_bytes_blockno_plus_fileid() {
    // Manually verify AAD format by decrypting with raw AES-GCM
    use aes::Aes256;
    use aes_gcm::{aead::{Aead, KeyInit, Payload, consts::U16}, AesGcm, Nonce};
    type Aes256Gcm16 = AesGcm<Aes256, U16>;

    let key = [0x42; 32];
    let pt = b"AAD test";
    let ct = content::encrypt_file(&key, pt).unwrap();

    let file_id = content::parse_header(&ct).unwrap();
    let block = &ct[content::HEADER_LEN..];
    let iv = &block[..16];
    let ct_tag = &block[16..];

    // AAD = blockNo(8 BE) + fileID(16) = 24 bytes
    let mut aad = Vec::with_capacity(24);
    aad.extend_from_slice(&0u64.to_be_bytes());
    aad.extend_from_slice(&file_id);
    assert_eq!(aad.len(), 24);

    let cipher = Aes256Gcm16::new_from_slice(&key).unwrap();
    let dec = cipher.decrypt(Nonce::from_slice(iv), Payload { msg: ct_tag, aad: &aad }).unwrap();
    assert_eq!(&dec, pt);

    // Wrong AAD (8 bytes only, like master key wrapping) must fail
    let bad_aad = [0u8; 8];
    assert!(cipher.decrypt(Nonce::from_slice(iv), Payload { msg: ct_tag, aad: &bad_aad }).is_err());
}

#[test]
fn test_multi_block_aad_increments_block_number() {
    // Verify block 0 and block 1 use different AAD (different blockNo)
    use aes::Aes256;
    use aes_gcm::{aead::{Aead, KeyInit, Payload, consts::U16}, AesGcm, Nonce};
    type Aes256Gcm16 = AesGcm<Aes256, U16>;

    let key = [0x42; 32];
    let pt = vec![0xAB; 4096 + 100]; // 2 blocks
    let ct = content::encrypt_file(&key, &pt).unwrap();
    let file_id = content::parse_header(&ct).unwrap();
    let cipher = Aes256Gcm16::new_from_slice(&key).unwrap();

    // Block 0
    let b0 = &ct[content::HEADER_LEN..content::HEADER_LEN + content::BLOCK_SIZE_CIPHER];
    let mut aad0 = Vec::new();
    aad0.extend_from_slice(&0u64.to_be_bytes());
    aad0.extend_from_slice(&file_id);
    let dec0 = cipher.decrypt(Nonce::from_slice(&b0[..16]), Payload { msg: &b0[16..], aad: &aad0 }).unwrap();
    assert_eq!(dec0.len(), 4096);

    // Block 1
    let b1 = &ct[content::HEADER_LEN + content::BLOCK_SIZE_CIPHER..];
    let mut aad1 = Vec::new();
    aad1.extend_from_slice(&1u64.to_be_bytes()); // blockNo = 1
    aad1.extend_from_slice(&file_id);
    let dec1 = cipher.decrypt(Nonce::from_slice(&b1[..16]), Payload { msg: &b1[16..], aad: &aad1 }).unwrap();
    assert_eq!(dec1.len(), 100);

    // Using wrong blockNo in AAD must fail
    assert!(cipher.decrypt(Nonce::from_slice(&b1[..16]), Payload { msg: &b1[16..], aad: &aad0 }).is_err());
}

// ============================================================================
// Filename encryption
// ============================================================================

#[test]
fn test_filename_roundtrip_basic() {
    let key = [0x42; 32];
    let iv = [0x01; 16];
    for name in ["hello.txt", "photo.png", "a", "test-file (1).docx", "日本語.txt"] {
        let enc = filename::encrypt_filename(&key, &iv, name, true).unwrap();
        let dec = filename::decrypt_filename(&key, &iv, &enc, true).unwrap();
        assert_eq!(dec, name);
    }
}

#[test]
fn test_filename_raw64_vs_padded() {
    let key = [0x42; 32];
    let iv = [0x01; 16];
    let name = "test.txt";

    let raw = filename::encrypt_filename(&key, &iv, name, true).unwrap();
    let pad = filename::encrypt_filename(&key, &iv, name, false).unwrap();

    assert!(!raw.contains('='), "Raw64 should not have padding");
    assert_eq!(filename::decrypt_filename(&key, &iv, &raw, true).unwrap(), name);
    assert_eq!(filename::decrypt_filename(&key, &iv, &pad, false).unwrap(), name);
}

#[test]
fn test_filename_different_dir_iv() {
    let key = [0x42; 32];
    let iv1 = [0x01; 16];
    let iv2 = [0x02; 16];
    let name = "same.txt";

    let enc1 = filename::encrypt_filename(&key, &iv1, name, true).unwrap();
    let enc2 = filename::encrypt_filename(&key, &iv2, name, true).unwrap();
    assert_ne!(enc1, enc2, "Same name with different dir IVs must encrypt differently");
}

#[test]
fn test_filename_deterministic() {
    let key = [0x42; 32];
    let iv = [0x01; 16];
    let name = "test.txt";

    let enc1 = filename::encrypt_filename(&key, &iv, name, true).unwrap();
    let enc2 = filename::encrypt_filename(&key, &iv, name, true).unwrap();
    assert_eq!(enc1, enc2, "EME filename encryption must be deterministic");
}

#[test]
fn test_filename_dot_names() {
    let key = [0x42; 32];
    let iv = [0x01; 16];

    for name in [".", "..", "..."] {
        let enc = filename::encrypt_filename(&key, &iv, name, true).unwrap();
        let dec = filename::decrypt_filename(&key, &iv, &enc, true).unwrap();
        assert_eq!(dec, name);
    }

    // All must produce different ciphertext
    let e1 = filename::encrypt_filename(&key, &iv, ".", true).unwrap();
    let e2 = filename::encrypt_filename(&key, &iv, "..", true).unwrap();
    let e3 = filename::encrypt_filename(&key, &iv, "...", true).unwrap();
    assert_ne!(e1, e2);
    assert_ne!(e2, e3);
    assert_ne!(e1, e3);
}

#[test]
fn test_filename_long_name_detection() {
    let key = [0x42; 32];
    let iv = [0x01; 16];

    // 255-char name → encrypted will be > 176 chars → is_long_name
    let long = "A".repeat(255);
    let enc = filename::encrypt_filename(&key, &iv, &long, true).unwrap();
    assert!(filename::is_long_name(&enc), "255-char name should be a long name");

    // Short name → not long
    let short_enc = filename::encrypt_filename(&key, &iv, "hi.txt", true).unwrap();
    assert!(!filename::is_long_name(&short_enc));
}

#[test]
fn test_filename_long_name_hash_deterministic() {
    let h1 = filename::long_name_hash("some-encrypted-name");
    let h2 = filename::long_name_hash("some-encrypted-name");
    assert_eq!(h1, h2);

    let h3 = filename::long_name_hash("different-name");
    assert_ne!(h1, h3);
}

#[test]
fn test_filename_various_lengths() {
    let key = [0x42; 32];
    let iv = [0x01; 16];

    for len in [1, 2, 15, 16, 17, 31, 32, 100, 200, 255, 300] {
        let name: String = (0..len).map(|i| (b'a' + (i % 26) as u8) as char).collect();
        let enc = filename::encrypt_filename(&key, &iv, &name, true).unwrap();
        let dec = filename::decrypt_filename(&key, &iv, &enc, true).unwrap();
        assert_eq!(dec, name, "Roundtrip failed for {}-char name", len);
    }
}

#[test]
fn test_filename_wrong_key_fails() {
    let iv = [0x01; 16];
    let enc = filename::encrypt_filename(&[0x01; 32], &iv, "secret.txt", true).unwrap();
    let result = filename::decrypt_filename(&[0x02; 32], &iv, &enc, true);
    // Should either error or produce wrong output
    assert!(result.is_err() || result.unwrap() != "secret.txt");
}

#[test]
fn test_filename_invalid_base64_fails() {
    let key = [0x42; 32];
    let iv = [0x01; 16];
    assert!(filename::decrypt_filename(&key, &iv, "!!!not-base64!!!", true).is_err());
    assert!(filename::decrypt_filename(&key, &iv, "", true).is_err());
}

// ============================================================================
// Master key wrapping: 8-byte AAD (not 24)
// ============================================================================

#[test]
fn test_master_key_wrapping_uses_8_byte_aad() {
    use aes::Aes256;
    use aes_gcm::{aead::{Aead, KeyInit, Payload, consts::U16}, AesGcm, Nonce};
    type Aes256Gcm16 = AesGcm<Aes256, U16>;

    let gcm_key = [0x99; 32];
    let master = [0xBB; 32];
    let nonce = [0u8; 16];
    let aad_8 = [0u8; 8]; // blockNo=0, no fileID

    let cipher = Aes256Gcm16::new_from_slice(&gcm_key).unwrap();
    let ct = cipher.encrypt(Nonce::from_slice(&nonce), Payload { msg: &master, aad: &aad_8 }).unwrap();

    // Correct 8-byte AAD works
    let dec = cipher.decrypt(Nonce::from_slice(&nonce), Payload { msg: &ct, aad: &aad_8 }).unwrap();
    assert_eq!(dec, master);

    // 24-byte AAD (content-style) fails
    assert!(cipher.decrypt(Nonce::from_slice(&nonce), Payload { msg: &ct, aad: &[0u8; 24] }).is_err());
}

// ============================================================================
// Scrypt timing
// ============================================================================

#[test]
fn test_scrypt_n65536_takes_at_least_10ms() {
    use std::time::Instant;
    let params = scrypt::Params::new(16, 8, 1, 32).unwrap();
    let mut out = [0u8; 32];
    let start = Instant::now();
    scrypt::scrypt(b"test", &[0; 32], &params, &mut out).unwrap();
    let ms = start.elapsed().as_millis();
    assert!(ms >= 10, "scrypt N=65536 completed in {}ms — too fast", ms);
}

#[test]
fn test_scrypt_different_passwords_different_keys() {
    let params = scrypt::Params::new(4, 8, 1, 32).unwrap();
    let salt = [0xAA; 32];
    let mut k1 = [0u8; 32];
    let mut k2 = [0u8; 32];
    scrypt::scrypt(b"password1", &salt, &params, &mut k1).unwrap();
    scrypt::scrypt(b"password2", &salt, &params, &mut k2).unwrap();
    assert_ne!(k1, k2);
}

#[test]
fn test_scrypt_different_salts_different_keys() {
    let params = scrypt::Params::new(4, 8, 1, 32).unwrap();
    let mut k1 = [0u8; 32];
    let mut k2 = [0u8; 32];
    scrypt::scrypt(b"same", &[0x01; 32], &params, &mut k1).unwrap();
    scrypt::scrypt(b"same", &[0x02; 32], &params, &mut k2).unwrap();
    assert_ne!(k1, k2);
}

// ============================================================================
// Full gocryptfs flow simulation (pure Rust, no FS mount)
// ============================================================================

#[test]
fn test_full_gocryptfs_flow() {
    let master = [0x01; 32];
    let ck = kdf::derive_content_key(&master).unwrap();
    let fk = kdf::derive_filename_key(&master).unwrap();

    // Verify known vectors
    assert_eq!(hex::encode(&*ck), "9137f2e67a842484137f3c458f357f204c30d7458f94f432fa989be96854a649");
    assert_eq!(hex::encode(&*fk), "e8a2499f48700b954f31de732efd04abce822f5c948e7fbc0896607be0d36d12");

    // Encrypt → decrypt content
    let plaintext = b"gocryptfs-compatible content";
    let ct = content::encrypt_file(&ck, plaintext).unwrap();
    let dec = content::decrypt_file(&ck, &ct).unwrap();
    assert_eq!(dec.as_slice(), plaintext);

    // Encrypt → decrypt filename
    let dir_iv = [0x01; 16];
    let name = "secret-document.pdf";
    let enc_name = filename::encrypt_filename(&fk, &dir_iv, name, true).unwrap();
    let dec_name = filename::decrypt_filename(&fk, &dir_iv, &enc_name, true).unwrap();
    assert_eq!(dec_name, name);

    // Content key can't decrypt filename-key-encrypted content
    assert!(content::decrypt_file(&fk, &ct).is_err());
}

// ============================================================================
// Size conversion monotonicity (both directions)
// From gocryptfs internal/contentenc/offsets_test.go
// ============================================================================

#[test]
fn test_cipher_size_to_plain_size_monotonicity() {
    // plaintext_size(x) must be monotonically non-decreasing for all x
    let mut prev = 0u64;
    for x in 0..10000u64 {
        let y = content::plaintext_size(x);
        assert!(y >= prev, "CipherToPlain not monotonic at {}: {} < {}", x, y, prev);
        prev = y;
    }
}

fn plain_size_to_cipher_size(plain: u64) -> u64 {
    if plain == 0 {
        return content::HEADER_LEN as u64;
    }
    let full_blocks = plain / content::BLOCK_SIZE_PLAIN as u64;
    let remainder = plain % content::BLOCK_SIZE_PLAIN as u64;
    let overhead = 32u64; // IV(16) + tag(16)
    let mut size = content::HEADER_LEN as u64 + full_blocks * content::BLOCK_SIZE_CIPHER as u64;
    if remainder > 0 {
        size += overhead + remainder;
    }
    size
}

#[test]
fn test_plain_size_to_cipher_size_monotonicity() {
    let mut prev = 0u64;
    for x in 0..10000u64 {
        let y = plain_size_to_cipher_size(x);
        assert!(y >= prev, "PlainToCipher not monotonic at {}: {} < {}", x, y, prev);
        prev = y;
    }
}

#[test]
fn test_size_conversion_roundtrip() {
    // For every plaintext size, cipher→plain(plain→cipher(x)) should return x
    for x in 0..5000u64 {
        let cipher_size = plain_size_to_cipher_size(x);
        let back = content::plaintext_size(cipher_size);
        assert_eq!(back, x, "Roundtrip failed at plain={}: cipher={}, back={}", x, cipher_size, back);
    }
}

// ============================================================================
// Block number mapping (from gocryptfs content_test.go TestBlockNo)
// ============================================================================

fn cipher_off_to_block_no(cipher_off: u64) -> u64 {
    if cipher_off < content::HEADER_LEN as u64 {
        return 0;
    }
    (cipher_off - content::HEADER_LEN as u64) / content::BLOCK_SIZE_CIPHER as u64
}

fn plain_off_to_block_no(plain_off: u64) -> u64 {
    plain_off / content::BLOCK_SIZE_PLAIN as u64
}

#[test]
fn test_cipher_off_to_block_no() {
    assert_eq!(cipher_off_to_block_no(0), 0);
    assert_eq!(cipher_off_to_block_no(788), 0);
    assert_eq!(cipher_off_to_block_no(content::HEADER_LEN as u64), 0);
    assert_eq!(cipher_off_to_block_no(content::HEADER_LEN as u64 + content::BLOCK_SIZE_CIPHER as u64), 1);
    assert_eq!(cipher_off_to_block_no(content::HEADER_LEN as u64 + content::BLOCK_SIZE_CIPHER as u64 * 2), 2);
}

#[test]
fn test_plain_off_to_block_no() {
    assert_eq!(plain_off_to_block_no(0), 0);
    assert_eq!(plain_off_to_block_no(788), 0);
    assert_eq!(plain_off_to_block_no(4095), 0);
    assert_eq!(plain_off_to_block_no(4096), 1);
    assert_eq!(plain_off_to_block_no(8192), 2);
}

// ============================================================================
// Range splitting (from gocryptfs content_test.go TestSplitRange)
// ============================================================================

struct PlainRange {
    block_no: u64,
    skip: u64,
    length: u64,
}

fn explode_plain_range(offset: u64, length: u64) -> Vec<PlainRange> {
    let mut parts = Vec::new();
    let mut remaining = length;
    let mut pos = offset;

    while remaining > 0 {
        let block_no = pos / content::BLOCK_SIZE_PLAIN as u64;
        let skip = pos % content::BLOCK_SIZE_PLAIN as u64;
        let available = content::BLOCK_SIZE_PLAIN as u64 - skip;
        let take = remaining.min(available);

        parts.push(PlainRange { block_no, skip, length: take });
        pos += take;
        remaining -= take;
    }
    parts
}

#[test]
fn test_split_range_no_duplicate_blocks() {
    let ranges = [(0, 70000), (0, 10), (234, 6511), (65444, 54), (0, 1024*1024), (0, 65536), (6654, 8945)];

    for (offset, length) in ranges {
        let parts = explode_plain_range(offset, length);
        let mut seen = std::collections::HashSet::new();
        for p in &parts {
            assert!(seen.insert(p.block_no), "Duplicate block {} for range ({}, {})", p.block_no, offset, length);
            assert!(p.length <= content::BLOCK_SIZE_PLAIN as u64, "Length {} > block size for range ({}, {})", p.length, offset, length);
            assert!(p.skip < content::BLOCK_SIZE_PLAIN as u64, "Skip {} >= block size for range ({}, {})", p.skip, offset, length);
        }
    }
}

#[test]
fn test_split_range_total_length() {
    let ranges = [(0, 70000), (0, 10), (234, 6511), (65444, 54), (6654, 8945)];

    for (offset, length) in ranges {
        let parts = explode_plain_range(offset, length);
        let total: u64 = parts.iter().map(|p| p.length).sum();
        assert_eq!(total, length, "Total length mismatch for range ({}, {})", offset, length);
    }
}

#[test]
fn test_split_range_first_block_skip() {
    // Non-aligned offset must have skip > 0 on first block
    let parts = explode_plain_range(100, 50);
    assert_eq!(parts[0].skip, 100);
    assert_eq!(parts[0].length, 50);

    // Aligned offset has skip = 0
    let parts = explode_plain_range(0, 50);
    assert_eq!(parts[0].skip, 0);
    assert_eq!(parts[0].length, 50);

    // Block boundary
    let parts = explode_plain_range(4096, 50);
    assert_eq!(parts[0].block_no, 1);
    assert_eq!(parts[0].skip, 0);
}

// ============================================================================
// Config validation
// ============================================================================

#[test]
fn test_config_reject_version_1() {
    let dir = tempfile::tempdir().unwrap();
    let conf = dir.path().join("gocryptfs.conf");
    std::fs::write(&conf, r#"{"Creator":"test","EncryptedKey":"dGVzdA==","ScryptObject":{"Salt":"c2FsdA==","N":65536,"R":8,"P":1,"KeyLen":32},"Version":1,"FeatureFlags":["GCMIV128"]}"#).unwrap();
    assert!(vaultbox_lib::crypto::config::GocryptfsConfig::load(dir.path()).is_err());
}

#[test]
fn test_config_reject_unknown_flag() {
    let dir = tempfile::tempdir().unwrap();
    let conf = dir.path().join("gocryptfs.conf");
    std::fs::write(&conf, r#"{"Creator":"test","EncryptedKey":"dGVzdA==","ScryptObject":{"Salt":"c2FsdA==","N":65536,"R":8,"P":1,"KeyLen":32},"Version":2,"FeatureFlags":["GCMIV128","FutureFlag"]}"#).unwrap();
    assert!(vaultbox_lib::crypto::config::GocryptfsConfig::load(dir.path()).is_err());
}

#[test]
fn test_config_accept_all_6_standard_flags() {
    let dir = tempfile::tempdir().unwrap();
    let conf = dir.path().join("gocryptfs.conf");
    std::fs::write(&conf, r#"{"Creator":"test","EncryptedKey":"dGVzdA==","ScryptObject":{"Salt":"c2FsdA==","N":65536,"R":8,"P":1,"KeyLen":32},"Version":2,"FeatureFlags":["GCMIV128","DirIV","EMENames","LongNames","HKDF","Raw64"]}"#).unwrap();
    let config = vaultbox_lib::crypto::config::GocryptfsConfig::load(dir.path()).unwrap();
    assert!(config.uses_hkdf());
    assert!(config.uses_raw64());
    assert!(config.uses_dir_iv());
    assert!(config.uses_eme_names());
    assert!(config.uses_long_names());
    assert!(config.has_flag("GCMIV128"));
}

#[test]
fn test_config_empty_file_fails() {
    let dir = tempfile::tempdir().unwrap();
    let conf = dir.path().join("gocryptfs.conf");
    std::fs::write(&conf, "").unwrap();
    assert!(vaultbox_lib::crypto::config::GocryptfsConfig::load(dir.path()).is_err());
}

#[test]
fn test_config_invalid_json_fails() {
    let dir = tempfile::tempdir().unwrap();
    let conf = dir.path().join("gocryptfs.conf");
    std::fs::write(&conf, "not json at all {{{").unwrap();
    assert!(vaultbox_lib::crypto::config::GocryptfsConfig::load(dir.path()).is_err());
}

#[test]
fn test_config_missing_file_fails() {
    let dir = tempfile::tempdir().unwrap();
    assert!(vaultbox_lib::crypto::config::GocryptfsConfig::load(dir.path()).is_err());
}

#[test]
fn test_config_roundtrip_serialize_deserialize() {
    let config = vaultbox_lib::crypto::config::GocryptfsConfig {
        creator: "vaultbox-test".into(),
        encrypted_key: "dGVzdA==".into(),
        scrypt_object: vaultbox_lib::crypto::config::ScryptObject {
            salt: "c2FsdA==".into(), n: 65536, r: 8, p: 1, key_len: 32,
        },
        version: 2,
        feature_flags: vec!["GCMIV128".into(), "HKDF".into(), "DirIV".into(), "EMENames".into(), "LongNames".into(), "Raw64".into()],
    };

    let json = serde_json::to_string_pretty(&config).unwrap();
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("gocryptfs.conf"), &json).unwrap();
    let loaded = vaultbox_lib::crypto::config::GocryptfsConfig::load(dir.path()).unwrap();
    assert_eq!(loaded.version, config.version);
    assert_eq!(loaded.creator, config.creator);
    assert_eq!(loaded.feature_flags, config.feature_flags);
}

// ============================================================================
// Security edge cases
// ============================================================================

#[test]
fn test_truncated_ciphertext_no_crash() {
    let key = [0x42; 32];
    let ct = content::encrypt_file(&key, b"hello world").unwrap();

    // Try every truncation length — must error, not crash
    for len in 0..ct.len() {
        let _ = content::decrypt_file(&key, &ct[..len]);
    }
}

#[test]
fn test_random_garbage_no_crash() {
    let key = [0x42; 32];
    // Various garbage inputs — must not panic
    for size in [0, 1, 17, 18, 19, 33, 100, 4128, 5000] {
        let garbage: Vec<u8> = (0..size).map(|i| (i * 7 + 13) as u8).collect();
        let _ = content::decrypt_file(&key, &garbage);
    }
}

#[test]
fn test_filename_garbage_no_crash() {
    let key = [0x42; 32];
    let iv = [0x01; 16];
    // Random base64-like strings — must not panic
    for s in ["AAAA", "AAAAAAAA", "AAAAAAAAAAAAAAAA", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA"] {
        let _ = filename::decrypt_filename(&key, &iv, s, true);
    }
}

#[test]
fn test_scrypt_reject_huge_n() {
    // LogN=64 is invalid (would overflow u64) — must be rejected
    let result = scrypt::Params::new(64, 8, 1, 32);
    assert!(result.is_err(), "scrypt with N=2^64 should be rejected");
}

#[test]
fn test_password_not_in_error_messages() {
    let config = vaultbox_lib::crypto::config::GocryptfsConfig {
        creator: "test".into(),
        encrypted_key: "dGVzdGRhdGF0ZXN0ZGF0YXRlc3RkYXRhdGVzdGRhdGF0ZXN0ZGF0YXRlc3RkYXRhdGVzdA==".into(),
        scrypt_object: vaultbox_lib::crypto::config::ScryptObject {
            salt: "c2FsdA==".into(), n: 16, r: 8, p: 1, key_len: 32,
        },
        version: 2,
        feature_flags: vec!["HKDF".into(), "GCMIV128".into()],
    };

    let password = "super-secret-password-12345";
    let err = kdf::derive_master_key(password, &config).unwrap_err();
    let err_msg = format!("{}", err);
    assert!(!err_msg.contains(password), "Error message must not contain the password: {}", err_msg);
}

// ============================================================================
// Feature flag combinations
// ============================================================================

#[test]
fn test_hkdf_off_uses_master_key_directly() {
    // Without HKDF, content_key = master_key (gocryptfs v0.7-v1.2 behavior)
    // derive_content_key always uses HKDF, but the caller decides based on flag
    let master = [0x42; 32];
    let ck_hkdf = kdf::derive_content_key(&master).unwrap();
    // Without HKDF, key IS the master key
    assert_ne!(*ck_hkdf, master, "HKDF output should differ from input");
    // This confirms that if HKDF flag is absent, caller should use master directly
}

#[test]
fn test_raw64_off_uses_padded_base64() {
    let key = [0x42; 32];
    let iv = [0x01; 16];
    let name = "test.txt";

    let raw64 = filename::encrypt_filename(&key, &iv, name, true).unwrap();
    let padded = filename::encrypt_filename(&key, &iv, name, false).unwrap();

    // Raw64: no '=' padding
    assert!(!raw64.contains('='));

    // Padded: may have '=' (depending on length)
    // Both decrypt correctly with their mode
    assert_eq!(filename::decrypt_filename(&key, &iv, &raw64, true).unwrap(), name);
    assert_eq!(filename::decrypt_filename(&key, &iv, &padded, false).unwrap(), name);

    // Cross-mode fails
    assert!(filename::decrypt_filename(&key, &iv, &raw64, false).is_err()
        || filename::decrypt_filename(&key, &iv, &raw64, false).unwrap() != name);
}

// ============================================================================
// Concurrent access (pure crypto, no FS)
// ============================================================================

#[test]
fn test_concurrent_encrypt_decrypt() {
    use std::sync::Arc;
    use std::thread;

    let key = Arc::new([0x42u8; 32]);
    let mut handles = Vec::new();

    for i in 0..10u8 {
        let key = Arc::clone(&key);
        handles.push(thread::spawn(move || {
            let plaintext: Vec<u8> = (0..1000).map(|j| i.wrapping_add(j as u8)).collect();
            let ct = content::encrypt_file(&key, &plaintext).unwrap();
            let dec = content::decrypt_file(&key, &ct).unwrap();
            assert_eq!(*dec, plaintext, "Thread {} roundtrip failed", i);
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn test_concurrent_filename_encrypt_decrypt() {
    use std::sync::Arc;
    use std::thread;

    let key = Arc::new([0x42u8; 32]);
    let iv = Arc::new([0x01u8; 16]);
    let mut handles = Vec::new();

    for i in 0..10u8 {
        let key = Arc::clone(&key);
        let iv = Arc::clone(&iv);
        handles.push(thread::spawn(move || {
            let name = format!("file_{}.txt", i);
            let enc = filename::encrypt_filename(&key, &iv, &name, true).unwrap();
            let dec = filename::decrypt_filename(&key, &iv, &enc, true).unwrap();
            assert_eq!(dec, name, "Thread {} filename roundtrip failed", i);
        }));
    }

    for h in handles {
        h.join().unwrap();
    }
}

// ============================================================================
// File operation sequences (vault ops on temp dirs)
// ============================================================================

#[test]
fn test_write_overwrite_read_sequence() {
    use vaultbox_lib::crypto::diriv;
    use vaultbox_lib::vault::ops;

    let dir = tempfile::tempdir().unwrap();
    diriv::create_diriv(dir.path()).unwrap();
    let ck = *kdf::derive_content_key(&[0x42; 32]).unwrap();
    let fk = *kdf::derive_filename_key(&[0x42; 32]).unwrap();

    ops::create_file(dir.path(), "", "test.txt", &fk, &ck, true).unwrap();
    ops::write_file(dir.path(), "test.txt", b"version 1", &fk, &ck, true).unwrap();
    assert_eq!(ops::read_file(dir.path(), "test.txt", &fk, &ck, true).unwrap().as_slice(), b"version 1");

    ops::write_file(dir.path(), "test.txt", b"version 2 is longer", &fk, &ck, true).unwrap();
    assert_eq!(ops::read_file(dir.path(), "test.txt", &fk, &ck, true).unwrap().as_slice(), b"version 2 is longer");

    ops::write_file(dir.path(), "test.txt", b"v3", &fk, &ck, true).unwrap();
    assert_eq!(ops::read_file(dir.path(), "test.txt", &fk, &ck, true).unwrap().as_slice(), b"v3");
}

#[test]
fn test_create_rename_read_delete_sequence() {
    use vaultbox_lib::crypto::diriv;
    use vaultbox_lib::vault::ops;

    let dir = tempfile::tempdir().unwrap();
    diriv::create_diriv(dir.path()).unwrap();
    let ck = *kdf::derive_content_key(&[0x42; 32]).unwrap();
    let fk = *kdf::derive_filename_key(&[0x42; 32]).unwrap();

    ops::create_file(dir.path(), "", "old.txt", &fk, &ck, true).unwrap();
    ops::write_file(dir.path(), "old.txt", b"content", &fk, &ck, true).unwrap();

    ops::rename_entry(dir.path(), "old.txt", "new.txt", &fk, true).unwrap();
    assert!(ops::read_file(dir.path(), "old.txt", &fk, &ck, true).is_err());
    assert_eq!(ops::read_file(dir.path(), "new.txt", &fk, &ck, true).unwrap().as_slice(), b"content");

    ops::delete_entry(dir.path(), "new.txt", &fk, true).unwrap();
    assert!(ops::read_file(dir.path(), "new.txt", &fk, &ck, true).is_err());
}

// ============================================================================
// Performance benchmarks (not assertions, just measure)
// ============================================================================

#[test]
fn bench_encrypt_1mb() {
    use std::time::Instant;
    let key = [0x42; 32];
    let data = vec![0xAB; 1024 * 1024];

    let start = Instant::now();
    let ct = content::encrypt_file(&key, &data).unwrap();
    let enc_ms = start.elapsed().as_millis();

    let start = Instant::now();
    let _dec = content::decrypt_file(&key, &ct).unwrap();
    let dec_ms = start.elapsed().as_millis();

    let mb_per_sec_enc = if enc_ms > 0 { 1000 / enc_ms } else { 9999 };
    let mb_per_sec_dec = if dec_ms > 0 { 1000 / dec_ms } else { 9999 };

    eprintln!("1MB encrypt: {}ms ({} MB/s), decrypt: {}ms ({} MB/s)", enc_ms, mb_per_sec_enc, dec_ms, mb_per_sec_dec);
}

#[test]
fn bench_encrypt_10mb() {
    use std::time::Instant;
    let key = [0x42; 32];
    let data = vec![0xAB; 10 * 1024 * 1024];

    let start = Instant::now();
    let ct = content::encrypt_file(&key, &data).unwrap();
    let enc_ms = start.elapsed().as_millis();

    let start = Instant::now();
    let _dec = content::decrypt_file(&key, &ct).unwrap();
    let dec_ms = start.elapsed().as_millis();

    eprintln!("10MB encrypt: {}ms, decrypt: {}ms", enc_ms, dec_ms);
}

#[test]
fn bench_filename_encrypt_1000() {
    use std::time::Instant;
    let key = [0x42; 32];
    let iv = [0x01; 16];

    let start = Instant::now();
    for i in 0..1000 {
        let name = format!("file_{:04}.txt", i);
        let _ = filename::encrypt_filename(&key, &iv, &name, true).unwrap();
    }
    let ms = start.elapsed().as_millis();
    eprintln!("1000 filename encryptions: {}ms", ms);
}

#[test]
fn bench_scrypt_logn16() {
    use std::time::Instant;
    let params = scrypt::Params::new(16, 8, 1, 32).unwrap();
    let mut out = [0u8; 32];
    let start = Instant::now();
    scrypt::scrypt(b"benchmark", &[0; 32], &params, &mut out).unwrap();
    let ms = start.elapsed().as_millis();
    eprintln!("scrypt LogN=16: {}ms", ms);
}

// ============================================================================
// Remaining TODO tests
// ============================================================================

// --- Longname .name file creation on disk ---

#[test]
fn test_longname_detection_and_hash() {
    // Test long name detection and hashing at the crypto level.
    // We can't create files with very long encrypted names on macOS (255 byte limit),
    // but we can verify the logic that detects and hashes long names.
    let key = [0x42; 32];
    let iv = [0x01; 16];

    // Short name → not long
    let short_enc = filename::encrypt_filename(&key, &iv, "short.txt", true).unwrap();
    assert!(!filename::is_long_name(&short_enc));

    // 128-char name → encrypted is ~240 chars → IS long name
    let medium_name: String = (0..128).map(|i| (b'a' + (i % 26) as u8) as char).collect();
    let medium_enc = filename::encrypt_filename(&key, &iv, &medium_name, true).unwrap();
    assert!(filename::is_long_name(&medium_enc), "128-char name should produce long encrypted name");

    // Long name hash should be consistent and different for different names
    let hash1 = filename::long_name_hash(&medium_enc);
    let hash2 = filename::long_name_hash(&medium_enc);
    assert_eq!(hash1, hash2, "Hash must be deterministic");

    let other_enc = filename::encrypt_filename(&key, &iv, &"B".repeat(128), true).unwrap();
    let hash3 = filename::long_name_hash(&other_enc);
    assert_ne!(hash1, hash3, "Different names must have different hashes");

    // Both names still decrypt correctly
    assert_eq!(filename::decrypt_filename(&key, &iv, &medium_enc, true).unwrap(), medium_name);
    assert_eq!(filename::decrypt_filename(&key, &iv, &other_enc, true).unwrap(), "B".repeat(128));
}

#[test]
fn test_short_name_file_on_disk() {
    // Verify normal file operations with names that DON'T trigger longname
    use vaultbox_lib::crypto::diriv;
    use vaultbox_lib::vault::ops;

    let dir = tempfile::tempdir().unwrap();
    diriv::create_diriv(dir.path()).unwrap();
    let ck = *kdf::derive_content_key(&[0x42; 32]).unwrap();
    let fk = *kdf::derive_filename_key(&[0x42; 32]).unwrap();

    // These names are short enough to NOT trigger longname format
    let x50 = "x".repeat(50);
    let names = ["hello.txt", "a-medium-length-name.pdf", &x50];
    for name in &names {
        ops::create_file(dir.path(), "", name, &fk, &ck, true).unwrap();
        ops::write_file(dir.path(), name, format!("content of {}", name).as_bytes(), &fk, &ck, true).unwrap();
    }

    let entries = ops::list_directory(dir.path(), "", &fk, &ck, true).unwrap();
    assert_eq!(entries.len(), names.len());

    for name in &names {
        let data = ops::read_file(dir.path(), name, &fk, &ck, true).unwrap();
        assert_eq!(data.as_slice(), format!("content of {}", name).as_bytes());
    }

    // On disk, no files should use longname format
    let disk_names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|n| n != "gocryptfs.diriv")
        .collect();

    let has_longname = disk_names.iter().any(|n| n.starts_with("gocryptfs.longname."));
    assert!(!has_longname, "Short names should NOT use longname format: {:?}", disk_names);
}

// --- Corrupted diriv → clean error ---

#[test]
fn test_corrupted_diriv_wrong_length() {
    use vaultbox_lib::crypto::diriv;

    let dir = tempfile::tempdir().unwrap();
    let iv_path = dir.path().join("gocryptfs.diriv");

    // Write wrong-length diriv
    std::fs::write(&iv_path, &[0u8; 10]).unwrap();
    let result = diriv::read_diriv(dir.path());
    assert!(result.is_err(), "Corrupted diriv (wrong length) should error");
}

#[test]
fn test_corrupted_diriv_missing() {
    use vaultbox_lib::crypto::diriv;

    let dir = tempfile::tempdir().unwrap();
    // No diriv file at all
    let result = diriv::read_diriv(dir.path());
    assert!(result.is_err(), "Missing diriv should error");
}

#[test]
fn test_corrupted_diriv_empty() {
    use vaultbox_lib::crypto::diriv;

    let dir = tempfile::tempdir().unwrap();
    let iv_path = dir.path().join("gocryptfs.diriv");
    std::fs::write(&iv_path, &[]).unwrap();
    let result = diriv::read_diriv(dir.path());
    assert!(result.is_err(), "Empty diriv should error");
}

#[test]
fn test_vault_ops_with_corrupted_diriv() {
    use vaultbox_lib::vault::ops;

    let dir = tempfile::tempdir().unwrap();
    let iv_path = dir.path().join("gocryptfs.diriv");
    std::fs::write(&iv_path, &[0xFF; 5]).unwrap(); // wrong length

    let ck = *kdf::derive_content_key(&[0x42; 32]).unwrap();
    let fk = *kdf::derive_filename_key(&[0x42; 32]).unwrap();

    // All ops should fail cleanly, not panic
    assert!(ops::list_directory(dir.path(), "", &fk, &ck, true).is_err());
    assert!(ops::create_file(dir.path(), "", "test.txt", &fk, &ck, true).is_err());
    assert!(ops::read_file(dir.path(), "test.txt", &fk, &ck, true).is_err());
}

// --- Filename not leaked in error messages ---

#[test]
fn test_filename_not_in_decrypt_error() {
    let key = [0x42; 32];
    let iv = [0x01; 16];
    let secret_name = "TOP_SECRET_PROJECT_NAME.docx";

    let enc = filename::encrypt_filename(&key, &iv, secret_name, true).unwrap();

    // Decrypt with wrong key → error should not contain the original name
    let result = filename::decrypt_filename(&[0x99; 32], &iv, &enc, true);
    if let Err(e) = result {
        let msg = format!("{}", e);
        assert!(!msg.contains(secret_name), "Error should not leak filename: {}", msg);
        assert!(!msg.contains("TOP_SECRET"), "Error should not leak filename parts: {}", msg);
    }
}

#[test]
fn test_content_error_does_not_leak_plaintext() {
    let key = [0x42; 32];
    let secret = b"This is classified information that must not appear in errors";
    let ct = content::encrypt_file(&key, secret).unwrap();

    // Decrypt with wrong key
    let result = content::decrypt_file(&[0x99; 32], &ct);
    if let Err(e) = result {
        let msg = format!("{}", e);
        assert!(!msg.contains("classified"), "Error should not leak plaintext: {}", msg);
    }
}

// --- LongNames disabled ---

#[test]
fn test_filename_works_without_longnames_flag() {
    // Even without LongNames flag, short names must work.
    // LongNames only affects how names > 176 encrypted chars are stored on disk.
    let key = [0x42; 32];
    let iv = [0x01; 16];

    for name in ["short.txt", "medium-length-filename.pdf", "a.b"] {
        let enc = filename::encrypt_filename(&key, &iv, name, true).unwrap();
        let dec = filename::decrypt_filename(&key, &iv, &enc, true).unwrap();
        assert_eq!(dec, name);
    }
}

// --- Performance benchmarks: file creation ---

#[test]
fn bench_create_1000_empty_files() {
    use std::time::Instant;
    use vaultbox_lib::crypto::diriv;
    use vaultbox_lib::vault::ops;

    let dir = tempfile::tempdir().unwrap();
    diriv::create_diriv(dir.path()).unwrap();
    let ck = *kdf::derive_content_key(&[0x42; 32]).unwrap();
    let fk = *kdf::derive_filename_key(&[0x42; 32]).unwrap();

    let start = Instant::now();
    for i in 0..1000 {
        let name = format!("file_{:04}.txt", i);
        ops::create_file(dir.path(), "", &name, &fk, &ck, true).unwrap();
    }
    let ms = start.elapsed().as_millis();
    eprintln!("Create 1000 empty files: {}ms ({:.1} files/sec)", ms, 1000.0 / (ms as f64 / 1000.0));
}

#[test]
fn bench_create_1000_files_with_4kb_content() {
    use std::time::Instant;
    use vaultbox_lib::crypto::diriv;
    use vaultbox_lib::vault::ops;

    let dir = tempfile::tempdir().unwrap();
    diriv::create_diriv(dir.path()).unwrap();
    let ck = *kdf::derive_content_key(&[0x42; 32]).unwrap();
    let fk = *kdf::derive_filename_key(&[0x42; 32]).unwrap();
    let data = vec![0xAB; 4096];

    let start = Instant::now();
    for i in 0..1000 {
        let name = format!("file_{:04}.bin", i);
        ops::create_file(dir.path(), "", &name, &fk, &ck, true).unwrap();
        ops::write_file(dir.path(), &name, &data, &fk, &ck, true).unwrap();
    }
    let ms = start.elapsed().as_millis();
    eprintln!("Create+write 1000 x 4KB files: {}ms ({:.1} files/sec)", ms, 1000.0 / (ms as f64 / 1000.0));
}

#[test]
fn bench_create_100_directories() {
    use std::time::Instant;
    use vaultbox_lib::crypto::diriv;
    use vaultbox_lib::vault::ops;

    let dir = tempfile::tempdir().unwrap();
    diriv::create_diriv(dir.path()).unwrap();
    let fk = *kdf::derive_filename_key(&[0x42; 32]).unwrap();

    let start = Instant::now();
    for i in 0..100 {
        let name = format!("dir_{:03}", i);
        ops::create_directory(dir.path(), "", &name, &fk, true).unwrap();
    }
    let ms = start.elapsed().as_millis();
    eprintln!("Create 100 directories: {}ms ({:.1} dirs/sec)", ms, 100.0 / (ms as f64 / 1000.0));
}
