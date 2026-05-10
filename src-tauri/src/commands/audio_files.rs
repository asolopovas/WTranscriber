#![allow(clippy::needless_pass_by_value)]

use std::path::PathBuf;

use base64::{Engine as _, engine::general_purpose::STANDARD};

use crate::{
    audio,
    error::{Error, Result},
    logfile,
};

#[tauri::command]
pub async fn probe_audio(path: PathBuf) -> Option<u64> {
    tokio::task::spawn_blocking(move || audio::probe_duration_ms(&path))
        .await
        .ok()
        .flatten()
}

#[tauri::command]
pub async fn probe_duration(path: PathBuf) -> Option<u64> {
    tokio::task::spawn_blocking(move || audio::probe_duration_ms(&path))
        .await
        .ok()
        .flatten()
}

#[tauri::command]
pub fn audio_waveform(path: PathBuf, bins: usize) -> Result<Vec<f32>> {
    audio::waveform_peaks(&path, bins)
}

#[tauri::command]
pub fn load_audio_meta(path: PathBuf) -> audio::AudioMeta {
    audio::meta::load(&path).unwrap_or_default()
}

#[tauri::command]
pub fn save_audio_meta(path: PathBuf, meta: audio::AudioMeta) -> Result<()> {
    audio::meta::save(&path, &meta)
}

#[tauri::command]
pub fn read_audio_bytes(path: PathBuf) -> Result<Vec<u8>> {
    Ok(std::fs::read(&path)?)
}

#[tauri::command]
pub fn save_recording(workdir: PathBuf, filename: String, bytes: String) -> Result<PathBuf> {
    let raw = STANDARD
        .decode(bytes.as_bytes())
        .map_err(|e| Error::Config(format!("invalid base64 payload: {e}")))?;
    std::fs::create_dir_all(&workdir)?;
    let safe = filename
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect::<String>();
    let mut dst = workdir.join(&safe);
    if dst.exists() {
        let stem = std::path::Path::new(&safe)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("recording");
        let ext = std::path::Path::new(&safe)
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
            "too many copies of {safe:?} in workdir"
        )));
    }
    std::fs::write(&dst, &raw)?;
    logfile::info(&format!(
        "save_recording {} bytes -> {}",
        raw.len(),
        dst.display()
    ));
    Ok(dst)
}
