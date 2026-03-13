use aes_gcm::{
    aead::{Aead, KeyInit},
    Aes256Gcm, Nonce,
};
use hkdf::Hkdf;
use sha2::Sha256;
use zeroize::Zeroizing;

use super::config::GocryptfsConfig;
use thiserror::Error;

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

    // Decrypt the master key
    let encrypted_key = STANDARD
        .decode(&config.encrypted_key)?;

    // The encrypted key is: nonce (12 bytes) + ciphertext+tag
    if encrypted_key.len() < 12 + 16 {
        return Err(KdfError::InvalidKeyLength);
    }

    let nonce = Nonce::from_slice(&encrypted_key[..12]);
    let ciphertext = &encrypted_key[12..];

    let cipher = Aes256Gcm::new_from_slice(scrypt_key.as_ref())
        .map_err(|_| KdfError::InvalidKeyLength)?;

    let master_key_vec = cipher
        .decrypt(nonce, ciphertext)
        .map_err(|_| KdfError::DecryptionFailed)?;

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
}
