use std::{io, path::Path};

pub fn ensure_parent_dir(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    Ok(())
}

#[cfg(target_os = "android")]
pub fn copy_recursive(src: &Path, dst: &Path, merge: bool) -> io::Result<u64> {
    let meta = std::fs::metadata(src)?;
    if meta.is_file() {
        ensure_parent_dir(dst)?;
        if merge
            && let Ok(dst_meta) = std::fs::metadata(dst)
            && dst_meta.is_file()
            && dst_meta.len() == meta.len()
        {
            return Ok(0);
        }
        return std::fs::copy(src, dst);
    }
    if !meta.is_dir() {
        return Ok(0);
    }
    std::fs::create_dir_all(dst)?;
    let mut total: u64 = 0;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let Some(name) = from.file_name() else {
            continue;
        };
        total = total.saturating_add(copy_recursive(&from, &dst.join(name), merge)?);
    }
    Ok(total)
}

#[cfg(target_os = "android")]
pub fn remove_recursive(path: &Path) -> io::Result<()> {
    let Ok(meta) = std::fs::metadata(path) else {
        return Ok(());
    };
    if meta.is_dir() {
        std::fs::remove_dir_all(path)
    } else {
        std::fs::remove_file(path)
    }
}
