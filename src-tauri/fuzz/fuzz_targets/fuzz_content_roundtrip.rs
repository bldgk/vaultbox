#![no_main]
mod crypto;

use libfuzzer_sys::fuzz_target;

/// Fuzz encrypt→decrypt roundtrip.
/// Goal: verify that decrypt(encrypt(plaintext)) == plaintext for all inputs.
fuzz_target!(|data: &[u8]| {
    let key = [0x42u8; 32];

    let encrypted = match crypto::content::encrypt_file(&key, data) {
        Ok(e) => e,
        Err(_) => return,
    };

    let decrypted = crypto::content::decrypt_file(&key, &encrypted)
        .expect("decrypt must succeed on data we just encrypted");

    assert_eq!(
        decrypted.as_slice(),
        data,
        "roundtrip mismatch: plaintext len={}",
        data.len()
    );
});
