use aes::Aes256;
use aes_gcm::{
    aead::{Aead, KeyInit},
    AesGcm, Nonce,
};
use aes_gcm::aead::consts::U16;
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

use super::config::GocryptfsConfig;
use thiserror::Error;

/// gocryptfs uses 16-byte (128-bit) nonces for GCM when GCMIV128 flag is set.
/// This is non-standard (AES-GCM default is 12 bytes) but Go's crypto library
/// supports it via `cipher.NewGCMWithNonceSize(block, 16)`.
type Aes256Gcm16 = AesGcm<Aes256, U16>;

#[derive(Debug, Error)]
pub enum KdfError {
    #[error("scrypt key derivation failed: {0}")]
    Scrypt(String),
    #[error("base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),
    #[error("Wrong password or corrupted config")]
    DecryptionFailed,
    #[error("Invalid encrypted key length")]
    InvalidKeyLength,
    #[error("HKDF expand failed")]
    HkdfError,
}

/// Derives the master key from a password and gocryptfs config.
pub fn derive_master_key(
    password: &str,
    config: &GocryptfsConfig,
) -> Result<Zeroizing<[u8; 32]>, KdfError> {
    use base64::engine::general_purpose::STANDARD;
    use base64::Engine;

    let salt = STANDARD
        .decode(&config.scrypt_object.salt)?;

    // Derive scrypt key from password
    let scrypt_params = scrypt::Params::new(
        log2(config.scrypt_object.n),
        config.scrypt_object.r,
        config.scrypt_object.p,
        32,
    )
    .map_err(|e| KdfError::Scrypt(e.to_string()))?;

    let mut scrypt_key = Zeroizing::new([0u8; 32]);
    scrypt::scrypt(
        password.as_bytes(),
        &salt,
        &scrypt_params,
        scrypt_key.as_mut(),
    )
    .map_err(|e| KdfError::Scrypt(e.to_string()))?;

    // When HKDF is enabled, gocryptfs derives a GCM key from the scrypt hash via HKDF-SHA256
    // with info string "AES-GCM file content encryption". This derived key is used for
    // master key wrapping. (A separate HKDF derivation from the *master* key produces the
    // actual file content encryption key — see derive_content_key.)
    let gcm_key = if config.uses_hkdf() {
        let hk = Hkdf::<Sha256>::new(None, scrypt_key.as_ref());
        let mut derived = Zeroizing::new([0u8; 32]);
        hk.expand(b"AES-GCM file content encryption", derived.as_mut())
            .map_err(|_| KdfError::HkdfError)?;
        derived
    } else {
        scrypt_key.clone()
    };

    // Decrypt the master key
    let encrypted_key = STANDARD
        .decode(&config.encrypted_key)?;

    // gocryptfs format: nonce (16 bytes) + ciphertext (32 bytes) + GCM tag (16 bytes) = 64 bytes
    // With HKDF: 128-bit (16-byte) nonces. Without: 96-bit (12-byte) nonces.
    let iv_len = if config.uses_hkdf() { 16 } else { 12 };
    if encrypted_key.len() < iv_len + 16 {
        return Err(KdfError::InvalidKeyLength);
    }

    let nonce = &encrypted_key[..iv_len];
    let ciphertext = &encrypted_key[iv_len..];

    // Use the appropriate GCM variant based on nonce size.
    // AAD = blockNo(0) as big-endian u64 = 8 zero bytes (same as gocryptfs DecryptBlock).
    use aes_gcm::aead::Payload;
    let aad = [0u8; 8];
    let payload = Payload { msg: ciphertext, aad: &aad };

    let master_key_vec = if config.uses_hkdf() {
        let cipher = Aes256Gcm16::new_from_slice(gcm_key.as_ref())
            .map_err(|_| KdfError::InvalidKeyLength)?;
        Zeroizing::new(
            cipher
                .decrypt(Nonce::from_slice(nonce), payload)
                .map_err(|_| KdfError::DecryptionFailed)?,
        )
    } else {
        use aes_gcm::Aes256Gcm;
        let cipher = Aes256Gcm::new_from_slice(gcm_key.as_ref())
            .map_err(|_| KdfError::InvalidKeyLength)?;
        Zeroizing::new(
            cipher
                .decrypt(Nonce::from_slice(nonce), payload)
                .map_err(|_| KdfError::DecryptionFailed)?,
        )
    };

    if master_key_vec.len() != 32 {
        return Err(KdfError::InvalidKeyLength);
    }

    let mut master_key = Zeroizing::new([0u8; 32]);
    master_key.copy_from_slice(&master_key_vec);

    Ok(master_key)
}

/// Derive content encryption key from master key using HKDF.
pub fn derive_content_key(master_key: &[u8; 32]) -> Result<Zeroizing<[u8; 32]>, KdfError> {
    let hk = Hkdf::<Sha256>::new(None, master_key);
    let mut content_key = Zeroizing::new([0u8; 32]);
    hk.expand(b"AES-GCM file content encryption", content_key.as_mut())
        .map_err(|_| KdfError::HkdfError)?;
    Ok(content_key)
}

/// Derive filename encryption key from master key using HKDF.
pub fn derive_filename_key(master_key: &[u8; 32]) -> Result<Zeroizing<[u8; 32]>, KdfError> {
    let hk = Hkdf::<Sha256>::new(None, master_key);
    let mut filename_key = Zeroizing::new([0u8; 32]);
    hk.expand(b"EME filename encryption", filename_key.as_mut())
        .map_err(|_| KdfError::HkdfError)?;
    Ok(filename_key)
}

/// Compute log2 of n (for scrypt N parameter).
fn log2(n: u64) -> u8 {
    assert!(n.is_power_of_two(), "scrypt N must be a power of 2");
    n.trailing_zeros() as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log2() {
        assert_eq!(log2(1), 0);
        assert_eq!(log2(2), 1);
        assert_eq!(log2(1024), 10);
        assert_eq!(log2(65536), 16);
    }

    #[test]
    fn test_log2_large() {
        assert_eq!(log2(1 << 20), 20);
        assert_eq!(log2(1 << 30), 30);
    }

    #[test]
    #[should_panic(expected = "power of 2")]
    fn test_log2_non_power_of_two() {
        log2(3);
    }

    #[test]
    #[should_panic(expected = "power of 2")]
    fn test_log2_zero() {
        log2(0);
    }

    #[test]
    fn test_derive_content_key_deterministic() {
        let master = [0x42u8; 32];
        let k1 = derive_content_key(&master).unwrap();
        let k2 = derive_content_key(&master).unwrap();
        assert_eq!(*k1, *k2);
    }

    #[test]
    fn test_derive_filename_key_deterministic() {
        let master = [0x42u8; 32];
        let k1 = derive_filename_key(&master).unwrap();
        let k2 = derive_filename_key(&master).unwrap();
        assert_eq!(*k1, *k2);
    }

    #[test]
    fn test_content_and_filename_keys_differ() {
        let master = [0x42u8; 32];
        let ck = derive_content_key(&master).unwrap();
        let fk = derive_filename_key(&master).unwrap();
        assert_ne!(*ck, *fk);
    }

    #[test]
    fn test_different_master_keys_yield_different_content_keys() {
        let m1 = [0x01u8; 32];
        let m2 = [0x02u8; 32];
        let ck1 = derive_content_key(&m1).unwrap();
        let ck2 = derive_content_key(&m2).unwrap();
        assert_ne!(*ck1, *ck2);
    }

    #[test]
    fn test_different_master_keys_yield_different_filename_keys() {
        let m1 = [0x01u8; 32];
        let m2 = [0x02u8; 32];
        let fk1 = derive_filename_key(&m1).unwrap();
        let fk2 = derive_filename_key(&m2).unwrap();
        assert_ne!(*fk1, *fk2);
    }

    #[test]
    fn test_derive_master_key_wrong_password() {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;

        // Build a minimal valid config with known values
        let salt = [0xAA; 32];
        let password = "correct-password";

        // Derive a wrapping key with small scrypt params
        let scrypt_params = scrypt::Params::new(4, 8, 1, 32).unwrap(); // N=16 for speed
        let mut wrapping_key = [0u8; 32];
        scrypt::scrypt(password.as_bytes(), &salt, &scrypt_params, &mut wrapping_key).unwrap();

        // When HKDF flag is set, derive GCM key from scrypt hash via HKDF
        let hk = Hkdf::<Sha256>::new(None, &wrapping_key);
        let mut gcm_key = [0u8; 32];
        hk.expand(b"AES-GCM file content encryption", &mut gcm_key).unwrap();

        // Encrypt a fake master key with 16-byte nonce (GCMIV128) + AAD
        use aes_gcm::aead::Payload;
        let cipher = Aes256Gcm16::new_from_slice(&gcm_key).unwrap();
        let nonce_bytes = [0u8; 16];
        let nonce = Nonce::from_slice(&nonce_bytes);
        let fake_master = [0xBB; 32];
        let aad = [0u8; 8]; // blockNo=0
        let payload = Payload { msg: fake_master.as_ref(), aad: &aad };
        let encrypted_master = cipher.encrypt(nonce, payload).unwrap();

        let mut encrypted_key_full = Vec::new();
        encrypted_key_full.extend_from_slice(&nonce_bytes);
        encrypted_key_full.extend_from_slice(&encrypted_master);

        let config = crate::crypto::config::GocryptfsConfig {
            creator: "test".into(),
            encrypted_key: STANDARD.encode(&encrypted_key_full),
            scrypt_object: crate::crypto::config::ScryptObject {
                salt: STANDARD.encode(&salt),
                n: 16,
                r: 8,
                p: 1,
                key_len: 32,
            },
            version: 2,
            feature_flags: vec!["GCMIV128".into(), "HKDF".into(), "DirIV".into(), "EMENames".into()],
        };

        // Correct password works
        let result = derive_master_key(password, &config);
        assert!(result.is_ok());
        assert_eq!(*result.unwrap(), fake_master);

        // Wrong password fails
        let result = derive_master_key("wrong-password", &config);
        assert!(result.is_err());
        match result.unwrap_err() {
            KdfError::DecryptionFailed => {} // expected
            other => panic!("Expected DecryptionFailed, got {:?}", other),
        }
    }

    #[test]
    fn test_derive_master_key_invalid_encrypted_key_length() {
        use base64::engine::general_purpose::STANDARD;
        use base64::Engine;

        let config = crate::crypto::config::GocryptfsConfig {
            creator: "test".into(),
            encrypted_key: STANDARD.encode(&[0u8; 10]), // too short: < 12+16
            scrypt_object: crate::crypto::config::ScryptObject {
                salt: STANDARD.encode(&[0u8; 32]),
                n: 16,
                r: 8,
                p: 1,
                key_len: 32,
            },
            version: 2,
            feature_flags: vec!["GCMIV128".into()],
        };

        let result = derive_master_key("anything", &config);
        assert!(matches!(result, Err(KdfError::InvalidKeyLength)));
    }

    #[test]
    fn test_derive_master_key_bad_base64_salt() {
        let config = crate::crypto::config::GocryptfsConfig {
            creator: "test".into(),
            encrypted_key: "valid_base64_but_irrelevant".into(),
            scrypt_object: crate::crypto::config::ScryptObject {
                salt: "not-valid-base64!!!".into(),
                n: 16,
                r: 8,
                p: 1,
                key_len: 32,
            },
            version: 2,
            feature_flags: vec![],
        };

        let result = derive_master_key("password", &config);
        assert!(result.is_err());
    }

    // ---- gocryptfs HKDF known-vector tests ----
    // These vectors are derived from gocryptfs Go implementation behavior:
    // HKDF-SHA256 with salt=None, IKM=master key, info=purpose string.

    fn hex_encode(data: &[u8]) -> String {
        data.iter().map(|b| format!("{:02x}", b)).collect()
    }

    #[test]
    fn test_hkdf_vector_zero_key_filename() {
        // master=0x00*32, info="EME filename encryption"
        let master = [0x00u8; 32];
        let result = derive_filename_key(&master).unwrap();
        assert_eq!(
            hex_encode(result.as_ref()),
            "9ba3cddd48c6339c6e56ebe85f0281d6e9051be4104176e65cb0f8a6f77ae6b4",
            "HKDF(0x00*32, 'EME filename encryption') mismatch"
        );
    }

    #[test]
    fn test_hkdf_vector_zero_key_filename_constant_name() {
        // Verify the derive_filename_key function uses the correct info string
        // by checking the same vector again via raw HKDF.
        let master = [0x00u8; 32];
        let hk = Hkdf::<Sha256>::new(None, &master);
        let mut derived = [0u8; 32];
        hk.expand(b"EME filename encryption", &mut derived).unwrap();
        assert_eq!(
            hex_encode(&derived),
            "9ba3cddd48c6339c6e56ebe85f0281d6e9051be4104176e65cb0f8a6f77ae6b4",
            "Raw HKDF with 'EME filename encryption' info string mismatch"
        );
    }

    #[test]
    fn test_hkdf_vector_one_key_filename() {
        // master=0x01*32, info="EME filename encryption"
        let master = [0x01u8; 32];
        let result = derive_filename_key(&master).unwrap();
        assert_eq!(
            hex_encode(result.as_ref()),
            "e8a2499f48700b954f31de732efd04abce822f5c948e7fbc0896607be0d36d12",
            "HKDF(0x01*32, 'EME filename encryption') mismatch"
        );
    }

    #[test]
    fn test_hkdf_vector_one_key_content() {
        // master=0x01*32, info="AES-GCM file content encryption"
        let master = [0x01u8; 32];
        let result = derive_content_key(&master).unwrap();
        assert_eq!(
            hex_encode(result.as_ref()),
            "9137f2e67a842484137f3c458f357f204c30d7458f94f432fa989be96854a649",
            "HKDF(0x01*32, 'AES-GCM file content encryption') mismatch"
        );
    }
}
