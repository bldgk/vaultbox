use std::fs;
use std::path::Path;
use thiserror::Error;

pub const DIRIV_FILENAME: &str = "gocryptfs.diriv";
pub const DIRIV_LEN: usize = 16;

#[derive(Debug, Error)]
pub enum DirIvError {
    #[error("Failed to read gocryptfs.diriv: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid diriv length: expected {DIRIV_LEN}, got {0}")]
    InvalidLength(usize),
}

/// Read the per-directory IV from a `gocryptfs.diriv` file.
pub fn read_diriv(dir_path: &Path) -> Result<[u8; DIRIV_LEN], DirIvError> {
    let iv_path = dir_path.join(DIRIV_FILENAME);
    let data = fs::read(&iv_path)?;
    if data.len() != DIRIV_LEN {
        return Err(DirIvError::InvalidLength(data.len()));
    }
    let mut iv = [0u8; DIRIV_LEN];
    iv.copy_from_slice(&data);
    Ok(iv)
}

/// Create a new `gocryptfs.diriv` file with random bytes.
pub fn create_diriv(dir_path: &Path) -> Result<[u8; DIRIV_LEN], DirIvError> {
    use rand::RngCore;
    let mut iv = [0u8; DIRIV_LEN];
    rand::rng().fill_bytes(&mut iv);
    let iv_path = dir_path.join(DIRIV_FILENAME);
    fs::write(&iv_path, &iv)?;
    Ok(iv)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn test_create_and_read_diriv() {
        let dir = tempfile::tempdir().unwrap();
        let iv = create_diriv(dir.path()).unwrap();
        assert_eq!(iv.len(), DIRIV_LEN);

        let read_iv = read_diriv(dir.path()).unwrap();
        assert_eq!(iv, read_iv);
    }

    #[test]
    fn test_create_diriv_random() {
        let dir1 = tempfile::tempdir().unwrap();
        let dir2 = tempfile::tempdir().unwrap();
        let iv1 = create_diriv(dir1.path()).unwrap();
        let iv2 = create_diriv(dir2.path()).unwrap();
        assert_ne!(iv1, iv2); // random IVs should differ
    }

    #[test]
    fn test_read_diriv_missing_file() {
        let dir = tempfile::tempdir().unwrap();
        let result = read_diriv(dir.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_read_diriv_wrong_length() {
        let dir = tempfile::tempdir().unwrap();
        let iv_path = dir.path().join(DIRIV_FILENAME);
        fs::write(&iv_path, &[0u8; 10]).unwrap(); // wrong length
        let result = read_diriv(dir.path());
        assert!(matches!(result, Err(DirIvError::InvalidLength(10))));
    }

    #[test]
    fn test_read_diriv_too_long() {
        let dir = tempfile::tempdir().unwrap();
        let iv_path = dir.path().join(DIRIV_FILENAME);
        fs::write(&iv_path, &[0u8; 32]).unwrap(); // too long
        let result = read_diriv(dir.path());
        assert!(matches!(result, Err(DirIvError::InvalidLength(32))));
    }

    #[test]
    fn test_read_diriv_exact_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let iv_path = dir.path().join(DIRIV_FILENAME);
        let expected = [1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16];
        fs::write(&iv_path, &expected).unwrap();
        let iv = read_diriv(dir.path()).unwrap();
        assert_eq!(iv, expected);
    }

    #[test]
    fn test_create_diriv_file_persists() {
        let dir = tempfile::tempdir().unwrap();
        create_diriv(dir.path()).unwrap();
        let iv_path = dir.path().join(DIRIV_FILENAME);
        assert!(iv_path.exists());
        let data = fs::read(&iv_path).unwrap();
        assert_eq!(data.len(), DIRIV_LEN);
    }
}
