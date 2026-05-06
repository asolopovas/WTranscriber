use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::Result;

pub fn audio_cache_key(path: &Path) -> Result<String> {
    let meta = std::fs::metadata(path)?;
    let abs: PathBuf = std::path::absolute(path)?;
    let mtime_ns = meta
        .modified()?
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    let key = format!("{}|{}|{}", abs.display(), meta.len(), mtime_ns);
    let hash = Sha256::digest(key.as_bytes());
    Ok(format!("{}.wav", hex::encode(&hash[..12])))
}
