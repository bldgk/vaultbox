#![no_main]
mod crypto;

use libfuzzer_sys::fuzz_target;

/// Fuzz filename encryption/decryption.
/// Tests both decrypt of arbitrary base64 and encrypt→decrypt roundtrip.
fuzz_target!(|data: &[u8]| {
    let key = [0x42u8; 32];
    let dir_iv = [0x01u8; 16];

    // Part 1: Try decrypting arbitrary string (should not panic)
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = crypto::filename::decrypt_filename(&key, &dir_iv, s, true);
        let _ = crypto::filename::decrypt_filename(&key, &dir_iv, s, false);
    }

    // Part 2: Roundtrip — encrypt then decrypt arbitrary valid UTF-8 names
    if let Ok(name) = std::str::from_utf8(data) {
        if !name.is_empty() && name.len() <= 255 {
            if let Ok(encrypted) =
                crypto::filename::encrypt_filename(&key, &dir_iv, name, true)
            {
                let decrypted =
                    crypto::filename::decrypt_filename(&key, &dir_iv, &encrypted, true)
                        .expect("decrypt must succeed on data we just encrypted");
                assert_eq!(decrypted, name, "filename roundtrip mismatch");
            }
        }
    }
});
