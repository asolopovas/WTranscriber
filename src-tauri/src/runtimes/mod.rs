pub mod cudnn;
pub mod inproc_cuda;
pub mod llama;
pub mod sherpa;

use std::path::{Path, PathBuf};

use crate::{
    error::{Error, Result},
    process::quiet_command,
};

pub use cudnn::{
    ensure as ensure_cudnn, is_installed as cudnn_installed, supported as cudnn_supported,
};
pub use llama::{ensure as ensure_llama, is_installed as llama_installed};
pub use sherpa::{
    Variant as SherpaVariant, ensure as ensure_sherpa, is_installed as sherpa_installed,
};

pub fn extract(archive: &Path, dest: &Path) -> Result<()> {
    let name = archive
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or_default()
        .to_lowercase();
    let flag = if name.ends_with(".tar.bz2") || ext_eq(&name, "tbz2") {
        "-xjf"
    } else if name.ends_with(".tar.gz") || ext_eq(&name, "tgz") {
        "-xzf"
    } else if name.ends_with(".tar.xz") || ext_eq(&name, "txz") {
        "-xJf"
    } else if ext_eq(&name, "zip") {
        "-xf"
    } else {
        return Err(Error::Config(format!(
            "unsupported archive format: {}",
            archive.display()
        )));
    };

    let status = quiet_command("tar")
        .arg(flag)
        .arg(archive)
        .arg("-C")
        .arg(dest)
        .status()
        .map_err(|e| Error::Config(format!("tar invoke failed: {e}")))?;
    if !status.success() {
        return Err(Error::Config(format!(
            "tar exit {} extracting {}",
            status,
            archive.display()
        )));
    }
    Ok(())
}

fn ext_eq(name: &str, ext: &str) -> bool {
    Path::new(name)
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case(ext))
}

pub fn locate_bin_dir(root: &Path, target: &str) -> Option<PathBuf> {
    let target_os = std::ffi::OsStr::new(target);
    crate::process::walk_for_file(root, 5, |p| p.is_file() && p.file_name() == Some(target_os))
        .and_then(|p| p.parent().map(Path::to_path_buf))
}

pub fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let ty = entry.file_type()?;
        let to = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &to)?;
        } else {
            std::fs::copy(entry.path(), &to)?;
        }
    }
    Ok(())
}

pub fn move_or_copy_dir(src: &Path, dst: &Path) -> Result<()> {
    if dst.exists() {
        let _ = std::fs::remove_dir_all(dst);
    }
    if let Some(parent) = dst.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::rename(src, dst).is_err() {
        copy_dir_all(src, dst)?;
    }
    Ok(())
}
