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

// Need hex encoding
mod hex {
    pub fn encode(data: &[u8]) -> String {
        data.iter().map(|b| format!("{:02x}", b)).collect()
    }
}
