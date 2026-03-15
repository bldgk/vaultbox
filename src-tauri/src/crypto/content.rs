//! gocryptfs file content encryption/decryption.
//!
//! File format:
//! - 18-byte header: 2 bytes version (0x00 0x02) + 16 bytes file ID
//! - Content blocks: each 4096 bytes plaintext → 4112 bytes encrypted (4096 + 16 GCM tag)
//!
//! Nonce construction (per gocryptfs source):
//!
//! - Nonce is 12 bytes (96 bits) for GCM-IV128 mode
//! - Block nonce: take fileID (16 bytes), XOR the block number into the last 8 bytes (big-endian),
//!   then use the first 12 bytes as the GCM nonce.

use aes::Aes256;
use aes_gcm::{
    aead::{Aead, KeyInit, Payload, consts::U16},
    AesGcm, Nonce,
};
use thiserror::Error;
use zeroize::{Zeroize, Zeroizing};

/// GCM with 16-byte nonce (GCMIV128)
type Aes256Gcm16 = AesGcm<Aes256, U16>;

pub const HEADER_LEN: usize = 18;
pub const FILE_ID_LEN: usize = 16;
pub const BLOCK_SIZE_PLAIN: usize = 4096;
/// Cipher block = IV(16) + ciphertext(4096) + GCM tag(16) = 4128
pub const BLOCK_SIZE_CIPHER: usize = 16 + 4096 + 16; // IV + plaintext + GCM tag
const VERSION_BYTES: [u8; 2] = [0x00, 0x02];
const IV_LEN: usize = 16;

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

/// Construct the AAD (additional authenticated data) for a block.
/// gocryptfs format: blockNo (8 bytes big-endian) + fileID (16 bytes) = 24 bytes.
/// When fileID is None (e.g. for master key wrapping), AAD is just the block number.
fn block_aad(block_num: u64, file_id: &[u8; FILE_ID_LEN]) -> Vec<u8> {
    let mut aad = Vec::with_capacity(8 + FILE_ID_LEN);
    aad.extend_from_slice(&block_num.to_be_bytes());
    aad.extend_from_slice(file_id);
    aad
}

/// Decrypt an entire encrypted file's content.
/// `data` is the full file bytes (including header).
/// Returns the decrypted plaintext wrapped in Zeroizing for automatic zeroization on drop.
///
/// gocryptfs block format: each cipher block = IV(16) + ciphertext + tag(16).
/// AAD = blockNo(8 bytes BE) + fileID(16 bytes).
pub fn decrypt_file(key: &[u8; 32], data: &[u8]) -> Result<Zeroizing<Vec<u8>>, ContentError> {
    if data.len() < HEADER_LEN {
        if data.is_empty() {
            return Ok(Zeroizing::new(Vec::new()));
        }
        return Err(ContentError::InvalidHeader);
    }

    let file_id = parse_header(data)?;
    let content = &data[HEADER_LEN..];

    if content.is_empty() {
        return Ok(Zeroizing::new(Vec::new()));
    }

    let cipher =
        Aes256Gcm16::new_from_slice(key).map_err(|_| ContentError::DecryptionFailed(0))?;

    let num_blocks = content.len().div_ceil(BLOCK_SIZE_CIPHER);
    let mut plaintext = Vec::with_capacity(num_blocks * BLOCK_SIZE_PLAIN);

    for block_num in 0..num_blocks as u64 {
        let start = block_num as usize * BLOCK_SIZE_CIPHER;
        let end = std::cmp::min(start + BLOCK_SIZE_CIPHER, content.len());
        let block = &content[start..end];

        if block.len() < IV_LEN {
            return Err(ContentError::DecryptionFailed(block_num));
        }

        // Each block starts with a 16-byte random IV
        let nonce = Nonce::from_slice(&block[..IV_LEN]);
        let ciphertext_and_tag = &block[IV_LEN..];

        let aad = block_aad(block_num, &file_id);
        let payload = Payload {
            msg: ciphertext_and_tag,
            aad: &aad,
        };

        let mut decrypted = cipher
            .decrypt(nonce, payload)
            .map_err(|_| ContentError::DecryptionFailed(block_num))?;

        plaintext.append(&mut decrypted);
        decrypted.zeroize();
    }

    Ok(Zeroizing::new(plaintext))
}

/// Encrypt plaintext content into gocryptfs format.
/// Returns the full encrypted file bytes (header + encrypted blocks).
/// Each block = random_IV(16) + ciphertext + GCM_tag(16).
pub fn encrypt_file(key: &[u8; 32], plaintext: &[u8]) -> Result<Vec<u8>, ContentError> {
    let (header, file_id) = create_header();
    encrypt_file_inner(key, &file_id, &header, plaintext)
}

/// Encrypt plaintext using a specific file ID (for overwriting existing files).
pub fn encrypt_file_with_id(
    key: &[u8; 32],
    file_id: &[u8; FILE_ID_LEN],
    plaintext: &[u8],
) -> Result<Vec<u8>, ContentError> {
    let mut header = [0u8; HEADER_LEN];
    header[0..2].copy_from_slice(&VERSION_BYTES);
    header[2..HEADER_LEN].copy_from_slice(file_id);
    encrypt_file_inner(key, file_id, &header, plaintext)
}

fn encrypt_file_inner(
    key: &[u8; 32],
    file_id: &[u8; FILE_ID_LEN],
    header: &[u8; HEADER_LEN],
    plaintext: &[u8],
) -> Result<Vec<u8>, ContentError> {
    use rand::RngCore;

    let cipher =
        Aes256Gcm16::new_from_slice(key).map_err(|_| ContentError::EncryptionFailed(0))?;

    let num_blocks = if plaintext.is_empty() {
        0
    } else {
        plaintext.len().div_ceil(BLOCK_SIZE_PLAIN)
    };

    let mut output = Vec::with_capacity(HEADER_LEN + num_blocks * BLOCK_SIZE_CIPHER);
    output.extend_from_slice(header);

    for block_num in 0..num_blocks as u64 {
        let start = block_num as usize * BLOCK_SIZE_PLAIN;
        let end = std::cmp::min(start + BLOCK_SIZE_PLAIN, plaintext.len());
        let block = &plaintext[start..end];

        // Random 16-byte IV per block
        let mut iv = [0u8; IV_LEN];
        rand::rng().fill_bytes(&mut iv);

        let aad = block_aad(block_num, file_id);
        let payload = Payload {
            msg: block,
            aad: &aad,
        };

        let encrypted = cipher
            .encrypt(Nonce::from_slice(&iv), payload)
            .map_err(|_| ContentError::EncryptionFailed(block_num))?;

        // Block on disk = IV + ciphertext + tag
        output.extend_from_slice(&iv);
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
    let block_overhead = (IV_LEN + 16) as u64; // IV + GCM tag = 32
    if remainder > block_overhead {
        plain_size += remainder - block_overhead;
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
        assert_eq!(decrypted.as_slice(), plaintext);
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
        assert_eq!(*decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_decrypt_exact_block() {
        let key = [0x42u8; 32];
        let plaintext = vec![0xABu8; BLOCK_SIZE_PLAIN]; // exactly one block

        let encrypted = encrypt_file(&key, &plaintext).unwrap();
        assert_eq!(encrypted.len(), HEADER_LEN + BLOCK_SIZE_CIPHER);

        let decrypted = decrypt_file(&key, &encrypted).unwrap();
        assert_eq!(*decrypted, plaintext);
    }

    #[test]
    fn test_plaintext_size_calculation() {
        assert_eq!(plaintext_size(0), 0);
        assert_eq!(plaintext_size(HEADER_LEN as u64), 0);
        // One full block: IV(16) + ciphertext(4096) + tag(16) = 4128
        assert_eq!(
            plaintext_size(HEADER_LEN as u64 + BLOCK_SIZE_CIPHER as u64),
            BLOCK_SIZE_PLAIN as u64
        );
        // One full block + partial (100 bytes - 32 overhead = 68 plain)
        assert_eq!(
            plaintext_size(HEADER_LEN as u64 + BLOCK_SIZE_CIPHER as u64 + 100),
            BLOCK_SIZE_PLAIN as u64 + 68
        );
    }

    #[test]
    fn test_header_roundtrip() {
        let (header, file_id) = create_header();
        let parsed_id = parse_header(&header).unwrap();
        assert_eq!(file_id, parsed_id);
    }

    // --- New tests ---

    #[test]
    fn test_encrypt_decrypt_multi_block() {
        // Exactly 2 full blocks
        let key = [0x42u8; 32];
        let plaintext = vec![0xCDu8; BLOCK_SIZE_PLAIN * 2];
        let encrypted = encrypt_file(&key, &plaintext).unwrap();
        assert_eq!(encrypted.len(), HEADER_LEN + BLOCK_SIZE_CIPHER * 2);
        let decrypted = decrypt_file(&key, &encrypted).unwrap();
        assert_eq!(*decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_decrypt_block_plus_one() {
        // One full block + 1 byte → 2 cipher blocks
        let key = [0x42u8; 32];
        let plaintext = vec![0xEFu8; BLOCK_SIZE_PLAIN + 1];
        let encrypted = encrypt_file(&key, &plaintext).unwrap();
        // Second block: IV(16) + 1 byte ciphertext + tag(16) = 33
        assert_eq!(encrypted.len(), HEADER_LEN + BLOCK_SIZE_CIPHER + 33);
        let decrypted = decrypt_file(&key, &encrypted).unwrap();
        assert_eq!(*decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_decrypt_very_large() {
        // 1 MB file spanning many blocks
        let key = [0x99u8; 32];
        let plaintext: Vec<u8> = (0..1_000_000).map(|i| (i % 251) as u8).collect();
        let encrypted = encrypt_file(&key, &plaintext).unwrap();
        let decrypted = decrypt_file(&key, &encrypted).unwrap();
        assert_eq!(*decrypted, plaintext);
    }

    #[test]
    fn test_decrypt_wrong_key_fails() {
        let key1 = [0x42u8; 32];
        let key2 = [0x43u8; 32];
        let plaintext = b"secret data";
        let encrypted = encrypt_file(&key1, plaintext).unwrap();
        let result = decrypt_file(&key2, &encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_truncated_ciphertext_fails() {
        let key = [0x42u8; 32];
        let plaintext = b"hello world";
        let encrypted = encrypt_file(&key, plaintext).unwrap();
        // Truncate the ciphertext (remove last byte from block)
        let truncated = &encrypted[..encrypted.len() - 1];
        let result = decrypt_file(&key, truncated);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_corrupted_block_fails() {
        let key = [0x42u8; 32];
        let plaintext = b"hello world";
        let mut encrypted = encrypt_file(&key, plaintext).unwrap();
        // Flip a bit in the ciphertext block (after header)
        encrypted[HEADER_LEN + 5] ^= 0xFF;
        let result = decrypt_file(&key, &encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_decrypt_corrupted_header_version_fails() {
        let key = [0x42u8; 32];
        let plaintext = b"test";
        let mut encrypted = encrypt_file(&key, plaintext).unwrap();
        encrypted[0] = 0xFF; // corrupt version byte
        let result = decrypt_file(&key, &encrypted);
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_header_too_short() {
        let result = parse_header(&[0x00, 0x02, 0x01]); // only 3 bytes
        assert!(result.is_err());
    }

    #[test]
    fn test_parse_header_wrong_version() {
        let mut data = [0u8; HEADER_LEN];
        data[0] = 0x00;
        data[1] = 0x03; // wrong version
        assert!(parse_header(&data).is_err());
    }

    #[test]
    fn test_encrypt_file_with_id_roundtrip() {
        let key = [0x42u8; 32];
        let file_id = [0xAA; FILE_ID_LEN];
        let plaintext = b"testing encrypt_file_with_id";

        let encrypted = encrypt_file_with_id(&key, &file_id, plaintext).unwrap();
        // Verify header contains our file_id
        let parsed_id = parse_header(&encrypted).unwrap();
        assert_eq!(parsed_id, file_id);

        let decrypted = decrypt_file(&key, &encrypted).unwrap();
        assert_eq!(decrypted.as_slice(), plaintext);
    }

    #[test]
    fn test_encrypt_file_with_id_random_iv() {
        // With random IVs, same input → different ciphertext each time
        let key = [0x42u8; 32];
        let file_id = [0xBB; FILE_ID_LEN];
        let plaintext = b"random iv test";

        let ct1 = encrypt_file_with_id(&key, &file_id, plaintext).unwrap();
        let ct2 = encrypt_file_with_id(&key, &file_id, plaintext).unwrap();
        // Headers are the same (same file_id) but block IVs differ
        assert_eq!(&ct1[..HEADER_LEN], &ct2[..HEADER_LEN]);
        assert_ne!(&ct1[HEADER_LEN..], &ct2[HEADER_LEN..]);
    }

    #[test]
    fn test_encrypt_file_different_file_ids_differ() {
        let key = [0x42u8; 32];
        let file_id1 = [0xAA; FILE_ID_LEN];
        let file_id2 = [0xBB; FILE_ID_LEN];
        let plaintext = b"same plaintext";

        let ct1 = encrypt_file_with_id(&key, &file_id1, plaintext).unwrap();
        let ct2 = encrypt_file_with_id(&key, &file_id2, plaintext).unwrap();
        // Headers differ and ciphertext blocks differ (different nonces)
        assert_ne!(ct1, ct2);
    }

    #[test]
    fn test_decrypt_empty_input() {
        let key = [0x42u8; 32];
        let result = decrypt_file(&key, &[]);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_decrypt_header_only() {
        let key = [0x42u8; 32];
        // Valid header but no content blocks
        let (header, _) = create_header();
        let result = decrypt_file(&key, &header);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn test_block_aad_format() {
        let fid = [0xAA; FILE_ID_LEN];
        let aad = block_aad(0, &fid);
        // AAD = blockNo(8 bytes) + fileID(16 bytes) = 24 bytes
        assert_eq!(aad.len(), 24);
        assert_eq!(&aad[..8], &[0, 0, 0, 0, 0, 0, 0, 0]); // block 0
        assert_eq!(&aad[8..], &[0xAA; 16]); // file_id

        let aad1 = block_aad(1, &fid);
        assert_eq!(&aad1[..8], &[0, 0, 0, 0, 0, 0, 0, 1]); // block 1
    }

    #[test]
    fn test_plaintext_size_edge_cases() {
        // Smaller than header
        assert_eq!(plaintext_size(1), 0);
        assert_eq!(plaintext_size(17), 0);
        // Just header, no content
        assert_eq!(plaintext_size(18), 0);
        // Header + 33 bytes (IV(16) + 1 byte plaintext + 16 tag)
        assert_eq!(plaintext_size(18 + 33), 1);
        // Header + just overhead (32 bytes = IV+tag → 0 plain)
        assert_eq!(plaintext_size(18 + 32), 0);
        // Multiple full blocks
        assert_eq!(
            plaintext_size(HEADER_LEN as u64 + BLOCK_SIZE_CIPHER as u64 * 3),
            BLOCK_SIZE_PLAIN as u64 * 3
        );
    }

    #[test]
    fn test_encrypt_decrypt_single_byte() {
        let key = [0x42u8; 32];
        let plaintext = &[0xAA];
        let encrypted = encrypt_file(&key, plaintext).unwrap();
        let decrypted = decrypt_file(&key, &encrypted).unwrap();
        assert_eq!(decrypted.as_slice(), plaintext);
    }

    #[test]
    fn test_encrypt_decrypt_all_zeros() {
        let key = [0u8; 32];
        let plaintext = vec![0u8; 8192]; // 2 blocks of zeros
        let encrypted = encrypt_file(&key, &plaintext).unwrap();
        let decrypted = decrypt_file(&key, &encrypted).unwrap();
        assert_eq!(*decrypted, plaintext);
    }

    #[test]
    fn test_header_file_id_uniqueness() {
        let (_, id1) = create_header();
        let (_, id2) = create_header();
        // Random file IDs should differ (probability of collision is negligible)
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_ciphertext_length_formula() {
        let key = [0x42u8; 32];
        let overhead = IV_LEN + 16; // 32 bytes per block (IV + GCM tag)
        for size in [0, 1, 100, 4095, 4096, 4097, 8192, 10000] {
            let plaintext = vec![0u8; size];
            let encrypted = encrypt_file(&key, &plaintext).unwrap();
            let num_blocks = if size == 0 {
                0
            } else {
                (size + BLOCK_SIZE_PLAIN - 1) / BLOCK_SIZE_PLAIN
            };
            // Each block adds overhead bytes; partial last block is smaller
            let last_plain = if size > 0 { size - (num_blocks - 1) * BLOCK_SIZE_PLAIN } else { 0 };
            let expected_len = if num_blocks == 0 {
                HEADER_LEN
            } else {
                HEADER_LEN
                    + (num_blocks - 1) * BLOCK_SIZE_CIPHER
                    + overhead + last_plain
            };
            assert_eq!(encrypted.len(), expected_len, "Failed for size {}", size);
        }
    }

    // ---- gocryptfs compatibility constants and structure tests ----

    #[test]
    fn test_block_size_cipher_is_4128() {
        // gocryptfs v2 block format: IV(16) + ciphertext(4096) + GCM tag(16) = 4128
        assert_eq!(BLOCK_SIZE_CIPHER, 4128);
        assert_eq!(BLOCK_SIZE_CIPHER, 16 + 4096 + 16);
    }

    #[test]
    fn test_plaintext_size_monotonicity() {
        // plaintext_size must be monotonically non-decreasing for increasing ciphertext sizes.
        // This is critical for correct file size reporting.
        let mut prev = 0u64;
        for ct_size in 0..10000u64 {
            let pt = plaintext_size(ct_size);
            assert!(
                pt >= prev,
                "plaintext_size is not monotonic: plaintext_size({}) = {} < plaintext_size({}) = {}",
                ct_size,
                pt,
                ct_size - 1,
                prev
            );
            prev = pt;
        }
    }

    #[test]
    fn test_block_number_mapping_offset_within_first_block() {
        // Offset 788 bytes into the ciphertext content area should map to block 0.
        // Content starts at HEADER_LEN (18). Offset 788 in the content = byte 806 of the file.
        // Block 0 spans content bytes [0, BLOCK_SIZE_CIPHER).
        // 788 < 4128, so it is block 0.
        let offset_in_content: u64 = 788;
        let block_num = offset_in_content / BLOCK_SIZE_CIPHER as u64;
        assert_eq!(block_num, 0, "offset 788 in content should be block 0");
    }

    #[test]
    fn test_block_number_mapping_second_block() {
        // The second block starts at offset BLOCK_SIZE_CIPHER in the content area.
        // So content offset BLOCK_SIZE_CIPHER should be block 1.
        let offset_in_content = BLOCK_SIZE_CIPHER as u64;
        let block_num = offset_in_content / BLOCK_SIZE_CIPHER as u64;
        assert_eq!(block_num, 1, "offset BLOCK_SIZE_CIPHER in content should be block 1");
    }

    #[test]
    fn test_plaintext_size_matches_encrypt_decrypt() {
        // For various plaintext sizes, verify that plaintext_size(encrypted.len())
        // returns the original plaintext length.
        let key = [0x42u8; 32];
        for size in [0, 1, 15, 16, 100, 4095, 4096, 4097, 8192, 12288, 16384] {
            let plaintext = vec![0xABu8; size];
            let encrypted = encrypt_file(&key, &plaintext).unwrap();
            let computed_pt_size = plaintext_size(encrypted.len() as u64);
            assert_eq!(
                computed_pt_size, size as u64,
                "plaintext_size mismatch for plaintext of {} bytes (ciphertext {} bytes)",
                size, encrypted.len()
            );
        }
    }
}
