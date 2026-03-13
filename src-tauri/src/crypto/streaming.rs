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
use zeroize::{Zeroize, Zeroizing};

const DEFAULT_CACHE_SIZE: usize = 256; // ~1 MB cache (256 * 4096)

pub struct StreamingReader {
    file: File,
    file_id: [u8; FILE_ID_LEN],
    cipher: Aes256Gcm,
    position: u64,
    plaintext_size: u64,
    cache: LruCache<u64, Zeroizing<Vec<u8>>>,
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

    fn decrypt_block(&mut self, block_num: u64) -> io::Result<Zeroizing<Vec<u8>>> {
        if let Some(cached) = self.cache.get(&block_num) {
            return Ok(Zeroizing::new((**cached).clone()));
        }

        let file_offset = HEADER_LEN as u64 + block_num * BLOCK_SIZE_CIPHER as u64;
        self.file.seek(SeekFrom::Start(file_offset))?;

        let mut block_buf = vec![0u8; BLOCK_SIZE_CIPHER];
        let bytes_read = self.file.read(&mut block_buf)?;
        if bytes_read == 0 {
            block_buf.zeroize();
            return Ok(Zeroizing::new(Vec::new()));
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

        block_buf.zeroize();
        self.cache.put(block_num, Zeroizing::new(decrypted.clone()));
        Ok(Zeroizing::new(decrypted))
    }

    pub fn plaintext_size(&self) -> u64 {
        self.plaintext_size
    }
}

impl Drop for StreamingReader {
    fn drop(&mut self) {
        self.cache.clear(); // Drops all Zeroizing<Vec<u8>> entries, zeroizing each
        self.file_id.zeroize();
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Seek, SeekFrom};

    /// Helper: encrypt plaintext and write to a temp file, return path
    fn write_encrypted_file(key: &[u8; 32], plaintext: &[u8]) -> tempfile::NamedTempFile {
        let encrypted = content::encrypt_file(key, plaintext).unwrap();
        let mut f = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut f, &encrypted).unwrap();
        f
    }

    #[test]
    fn test_streaming_read_small_file() {
        let key = [0x42u8; 32];
        let plaintext = b"hello streaming world";
        let f = write_encrypted_file(&key, plaintext);

        let mut reader = StreamingReader::open(f.path(), &key).unwrap();
        assert_eq!(reader.plaintext_size(), plaintext.len() as u64);

        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, plaintext);
    }

    #[test]
    fn test_streaming_read_empty_file() {
        let key = [0x42u8; 32];
        let f = write_encrypted_file(&key, b"");

        let mut reader = StreamingReader::open(f.path(), &key).unwrap();
        assert_eq!(reader.plaintext_size(), 0);

        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).unwrap();
        assert!(buf.is_empty());
    }

    #[test]
    fn test_streaming_read_multi_block() {
        let key = [0x42u8; 32];
        let plaintext: Vec<u8> = (0..10000).map(|i| (i % 256) as u8).collect();
        let f = write_encrypted_file(&key, &plaintext);

        let mut reader = StreamingReader::open(f.path(), &key).unwrap();
        assert_eq!(reader.plaintext_size(), plaintext.len() as u64);

        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, plaintext);
    }

    #[test]
    fn test_streaming_read_exact_block_size() {
        let key = [0x42u8; 32];
        let plaintext = vec![0xAB; BLOCK_SIZE_PLAIN];
        let f = write_encrypted_file(&key, &plaintext);

        let mut reader = StreamingReader::open(f.path(), &key).unwrap();
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, plaintext);
    }

    #[test]
    fn test_streaming_seek_start() {
        let key = [0x42u8; 32];
        let plaintext = b"abcdefghij";
        let f = write_encrypted_file(&key, plaintext);

        let mut reader = StreamingReader::open(f.path(), &key).unwrap();

        // Read first 3 bytes
        let mut buf = [0u8; 3];
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"abc");

        // Seek back to start
        reader.seek(SeekFrom::Start(0)).unwrap();
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(&buf, b"abc");
    }

    #[test]
    fn test_streaming_seek_to_offset() {
        let key = [0x42u8; 32];
        let plaintext = b"0123456789";
        let f = write_encrypted_file(&key, plaintext);

        let mut reader = StreamingReader::open(f.path(), &key).unwrap();
        reader.seek(SeekFrom::Start(5)).unwrap();

        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, b"56789");
    }

    #[test]
    fn test_streaming_seek_end() {
        let key = [0x42u8; 32];
        let plaintext = b"0123456789";
        let f = write_encrypted_file(&key, plaintext);

        let mut reader = StreamingReader::open(f.path(), &key).unwrap();
        let pos = reader.seek(SeekFrom::End(-3)).unwrap();
        assert_eq!(pos, 7);

        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, b"789");
    }

    #[test]
    fn test_streaming_seek_current() {
        let key = [0x42u8; 32];
        let plaintext = b"abcdefghij";
        let f = write_encrypted_file(&key, plaintext);

        let mut reader = StreamingReader::open(f.path(), &key).unwrap();
        reader.seek(SeekFrom::Start(2)).unwrap();
        reader.seek(SeekFrom::Current(3)).unwrap();

        let mut buf = [0u8; 1];
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(buf[0], b'f'); // position 5
    }

    #[test]
    fn test_streaming_seek_before_start_fails() {
        let key = [0x42u8; 32];
        let plaintext = b"test";
        let f = write_encrypted_file(&key, plaintext);

        let mut reader = StreamingReader::open(f.path(), &key).unwrap();
        let result = reader.seek(SeekFrom::Current(-1));
        assert!(result.is_err());
    }

    #[test]
    fn test_streaming_seek_past_end_reads_zero() {
        let key = [0x42u8; 32];
        let plaintext = b"test";
        let f = write_encrypted_file(&key, plaintext);

        let mut reader = StreamingReader::open(f.path(), &key).unwrap();
        reader.seek(SeekFrom::Start(100)).unwrap();

        let mut buf = [0u8; 10];
        let n = reader.read(&mut buf).unwrap();
        assert_eq!(n, 0); // EOF
    }

    #[test]
    fn test_streaming_partial_reads() {
        let key = [0x42u8; 32];
        let plaintext: Vec<u8> = (0..100).collect();
        let f = write_encrypted_file(&key, &plaintext);

        let mut reader = StreamingReader::open(f.path(), &key).unwrap();
        let mut result = Vec::new();

        // Read in small chunks
        let mut buf = [0u8; 7];
        loop {
            let n = reader.read(&mut buf).unwrap();
            if n == 0 {
                break;
            }
            result.extend_from_slice(&buf[..n]);
        }
        assert_eq!(result, plaintext);
    }

    #[test]
    fn test_streaming_read_across_block_boundary() {
        let key = [0x42u8; 32];
        // Plaintext spanning 2 blocks
        let plaintext: Vec<u8> = (0u8..200).cycle().take(BLOCK_SIZE_PLAIN + 100).collect();
        let f = write_encrypted_file(&key, &plaintext);

        let mut reader = StreamingReader::open(f.path(), &key).unwrap();

        // Seek to near the block boundary and read across it
        let offset = BLOCK_SIZE_PLAIN as u64 - 10;
        reader.seek(SeekFrom::Start(offset)).unwrap();

        let mut buf = [0u8; 20]; // reads 10 from block 0, 10 from block 1
        reader.read_exact(&mut buf).unwrap();
        assert_eq!(&buf[..], &plaintext[offset as usize..offset as usize + 20]);
    }

    #[test]
    fn test_streaming_open_invalid_file() {
        let key = [0x42u8; 32];
        let mut f = tempfile::NamedTempFile::new().unwrap();
        std::io::Write::write_all(&mut f, b"too small").unwrap();

        let result = StreamingReader::open(f.path(), &key);
        assert!(result.is_err());
    }

    #[test]
    fn test_streaming_cache_efficiency() {
        let key = [0x42u8; 32];
        let plaintext = vec![0xAB; BLOCK_SIZE_PLAIN * 3]; // 3 blocks
        let f = write_encrypted_file(&key, &plaintext);

        let mut reader = StreamingReader::open(f.path(), &key).unwrap();

        // Read all data
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).unwrap();
        assert_eq!(buf, plaintext);

        // Seek back and read again (should hit cache)
        reader.seek(SeekFrom::Start(0)).unwrap();
        let mut buf2 = Vec::new();
        reader.read_to_end(&mut buf2).unwrap();
        assert_eq!(buf2, plaintext);
    }
}
