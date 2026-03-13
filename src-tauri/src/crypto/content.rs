//! gocryptfs file content encryption/decryption.
//!
//! File format:
//! - 18-byte header: 2 bytes version (0x00 0x02) + 16 bytes file ID
//! - Content blocks: each 4096 bytes plaintext → 4112 bytes encrypted (4096 + 16 GCM tag)
//!
//! Nonce construction: 96-bit nonce = fileID XOR block_number(big-endian, zero-padded to 16 bytes)
//! then truncated to 12 bytes. Actually in gocryptfs:
//! - Nonce is 12 bytes: first 12 bytes of (file_id XOR zero-padded block_number)
//! Wait, let me re-read the spec. The nonce is constructed differently:
//! - 96-bit nonce = file_id[0..12] XOR block_number_padded[0..12] (taking first 12 bytes)
//! But file_id is 16 bytes... Let me check gocryptfs source.
//!
//! Actually per gocryptfs source:
//! - Nonce is 12 bytes (96 bits) for GCM-IV128 mode
//! - Nonce = file_id[0..16] is used to derive block nonces
//! - Block nonce: take fileID (16 bytes), XOR the block number into the last 8 bytes (big-endian),
//!   then use the first 12 bytes as the GCM nonce.

use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use thiserror::Error;
pub const HEADER_LEN: usize = 18;
pub const FILE_ID_LEN: usize = 16;
pub const BLOCK_SIZE_PLAIN: usize = 4096;
pub const BLOCK_SIZE_CIPHER: usize = 4096 + 16; // plaintext + GCM tag
const VERSION_BYTES: [u8; 2] = [0x00, 0x02];
const NONCE_LEN: usize = 12;

#[derive(Debug, Error)]
pub enum ContentError {
    #[error("Invalid file header")]
    InvalidHeader,
    #[error("Decryption failed at block {0}")]
    DecryptionFailed(u64),
    #[error("Encryption failed at block {0}")]
    EncryptionFailed(u64),
    #[error("File is empty")]
    EmptyFile,
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

/// Parse the 18-byte gocryptfs file header.
/// Returns the 16-byte file ID.
pub fn parse_header(data: &[u8]) -> Result<[u8; FILE_ID_LEN], ContentError> {
    if data.len() < HEADER_LEN {
        return Err(ContentError::InvalidHeader);
    }
    if data[0..2] != VERSION_BYTES {
        return Err(ContentError::InvalidHeader);
    }
    let mut file_id = [0u8; FILE_ID_LEN];
    file_id.copy_from_slice(&data[2..HEADER_LEN]);
    Ok(file_id)
}

/// Create a new file header with a random file ID.
pub fn create_header() -> ([u8; HEADER_LEN], [u8; FILE_ID_LEN]) {
    use rand::RngCore;
    let mut file_id = [0u8; FILE_ID_LEN];
    rand::rng().fill_bytes(&mut file_id);

    let mut header = [0u8; HEADER_LEN];
    header[0..2].copy_from_slice(&VERSION_BYTES);
    header[2..HEADER_LEN].copy_from_slice(&file_id);

    (header, file_id)
}

/// Construct the GCM nonce for a given block number.
/// Takes the 16-byte file ID, XORs the block number into bytes 8..16 (big-endian),
/// then uses bytes 0..12 as the nonce.
fn block_nonce(file_id: &[u8; FILE_ID_LEN], block_num: u64) -> [u8; NONCE_LEN] {
    let mut buf = *file_id;
    let bn_bytes = block_num.to_be_bytes();
    // XOR block number into the last 8 bytes of the file ID
    for i in 0..8 {
        buf[8 + i] ^= bn_bytes[i];
    }
    let mut nonce = [0u8; NONCE_LEN];
    nonce.copy_from_slice(&buf[..NONCE_LEN]);
    nonce
}

/// Construct the AAD (additional authenticated data) for a block.
/// It's the block number as big-endian u64.
fn block_aad(block_num: u64) -> [u8; 8] {
    block_num.to_be_bytes()
}

/// Decrypt an entire encrypted file's content.
/// `data` is the full file bytes (including header).
/// Returns the decrypted plaintext.
pub fn decrypt_file(key: &[u8; 32], data: &[u8]) -> Result<Vec<u8>, ContentError> {
    if data.len() < HEADER_LEN {
        if data.is_empty() {
            return Ok(Vec::new());
        }
        return Err(ContentError::InvalidHeader);
    }

    let file_id = parse_header(data)?;
    let content = &data[HEADER_LEN..];

    if content.is_empty() {
        return Ok(Vec::new());
    }

    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|_| ContentError::DecryptionFailed(0))?;

    let num_blocks = (content.len() + BLOCK_SIZE_CIPHER - 1) / BLOCK_SIZE_CIPHER;
    let mut plaintext = Vec::with_capacity(num_blocks * BLOCK_SIZE_PLAIN);

    for block_num in 0..num_blocks as u64 {
        let start = block_num as usize * BLOCK_SIZE_CIPHER;
        let end = std::cmp::min(start + BLOCK_SIZE_CIPHER, content.len());
        let block = &content[start..end];

        let nonce_bytes = block_nonce(&file_id, block_num);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let aad = block_aad(block_num);

        let payload = Payload {
            msg: block,
            aad: &aad,
        };

        let mut decrypted = cipher
            .decrypt(nonce, payload)
            .map_err(|_| ContentError::DecryptionFailed(block_num))?;

        plaintext.append(&mut decrypted);
    }

    Ok(plaintext)
}

/// Encrypt plaintext content into gocryptfs format.
/// Returns the full encrypted file bytes (header + encrypted blocks).
pub fn encrypt_file(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, ContentError> {
    let (header, file_id) = create_header();

    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|_| ContentError::EncryptionFailed(0))?;

    let num_blocks = if plaintext.is_empty() {
        0
    } else {
        (plaintext.len() + BLOCK_SIZE_PLAIN - 1) / BLOCK_SIZE_PLAIN
    };

    let mut output = Vec::with_capacity(HEADER_LEN + num_blocks * BLOCK_SIZE_CIPHER);
    output.extend_from_slice(&header);

    for block_num in 0..num_blocks as u64 {
        let start = block_num as usize * BLOCK_SIZE_PLAIN;
        let end = std::cmp::min(start + BLOCK_SIZE_PLAIN, plaintext.len());
        let block = &plaintext[start..end];

        let nonce_bytes = block_nonce(&file_id, block_num);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let aad = block_aad(block_num);

        let payload = Payload {
            msg: block,
            aad: &aad,
        };

        let encrypted = cipher
            .encrypt(nonce, payload)
            .map_err(|_| ContentError::EncryptionFailed(block_num))?;

        output.extend_from_slice(&encrypted);
    }

    Ok(output)
}

/// Encrypt plaintext using a specific file ID (for overwriting existing files).
pub fn encrypt_file_with_id(
    key: &[u8; 32],
    file_id: &[u8; FILE_ID_LEN],
    plaintext: &[u8],
) -> Result<Vec<u8>, ContentError> {
    let cipher =
        Aes256Gcm::new_from_slice(key).map_err(|_| ContentError::EncryptionFailed(0))?;

    let num_blocks = if plaintext.is_empty() {
        0
    } else {
        (plaintext.len() + BLOCK_SIZE_PLAIN - 1) / BLOCK_SIZE_PLAIN
    };

    let mut header = [0u8; HEADER_LEN];
    header[0..2].copy_from_slice(&VERSION_BYTES);
    header[2..HEADER_LEN].copy_from_slice(file_id);

    let mut output = Vec::with_capacity(HEADER_LEN + num_blocks * BLOCK_SIZE_CIPHER);
    output.extend_from_slice(&header);

    for block_num in 0..num_blocks as u64 {
        let start = block_num as usize * BLOCK_SIZE_PLAIN;
        let end = std::cmp::min(start + BLOCK_SIZE_PLAIN, plaintext.len());
        let block = &plaintext[start..end];

        let nonce_bytes = block_nonce(file_id, block_num);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let aad = block_aad(block_num);

        let payload = Payload {
            msg: block,
            aad: &aad,
        };

        let encrypted = cipher
            .encrypt(nonce, payload)
            .map_err(|_| ContentError::EncryptionFailed(block_num))?;

        output.extend_from_slice(&encrypted);
    }

    Ok(output)
}

/// Calculate the plaintext size from the ciphertext file size.
pub fn plaintext_size(ciphertext_size: u64) -> u64 {
    if ciphertext_size <= HEADER_LEN as u64 {
        return 0;
    }
    let content_size = ciphertext_size - HEADER_LEN as u64;
    let full_blocks = content_size / BLOCK_SIZE_CIPHER as u64;
    let remainder = content_size % BLOCK_SIZE_CIPHER as u64;

    let mut plain_size = full_blocks * BLOCK_SIZE_PLAIN as u64;
    if remainder > 16 {
        // Last partial block: subtract GCM tag
        plain_size += remainder - 16;
    }
    plain_size
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let key = [0x42u8; 32];
        let plaintext = b"Hello, encrypted world! This is a test of content encryption.";

        let encrypted = encrypt_file(&key, plaintext).unwrap();
        assert!(encrypted.len() > HEADER_LEN);

        let decrypted = decrypt_file(&key, &encrypted).unwrap();
        assert_eq!(&decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_decrypt_empty() {
        let key = [0x42u8; 32];
        let plaintext = b"";

        let encrypted = encrypt_file(&key, plaintext).unwrap();
        assert_eq!(encrypted.len(), HEADER_LEN);

        let decrypted = decrypt_file(&key, &encrypted).unwrap();
        assert!(decrypted.is_empty());
    }

    #[test]
    fn test_encrypt_decrypt_large() {
        let key = [0x42u8; 32];
        let plaintext: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();

        let encrypted = encrypt_file(&key, &plaintext).unwrap();
        let decrypted = decrypt_file(&key, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_decrypt_exact_block() {
        let key = [0x42u8; 32];
        let plaintext = vec![0xABu8; BLOCK_SIZE_PLAIN]; // exactly one block

        let encrypted = encrypt_file(&key, &plaintext).unwrap();
        assert_eq!(encrypted.len(), HEADER_LEN + BLOCK_SIZE_CIPHER);

        let decrypted = decrypt_file(&key, &encrypted).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_plaintext_size_calculation() {
        assert_eq!(plaintext_size(0), 0);
        assert_eq!(plaintext_size(HEADER_LEN as u64), 0);
        // One full block
        assert_eq!(
            plaintext_size(HEADER_LEN as u64 + BLOCK_SIZE_CIPHER as u64),
            BLOCK_SIZE_PLAIN as u64
        );
        // One full block + partial
        assert_eq!(
            plaintext_size(HEADER_LEN as u64 + BLOCK_SIZE_CIPHER as u64 + 100),
            BLOCK_SIZE_PLAIN as u64 + 84 // 100 - 16 tag
        );
    }

    #[test]
    fn test_header_roundtrip() {
        let (header, file_id) = create_header();
        let parsed_id = parse_header(&header).unwrap();
        assert_eq!(file_id, parsed_id);
    }
}
