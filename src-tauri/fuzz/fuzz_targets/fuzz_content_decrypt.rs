#![no_main]
mod crypto;

use libfuzzer_sys::fuzz_target;

/// Fuzz content decryption with arbitrary bytes.
/// Goal: find panics, OOM, or unexpected behavior when decrypting malformed data.
fuzz_target!(|data: &[u8]| {
    let key = [0x42u8; 32];
    // Should never panic — only return Ok or Err
    let _ = crypto::content::decrypt_file(&key, data);
});
