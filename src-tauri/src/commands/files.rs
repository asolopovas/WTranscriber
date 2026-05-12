#![allow(clippy::needless_pass_by_value)]

use std::path::{Path, PathBuf};

use super::file_names::unique_child_path;
use crate::{
    browser::{self, DirListing},
    error::{Error, Result},
    logfile, transcriber,
    transcriber::{Transcript, export::Format as ExportFormat},
};

#[tauri::command]
pub fn list_directory(path: Option<PathBuf>) -> Result<DirListing> {
    let p = path.unwrap_or_else(browser::home_dir);
    browser::list(&p)
}

#[tauri::command]
pub fn default_dir() -> PathBuf {
    browser::home_dir()
}

#[tauri::command]
pub async fn add_to_workdir(source: PathBuf, workdir: PathBuf) -> Result<PathBuf> {
    tokio::task::spawn_blocking(move || add_to_workdir_blocking(&source, &workdir))
        .await
        .map_err(|e| Error::Config(format!("task: {e}")))?
}

fn add_to_workdir_blocking(source: &Path, workdir: &Path) -> Result<PathBuf> {
    if !source.is_file() {
        return Err(Error::Config(format!("not a file: {}", source.display())));
    }
    std::fs::create_dir_all(workdir)?;
    let file_name = source
        .file_name()
        .ok_or_else(|| Error::Config("source has no file name".into()))?;
    let initial_dst = workdir.join(file_name);
    if let Ok(src_canon) = std::fs::canonicalize(source)
        && let Ok(dst_canon) = std::fs::canonicalize(&initial_dst)
        && src_canon == dst_canon
    {
        return Ok(initial_dst);
    }
    let dst = unique_child_path(workdir, file_name, "file").ok_or_else(|| {
        Error::Config(format!(
            "too many copies of '{}' in workdir",
            file_name.to_string_lossy()
        ))
    })?;
    std::fs::copy(source, &dst)?;
    logfile::info(&format!(
        "add_to_workdir {} -> {}",
        source.display(),
        dst.display()
    ));
    Ok(dst)
}

#[tauri::command]
pub fn rename_file(source: PathBuf, new_name: String) -> Result<PathBuf> {
    let trimmed = new_name.trim();
    if trimmed.is_empty() {
        return Err(Error::Config("new name is empty".into()));
    }
    if trimmed.contains(['/', '\\']) {
        return Err(Error::Config(
            "new name must not contain path separators".into(),
        ));
    }
    let parent = source
        .parent()
        .ok_or_else(|| Error::Config("source has no parent directory".into()))?;
    let mut target_name = trimmed.to_owned();
    if Path::new(&target_name).extension().is_none()
        && let Some(ext) = source.extension().and_then(|e| e.to_str())
        && !ext.is_empty()
    {
        target_name.push('.');
        target_name.push_str(ext);
    }
    let dst = parent.join(&target_name);
    if dst == source {
        return Ok(dst);
    }
    if dst.exists() {
        return Err(Error::Config(format!(
            "destination already exists: {target_name}"
        )));
    }
    std::fs::rename(&source, &dst)?;
    if let Err(e) = transcriber::cache::rename_source(&source, &dst) {
        logfile::warn(&format!("cache index rename failed: {e}"));
    }
    logfile::info(&format!("rename {} -> {}", source.display(), dst.display()));
    Ok(dst)
}

#[tauri::command]
pub fn reveal_in_folder(path: PathBuf) -> Result<()> {
    if !path.exists() {
        return Err(Error::Config(format!(
            "path does not exist: {}",
            path.display()
        )));
    }
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer.exe")
            .arg(format!("/select,{}", path.display()))
            .spawn()
            .map_err(|e| Error::Config(format!("explorer failed: {e}")))?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn()
            .map_err(|e| Error::Config(format!("open -R failed: {e}")))?;
        Ok(())
    }
    #[cfg(all(unix, not(target_os = "macos"), not(target_os = "android")))]
    {
        let parent = path
            .parent()
            .ok_or_else(|| Error::Config("no parent directory".into()))?;
        std::process::Command::new("xdg-open")
            .arg(parent)
            .spawn()
            .map_err(|e| Error::Config(format!("xdg-open failed: {e}")))?;
        Ok(())
    }
    #[cfg(target_os = "android")]
    {
        if crate::android_reveal_path(&path.to_string_lossy()) {
            Ok(())
        } else {
            Err(Error::Config("could not open path on android".into()))
        }
    }
}

#[tauri::command]
pub fn delete_file(path: PathBuf) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    std::fs::remove_file(&path)?;
    logfile::info(&format!("delete {}", path.display()));
    Ok(())
}

#[tauri::command]
pub fn format_transcript(transcript: Transcript, format: ExportFormat) -> Result<String> {
    let mut buf: Vec<u8> = Vec::new();
    transcriber::export::write_to(&transcript, &mut buf, format)?;
    String::from_utf8(buf).map_err(|e| Error::Config(format!("format_transcript utf-8: {e}")))
}

#[tauri::command]
#[allow(
    clippy::needless_pass_by_value,
    unused_variables,
    clippy::unnecessary_wraps
)]
pub fn share_transcript(title: String, text: String) -> Result<bool> {
    #[cfg(target_os = "android")]
    {
        if !crate::android_share_text(&title, &text) {
            return Err(Error::Config("android share unavailable".into()));
        }
        logfile::info(&format!("share '{title}' ({} chars)", text.len()));
        return Ok(true);
    }
    #[cfg(not(target_os = "android"))]
    {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_to_workdir_returns_existing_path_for_same_file() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("clip.wav");
        std::fs::write(&source, b"audio").unwrap();

        let copied = add_to_workdir_blocking(&source, dir.path()).unwrap();

        assert_eq!(copied, source);
    }

    #[test]
    fn add_to_workdir_uses_unique_name_for_duplicate() {
        let dir = tempfile::tempdir().unwrap();
        let source_dir = dir.path().join("source");
        let workdir = dir.path().join("work");
        std::fs::create_dir_all(&source_dir).unwrap();
        std::fs::create_dir_all(&workdir).unwrap();
        let source = source_dir.join("clip.wav");
        std::fs::write(&source, b"new").unwrap();
        std::fs::write(workdir.join("clip.wav"), b"old").unwrap();

        let copied = add_to_workdir_blocking(&source, &workdir).unwrap();

        assert_eq!(copied.file_name().unwrap(), "clip (1).wav");
        assert_eq!(std::fs::read(copied).unwrap(), b"new");
    }

    #[test]
    fn rename_file_preserves_extension_when_missing() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("old.wav");
        std::fs::write(&source, b"audio").unwrap();

        let renamed = rename_file(source, "new name".into()).unwrap();

        assert_eq!(renamed.file_name().unwrap(), "new name.wav");
        assert!(renamed.exists());
    }

    #[test]
    fn rename_file_rejects_path_separators() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("old.wav");
        std::fs::write(&source, b"audio").unwrap();

        let err = rename_file(source, "nested/name".into()).unwrap_err();

        assert!(err.to_string().contains("path separators"));
    }
}
