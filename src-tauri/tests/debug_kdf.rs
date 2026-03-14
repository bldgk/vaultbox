//! Debug test: compare Rust KDF output with known Go values for the cipher vault.

#[test]
fn test_real_vault_kdf() {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;
    use hkdf::Hkdf;
    use sha2::Sha256;

    let password = b"123456123";
    let salt_b64 = "IajRu/RqU/v8LG7xDc8ARD/pUo+x4XeDESy2JcmH3Xs=";
    let enc_key_b64 = "vPlK2K0mTbm5uTQ3iA1ZV2sjOADUaChwiSqo8YreWaLC935IfJepbAWvr2kZvD89ce2FN0y374zxIPo92PVWaw==";

    let salt = STANDARD.decode(salt_b64).unwrap();
    let enc_key = STANDARD.decode(enc_key_b64).unwrap();

    // Expected values from Go
    let expected_scrypt = "64909bd463eb95310f78ee5a80d3301f7a8b33faf97828f501a06e54ac2a1403";
    let expected_gcm_key = "e28a2725f3b73d38b6b05784783e42685b288270d3e721e769c3d0308bf959a5";
    let expected_master = "b0cb316549ae0f38192e9a978e1dea41bee1ca5e6ae066b26cbf145404be994c";

    // Step 1: scrypt
    let scrypt_params = scrypt::Params::new(16, 8, 1, 32).unwrap();
    let mut scrypt_key = [0u8; 32];
    scrypt::scrypt(password, &salt, &scrypt_params, &mut scrypt_key).unwrap();
    let scrypt_hex = hex::encode(&scrypt_key);
    eprintln!("Rust scryptKey: {}", scrypt_hex);
    assert_eq!(scrypt_hex, expected_scrypt, "scrypt key mismatch");

    // Step 2: HKDF
    let hk = Hkdf::<Sha256>::new(None, &scrypt_key);
    let mut gcm_key = [0u8; 32];
    hk.expand(b"AES-GCM file content encryption", &mut gcm_key).unwrap();
    let gcm_hex = hex::encode(&gcm_key);
    eprintln!("Rust gcmKey:    {}", gcm_hex);
    assert_eq!(gcm_hex, expected_gcm_key, "HKDF-derived GCM key mismatch");

    // Step 3: Split
    let nonce = &enc_key[..16];
    let ciphertext = &enc_key[16..];
    eprintln!("nonce: {}", hex::encode(nonce));
    eprintln!("ciphertext: {}", hex::encode(ciphertext));

    // Step 4: AES-GCM decrypt with 16-byte nonce
    use aes::Aes256;
    use aes_gcm::{aead::{Aead, KeyInit}, AesGcm, Nonce, aead::Payload, aead::consts::U16};
    type Aes256Gcm16 = AesGcm<Aes256, U16>;

    let cipher = Aes256Gcm16::new_from_slice(&gcm_key).unwrap();
    let aad = [0u8; 8]; // blockNo=0
    let payload = Payload { msg: ciphertext, aad: &aad };
    let master_key = cipher.decrypt(Nonce::from_slice(nonce), payload).unwrap();
    let master_hex = hex::encode(&master_key);
    eprintln!("Rust masterKey: {}", master_hex);
    assert_eq!(master_hex, expected_master, "master key mismatch");

    eprintln!("\nAll steps match Go output!");
}

// ---- Pure HKDF vector tests (no scrypt needed) ----

/// Validate HKDF-SHA256 known vectors directly, without any scrypt derivation.
/// These are the same vectors used in gocryptfs to verify HKDF output.
#[test]
fn test_hkdf_known_vectors_pure() {
    use hkdf::Hkdf;
    use sha2::Sha256;

    struct HkdfVector {
        master: [u8; 32],
        info: &'static [u8],
        expected_hex: &'static str,
    }

    let vectors = [
        HkdfVector {
            master: [0x00; 32],
            info: b"EME filename encryption",
            expected_hex: "9ba3cddd48c6339c6e56ebe85f0281d6e9051be4104176e65cb0f8a6f77ae6b4",
        },
        HkdfVector {
            master: [0x01; 32],
            info: b"EME filename encryption",
            expected_hex: "e8a2499f48700b954f31de732efd04abce822f5c948e7fbc0896607be0d36d12",
        },
        HkdfVector {
            master: [0x01; 32],
            info: b"AES-GCM file content encryption",
            expected_hex: "9137f2e67a842484137f3c458f357f204c30d7458f94f432fa989be96854a649",
        },
        HkdfVector {
            master: [0x00; 32],
            info: b"AES-GCM file content encryption",
            expected_hex: "ab893c7a9f8683d8c56296fdd109aa8d73c1937d112d1cf89923f9d175fbd782",
        },
    ];

    for (i, v) in vectors.iter().enumerate() {
        let hk = Hkdf::<Sha256>::new(None, &v.master);
        let mut derived = [0u8; 32];
        hk.expand(v.info, &mut derived).unwrap();
        let got = hex::encode(&derived);
        assert_eq!(
            got, v.expected_hex,
            "HKDF vector {} failed: master=0x{:02x}*32, info={:?}",
            i, v.master[0], std::str::from_utf8(v.info).unwrap()
        );
    }
}

/// Verify that derive_content_key and derive_filename_key from kdf.rs
/// match the pure HKDF computation.
#[test]
fn test_kdf_functions_match_pure_hkdf() {
    use hkdf::Hkdf;
    use sha2::Sha256;
    use vaultbox_lib::crypto::kdf;

    for master_byte in [0x00u8, 0x01, 0x42, 0xFF] {
        let master = [master_byte; 32];

        // Content key via kdf function
        let ck = kdf::derive_content_key(&master).unwrap();
        // Content key via raw HKDF
        let hk = Hkdf::<Sha256>::new(None, &master);
        let mut expected_ck = [0u8; 32];
        hk.expand(b"AES-GCM file content encryption", &mut expected_ck).unwrap();
        assert_eq!(
            ck.as_ref(), &expected_ck,
            "derive_content_key mismatch for master=0x{:02x}*32", master_byte
        );

        // Filename key via kdf function
        let fk = kdf::derive_filename_key(&master).unwrap();
        let mut expected_fk = [0u8; 32];
        hk.expand(b"EME filename encryption", &mut expected_fk).unwrap();
        assert_eq!(
            fk.as_ref(), &expected_fk,
            "derive_filename_key mismatch for master=0x{:02x}*32", master_byte
        );
    }
}

// Need hex encoding
mod hex {
    pub fn encode(data: &[u8]) -> String {
        data.iter().map(|b| format!("{:02x}", b)).collect()
    }
}
