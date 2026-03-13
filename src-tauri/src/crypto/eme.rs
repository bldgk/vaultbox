//! AES-256-EME (ECB-Mix-ECB) implementation for gocryptfs filename encryption.
//!
//! Ported from the Go `eme` package by rfjakob (github.com/rfjakob/eme).
//! Based on "A Parallelizable Enciphering Mode" by Halevi & Rogaway.

use aes::cipher::{BlockDecrypt, BlockEncrypt, KeyInit, generic_array::GenericArray};
use aes::Aes256;

const BLOCK_SIZE: usize = 16;

/// Multiply by 2 in GF(2^128) as per EME-32 spec.
/// Matches Go `multByTwo` exactly.
fn mult_by_two(input: &[u8; BLOCK_SIZE], output: &mut [u8; BLOCK_SIZE]) {
    output[0] = 2u8.wrapping_mul(input[0]);
    // Constant-time conditional XOR: if input[15] >= 128, XOR with 135 (0x87)
    output[0] ^= 135 & (input[15] >> 7).wrapping_neg();
    for j in 1..BLOCK_SIZE {
        output[j] = 2u8.wrapping_mul(input[j]);
        output[j] = output[j].wrapping_add(input[j - 1] >> 7);
    }
}

fn xor_blocks(dst: &mut [u8], a: &[u8], b: &[u8]) {
    for i in 0..dst.len() {
        dst[i] = a[i] ^ b[i];
    }
}

/// Tabulate L values: L_i = 2^(i+1) * E_K(0)
fn tabulate_l(cipher: &Aes256, m: usize) -> Vec<[u8; BLOCK_SIZE]> {
    // L = E_K(0)
    let mut li = [0u8; BLOCK_SIZE];
    let ga = GenericArray::from_mut_slice(&mut li);
    cipher.encrypt_block(ga);

    let mut table = Vec::with_capacity(m);
    for _ in 0..m {
        let mut next = [0u8; BLOCK_SIZE];
        mult_by_two(&li, &mut next);
        table.push(next);
        li = next;
    }
    table
}

/// EME Transform - handles both encrypt and decrypt.
///
/// Key differences from naive implementation:
/// - L is derived from E_K(0), not from the tweak
/// - MP includes XOR with tweak T
/// - MC uses the direction parameter (E for encrypt, D for decrypt)
/// - CCC[0] = MC ⊕ T ⊕ (⊕ CCC[j] for j >= 2)
fn eme_transform(key: &[u8; 32], tweak: &[u8; BLOCK_SIZE], data: &[u8], encrypt: bool) -> Vec<u8> {
    assert!(
        !data.is_empty() && data.len() % BLOCK_SIZE == 0,
        "EME data must be non-empty and a multiple of 16 bytes"
    );

    let cipher = Aes256::new(GenericArray::from_slice(key));
    let m = data.len() / BLOCK_SIZE;
    let p = data;

    // Output array
    let mut c = vec![0u8; data.len()];

    let l_table = tabulate_l(&cipher, m);

    // First pass: PPj = Pj ⊕ L[j], then PPPj = AES(PPj, direction)
    let mut ppj = [0u8; BLOCK_SIZE];
    for j in 0..m {
        let pj = &p[j * BLOCK_SIZE..(j + 1) * BLOCK_SIZE];
        xor_blocks(&mut ppj, pj, &l_table[j]);
        c[j * BLOCK_SIZE..(j + 1) * BLOCK_SIZE].copy_from_slice(&ppj);
        let dst = GenericArray::from_mut_slice(&mut c[j * BLOCK_SIZE..(j + 1) * BLOCK_SIZE]);
        if encrypt {
            cipher.encrypt_block(dst);
        } else {
            cipher.decrypt_block(dst);
        }
    }

    // MP = (⊕ PPP[j]) ⊕ T
    let mut mp = [0u8; BLOCK_SIZE];
    xor_blocks(&mut mp, &c[0..BLOCK_SIZE], tweak);
    for j in 1..m {
        for k in 0..BLOCK_SIZE {
            mp[k] ^= c[j * BLOCK_SIZE + k];
        }
    }

    // MC = AES(MP, direction)  -- uses the DIRECTION, not always encrypt!
    let mut mc = [0u8; BLOCK_SIZE];
    mc.copy_from_slice(&mp);
    let mc_ga = GenericArray::from_mut_slice(&mut mc);
    if encrypt {
        cipher.encrypt_block(mc_ga);
    } else {
        cipher.decrypt_block(mc_ga);
    }

    // M = MP ⊕ MC
    let mut m_val = [0u8; BLOCK_SIZE];
    xor_blocks(&mut m_val, &mp, &mc);

    // For j = 1..m-1: CCCj = PPPj ⊕ (2^j * M)
    let mut cccj = [0u8; BLOCK_SIZE];
    for j in 1..m {
        let mut new_m = [0u8; BLOCK_SIZE];
        mult_by_two(&m_val, &mut new_m);
        m_val = new_m;
        xor_blocks(&mut cccj, &c[j * BLOCK_SIZE..(j + 1) * BLOCK_SIZE], &m_val);
        c[j * BLOCK_SIZE..(j + 1) * BLOCK_SIZE].copy_from_slice(&cccj);
    }

    // CCC1 = MC ⊕ T ⊕ (⊕ CCC[j] for j >= 2)
    let mut ccc1 = [0u8; BLOCK_SIZE];
    xor_blocks(&mut ccc1, &mc, tweak);
    for j in 1..m {
        for k in 0..BLOCK_SIZE {
            ccc1[k] ^= c[j * BLOCK_SIZE + k];
        }
    }
    c[0..BLOCK_SIZE].copy_from_slice(&ccc1);

    // Second pass: CCj = AES(CCCj, direction), then Cj = CCj ⊕ L[j]
    for j in 0..m {
        let block = GenericArray::from_mut_slice(&mut c[j * BLOCK_SIZE..(j + 1) * BLOCK_SIZE]);
        if encrypt {
            cipher.encrypt_block(block);
        } else {
            cipher.decrypt_block(block);
        }
        for k in 0..BLOCK_SIZE {
            c[j * BLOCK_SIZE + k] ^= l_table[j][k];
        }
    }

    c
}

/// AES-256-EME encrypt.
pub fn eme_encrypt(key: &[u8; 32], tweak: &[u8; BLOCK_SIZE], plaintext: &[u8]) -> Vec<u8> {
    eme_transform(key, tweak, plaintext, true)
}

/// AES-256-EME decrypt.
pub fn eme_decrypt(key: &[u8; 32], tweak: &[u8; BLOCK_SIZE], ciphertext: &[u8]) -> Vec<u8> {
    eme_transform(key, tweak, ciphertext, false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_eme_roundtrip_2blocks() {
        let key = [0x42u8; 32];
        let tweak = [0x01u8; 16];
        let plaintext = [0xABu8; 32]; // 2 blocks

        let ciphertext = eme_encrypt(&key, &tweak, &plaintext);
        assert_ne!(&ciphertext[..], &plaintext[..]);

        let decrypted = eme_decrypt(&key, &tweak, &ciphertext);
        assert_eq!(&decrypted[..], &plaintext[..]);
    }

    #[test]
    fn test_eme_roundtrip_1block() {
        let key = [0x00u8; 32];
        let tweak = [0x00u8; 16];
        let plaintext = [0xFFu8; 16];

        let ciphertext = eme_encrypt(&key, &tweak, &plaintext);
        let decrypted = eme_decrypt(&key, &tweak, &ciphertext);
        assert_eq!(&decrypted[..], &plaintext[..]);
    }

    #[test]
    fn test_eme_roundtrip_8blocks() {
        let key: [u8; 32] = (0..32).collect::<Vec<u8>>().try_into().unwrap();
        let tweak: [u8; 16] = (0..16).collect::<Vec<u8>>().try_into().unwrap();
        let plaintext: Vec<u8> = (0..128).collect(); // 8 blocks

        let ciphertext = eme_encrypt(&key, &tweak, &plaintext);
        let decrypted = eme_decrypt(&key, &tweak, &ciphertext);
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_eme_deterministic() {
        let key = [0x42u8; 32];
        let tweak = [0x01u8; 16];
        let plaintext = [0xABu8; 32];

        let ct1 = eme_encrypt(&key, &tweak, &plaintext);
        let ct2 = eme_encrypt(&key, &tweak, &plaintext);
        assert_eq!(ct1, ct2);
    }

    #[test]
    fn test_eme_various_sizes() {
        // Test all valid sizes from 1 to 8 blocks
        for num_blocks in 1..=8 {
            let key = [0x42u8; 32];
            let tweak = [0x99u8; 16];
            let plaintext: Vec<u8> = (0..num_blocks * 16)
                .map(|i| (i % 256) as u8)
                .collect();

            let ciphertext = eme_encrypt(&key, &tweak, &plaintext);
            let decrypted = eme_decrypt(&key, &tweak, &ciphertext);
            assert_eq!(decrypted, plaintext, "Failed for {} blocks", num_blocks);
        }
    }
}
