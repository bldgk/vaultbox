//! Streaming decryption wrapper for large files.
//! Implements on-demand block decryption for Read+Seek access.

use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::Path;

use super::content::{
    self, BLOCK_SIZE_CIPHER, BLOCK_SIZE_PLAIN, FILE_ID_LEN, HEADER_LEN,
};
use aes_gcm::{
    aead::{Aead, KeyInit, Payload},
    Aes256Gcm, Nonce,
};
use lru::LruCache;
use std::num::NonZeroUsize;

const DEFAULT_CACHE_SIZE: usize = 256; // ~1 MB cache (256 * 4096)

pub struct StreamingReader {
    file: File,
    file_id: [u8; FILE_ID_LEN],
    cipher: Aes256Gcm,
    position: u64,
    plaintext_size: u64,
    cache: LruCache<u64, Vec<u8>>,
}

impl StreamingReader {
    pub fn open(path: &Path, key: &[u8; 32]) -> io::Result<Self> {
        let mut file = File::open(path)?;
        let file_size = file.metadata()?.len();

        // Read header
        let mut header = [0u8; HEADER_LEN];
        if file_size < HEADER_LEN as u64 {
            return Err(io::Error::new(io::ErrorKind::InvalidData, "File too small"));
        }
        file.read_exact(&mut header)?;

        let file_id = content::parse_header(&header)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        let cipher = Aes256Gcm::new_from_slice(key)
            .map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e.to_string()))?;

        let plaintext_size = content::plaintext_size(file_size);
        let cache = LruCache::new(NonZeroUsize::new(DEFAULT_CACHE_SIZE).unwrap());

        Ok(StreamingReader {
            file,
            file_id,
            cipher,
            position: 0,
            plaintext_size,
            cache,
        })
    }

    fn decrypt_block(&mut self, block_num: u64) -> io::Result<Vec<u8>> {
        if let Some(cached) = self.cache.get(&block_num) {
            return Ok(cached.clone());
        }

        let file_offset = HEADER_LEN as u64 + block_num * BLOCK_SIZE_CIPHER as u64;
        self.file.seek(SeekFrom::Start(file_offset))?;

        let mut block_buf = vec![0u8; BLOCK_SIZE_CIPHER];
        let bytes_read = self.file.read(&mut block_buf)?;
        if bytes_read == 0 {
            return Ok(Vec::new());
        }
        block_buf.truncate(bytes_read);

        // Construct nonce
        let mut nonce_buf = self.file_id;
        let bn_bytes = block_num.to_be_bytes();
        for i in 0..8 {
            nonce_buf[8 + i] ^= bn_bytes[i];
        }
        let nonce = Nonce::from_slice(&nonce_buf[..12]);
        let aad = block_num.to_be_bytes();

        let payload = Payload {
            msg: &block_buf,
            aad: &aad,
        };

        let decrypted = self
            .cipher
            .decrypt(nonce, payload)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "Decryption failed"))?;

        self.cache.put(block_num, decrypted.clone());
        Ok(decrypted)
    }

    pub fn plaintext_size(&self) -> u64 {
        self.plaintext_size
    }
}

impl Read for StreamingReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.position >= self.plaintext_size {
            return Ok(0);
        }

        let mut total_read = 0;
        while total_read < buf.len() && self.position < self.plaintext_size {
            let block_num = self.position / BLOCK_SIZE_PLAIN as u64;
            let block_offset = (self.position % BLOCK_SIZE_PLAIN as u64) as usize;

            let block_data = self.decrypt_block(block_num)?;
            if block_data.is_empty() {
                break;
            }

            let available = block_data.len() - block_offset;
            let to_copy = std::cmp::min(available, buf.len() - total_read);
            buf[total_read..total_read + to_copy]
                .copy_from_slice(&block_data[block_offset..block_offset + to_copy]);

            total_read += to_copy;
            self.position += to_copy as u64;
        }

        Ok(total_read)
    }
}

impl Seek for StreamingReader {
    fn seek(&mut self, pos: SeekFrom) -> io::Result<u64> {
        let new_pos = match pos {
            SeekFrom::Start(offset) => offset as i64,
            SeekFrom::End(offset) => self.plaintext_size as i64 + offset,
            SeekFrom::Current(offset) => self.position as i64 + offset,
        };

        if new_pos < 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Seek before start of file",
            ));
        }

        self.position = new_pos as u64;
        Ok(self.position)
    }
}
