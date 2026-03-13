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
