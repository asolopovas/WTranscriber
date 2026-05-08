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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_is_stable_for_same_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("clip.mp3");
        std::fs::write(&p, b"audio bytes").unwrap();
        let a = audio_cache_key(&p).unwrap();
        let b = audio_cache_key(&p).unwrap();
        assert_eq!(a, b);
        assert!(
            std::path::Path::new(&a)
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
        );
    }

    #[test]
    fn key_differs_for_different_paths() {
        let dir = tempfile::tempdir().unwrap();
        let a_path = dir.path().join("a.mp3");
        let b_path = dir.path().join("b.mp3");
        std::fs::write(&a_path, b"x").unwrap();
        std::fs::write(&b_path, b"x").unwrap();
        assert_ne!(
            audio_cache_key(&a_path).unwrap(),
            audio_cache_key(&b_path).unwrap()
        );
    }

    #[test]
    fn key_errors_when_file_missing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(audio_cache_key(&dir.path().join("nope.mp3")).is_err());
    }
}
