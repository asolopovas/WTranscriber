#![allow(clippy::needless_pass_by_value)]

use std::path::{Path, PathBuf};

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
    let mut dst = workdir.join(file_name);
    if let Ok(src_canon) = std::fs::canonicalize(source)
        && let Ok(dst_canon) = std::fs::canonicalize(&dst)
        && src_canon == dst_canon
    {
        return Ok(dst);
    }
    if dst.exists() {
        let stem = Path::new(file_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        let ext = Path::new(file_name)
            .extension()
            .and_then(|s| s.to_str())
            .unwrap_or("");
        for n in 1..=999 {
            let candidate = if ext.is_empty() {
                workdir.join(format!("{stem} ({n})"))
            } else {
                workdir.join(format!("{stem} ({n}).{ext}"))
            };
            if !candidate.exists() {
                dst = candidate;
                break;
            }
        }
    }
    if dst.exists() {
        return Err(Error::Config(format!(
            "too many copies of {file_name:?} in workdir"
        )));
    }
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
pub fn delete_file(path: PathBuf) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }
    std::fs::remove_file(&path)?;
    logfile::info(&format!("delete {}", path.display()));
    Ok(())
}

#[tauri::command]
pub fn export_transcript(
    transcript: Transcript,
    dest: PathBuf,
    format: ExportFormat,
) -> Result<PathBuf> {
    transcriber::export::write(&transcript, &dest, format)?;
    logfile::info(&format!("export {:?} -> {}", format, dest.display()));
    Ok(dest)
}
