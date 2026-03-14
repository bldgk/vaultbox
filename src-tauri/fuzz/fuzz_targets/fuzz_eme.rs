#![no_main]
mod crypto;

use libfuzzer_sys::fuzz_target;

/// Fuzz EME encrypt/decrypt roundtrip.
/// Input must be non-empty and a multiple of 16 bytes.
fuzz_target!(|data: &[u8]| {
    // EME requires non-empty, 16-byte-aligned data
    if data.is_empty() || data.len() % 16 != 0 || data.len() > 4096 {
        return;
    }

    let key = [0x42u8; 32];
    let tweak = [0x01u8; 16];

    let ciphertext = crypto::eme::eme_encrypt(&key, &tweak, data);
    assert_eq!(ciphertext.len(), data.len());

    let decrypted = crypto::eme::eme_decrypt(&key, &tweak, &ciphertext);
    assert_eq!(decrypted.as_slice(), data, "EME roundtrip mismatch");
});
