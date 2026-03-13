//! gocryptfs filename encryption/decryption.
//!
//! Filenames are encrypted with AES-256-EME using a per-directory IV (gocryptfs.diriv).
//! The encrypted name is then base64url-encoded.

use super::eme;
use sha2::{Sha256, Digest};
use thiserror::Error;

const BLOCK_SIZE: usize = 16;

#[derive(Debug, Error)]
pub enum FilenameError {
    #[error("Base64 decode error: {0}")]
    Base64Decode(String),
    #[error("Encrypted name too short")]
    TooShort,
    #[error("Invalid padding in decrypted filename")]
    InvalidPadding,
    #[error("Filename too long")]
    TooLong,
}

/// Encrypt a plaintext filename using EME with the given key and directory IV.
pub fn encrypt_filename(
    key: &[u8; 32],
    dir_iv: &[u8; 16],
    plaintext_name: &str,
    use_raw64: bool,
) -> Result<String, FilenameError> {
    let padded = pad_filename(plaintext_name.as_bytes());
    let encrypted = eme::eme_encrypt(key, dir_iv, &padded);

    if use_raw64 {
        Ok(base64_raw_url_encode(&encrypted))
    } else {
        Ok(base64_url_encode(&encrypted))
    }
}

/// Decrypt an encrypted filename using EME with the given key and directory IV.
pub fn decrypt_filename(
    key: &[u8; 32],
    dir_iv: &[u8; 16],
    encrypted_name: &str,
    use_raw64: bool,
) -> Result<String, FilenameError> {
    let encrypted_bytes = if use_raw64 {
        base64_raw_url_decode(encrypted_name)?
    } else {
        base64_url_decode(encrypted_name)?
    };

    if encrypted_bytes.is_empty() || encrypted_bytes.len() % BLOCK_SIZE != 0 {
        return Err(FilenameError::TooShort);
    }

    let decrypted = eme::eme_decrypt(key, dir_iv, &encrypted_bytes);
    let unpadded = unpad_filename(&decrypted)?;

    String::from_utf8(unpadded.to_vec()).map_err(|_| FilenameError::InvalidPadding)
}

/// PKCS#7-like padding to 16-byte boundary.
/// gocryptfs pads plaintext filenames to a multiple of 16 bytes.
fn pad_filename(name: &[u8]) -> Vec<u8> {
    let pad_len = BLOCK_SIZE - (name.len() % BLOCK_SIZE);
    let mut padded = Vec::with_capacity(name.len() + pad_len);
    padded.extend_from_slice(name);
    padded.resize(name.len() + pad_len, pad_len as u8);
    padded
}

/// Remove PKCS#7-like padding.
fn unpad_filename(data: &[u8]) -> Result<&[u8], FilenameError> {
    if data.is_empty() {
        return Err(FilenameError::InvalidPadding);
    }

    let pad_byte = data[data.len() - 1];
    if pad_byte == 0 || pad_byte as usize > BLOCK_SIZE || pad_byte as usize > data.len() {
        return Err(FilenameError::InvalidPadding);
    }

    // Verify all padding bytes are the same
    for &b in &data[data.len() - pad_byte as usize..] {
        if b != pad_byte {
            return Err(FilenameError::InvalidPadding);
        }
    }

    Ok(&data[..data.len() - pad_byte as usize])
}

/// Compute the long name hash for a filename.
/// Used when encrypted name > 176 bytes.
pub fn long_name_hash(encrypted_name: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(encrypted_name.as_bytes());
    let hash = hasher.finalize();
    base64_raw_url_encode(&hash)
}

/// Check if an encrypted filename is a long name.
pub fn is_long_name(encrypted_name: &str) -> bool {
    encrypted_name.len() > 176
}

/// Base64url encode without padding (Raw64).
fn base64_raw_url_encode(data: &[u8]) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    URL_SAFE_NO_PAD.encode(data)
}

/// Base64url decode without padding (Raw64).
fn base64_raw_url_decode(s: &str) -> Result<Vec<u8>, FilenameError> {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    URL_SAFE_NO_PAD
        .decode(s)
        .map_err(|e| FilenameError::Base64Decode(e.to_string()))
}

/// Base64url encode with padding.
fn base64_url_encode(data: &[u8]) -> String {
    use base64::engine::general_purpose::URL_SAFE;
    use base64::Engine;
    URL_SAFE.encode(data)
}

/// Base64url decode with padding.
fn base64_url_decode(s: &str) -> Result<Vec<u8>, FilenameError> {
    use base64::engine::general_purpose::URL_SAFE;
    use base64::Engine;
    URL_SAFE
        .decode(s)
        .map_err(|e| FilenameError::Base64Decode(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pad_unpad() {
        let name = b"hello.txt";
        let padded = pad_filename(name);
        assert_eq!(padded.len() % BLOCK_SIZE, 0);
        assert_eq!(padded.len(), 16); // 9 bytes + 7 padding
        let unpadded = unpad_filename(&padded).unwrap();
        assert_eq!(unpadded, name);
    }

    #[test]
    fn test_pad_exact_block() {
        let name = [0x41u8; 16]; // exactly 16 bytes
        let padded = pad_filename(&name);
        assert_eq!(padded.len(), 32); // full block of padding added
        let unpadded = unpad_filename(&padded).unwrap();
        assert_eq!(unpadded, &name[..]);
    }

    #[test]
    fn test_encrypt_decrypt_filename() {
        let key = [0x42u8; 32];
        let dir_iv = [0x01u8; 16];
        let name = "test-document.txt";

        let encrypted = encrypt_filename(&key, &dir_iv, name, true).unwrap();
        let decrypted = decrypt_filename(&key, &dir_iv, &encrypted, true).unwrap();
        assert_eq!(decrypted, name);
    }

    #[test]
    fn test_long_name_hash() {
        let long_encrypted = "a".repeat(200);
        assert!(is_long_name(&long_encrypted));
        let hash = long_name_hash(&long_encrypted);
        assert!(!hash.is_empty());
    }
}
