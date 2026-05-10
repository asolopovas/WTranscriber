use std::{
    ffi::OsStr,
    path::{Path, PathBuf},
};

pub(super) const MAX_COPY_SUFFIX: u16 = 999;

pub(super) fn unique_child_path(
    dir: &Path,
    file_name: &OsStr,
    fallback_stem: &str,
) -> Option<PathBuf> {
    let dst = dir.join(file_name);
    if !dst.exists() {
        return Some(dst);
    }
    let file_path = Path::new(file_name);
    let stem = file_path
        .file_stem()
        .and_then(OsStr::to_str)
        .unwrap_or(fallback_stem);
    let ext = file_path.extension().and_then(OsStr::to_str).unwrap_or("");
    (1..=MAX_COPY_SUFFIX)
        .map(|n| suffixed_path(dir, stem, ext, n))
        .find(|candidate| !candidate.exists())
}

fn suffixed_path(dir: &Path, stem: &str, ext: &str, suffix: u16) -> PathBuf {
    if ext.is_empty() {
        dir.join(format!("{stem} ({suffix})"))
    } else {
        dir.join(format!("{stem} ({suffix}).{ext}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unique_child_path_returns_plain_name_when_available() {
        let dir = tempfile::tempdir().unwrap();

        let path = unique_child_path(dir.path(), OsStr::new("audio.wav"), "file").unwrap();

        assert_eq!(path, dir.path().join("audio.wav"));
    }

    #[test]
    fn unique_child_path_adds_suffix_before_extension() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("audio.wav"), []).unwrap();

        let path = unique_child_path(dir.path(), OsStr::new("audio.wav"), "file").unwrap();

        assert_eq!(path, dir.path().join("audio (1).wav"));
    }

    #[test]
    fn unique_child_path_adds_suffix_without_extension() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("recording"), []).unwrap();

        let path = unique_child_path(dir.path(), OsStr::new("recording"), "file").unwrap();

        assert_eq!(path, dir.path().join("recording (1)"));
    }

    #[test]
    fn unique_child_path_returns_none_after_suffix_limit() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("audio.wav"), []).unwrap();
        for n in 1..=MAX_COPY_SUFFIX {
            std::fs::write(dir.path().join(format!("audio ({n}).wav")), []).unwrap();
        }

        let path = unique_child_path(dir.path(), OsStr::new("audio.wav"), "file");

        assert!(path.is_none());
    }
}
