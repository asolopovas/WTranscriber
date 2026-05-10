#![allow(clippy::needless_pass_by_value)]

use std::{ffi::OsString, path::PathBuf};

use base64::{Engine as _, engine::general_purpose::STANDARD};

use super::file_names::unique_child_path;
use crate::{
    audio,
    error::{Error, Result},
    logfile,
};

#[tauri::command]
pub async fn probe_audio(path: PathBuf) -> Option<u64> {
    probe_duration(path).await
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

fn safe_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|c| {
            if matches!(c, '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|') {
                '_'
            } else {
                c
            }
        })
        .collect()
}

#[tauri::command]
pub fn save_recording(workdir: PathBuf, filename: String, bytes: String) -> Result<PathBuf> {
    let raw = STANDARD
        .decode(bytes.as_bytes())
        .map_err(|e| Error::Config(format!("invalid base64 payload: {e}")))?;
    std::fs::create_dir_all(&workdir)?;
    let safe = safe_filename(&filename);
    let safe_name = OsString::from(&safe);
    let dst = unique_child_path(&workdir, &safe_name, "recording")
        .ok_or_else(|| Error::Config(format!("too many copies of {safe:?} in workdir")))?;
    std::fs::write(&dst, &raw)?;
    logfile::info(&format!(
        "save_recording {} bytes -> {}",
        raw.len(),
        dst.display()
    ));
    Ok(dst)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn safe_filename_replaces_forbidden_path_chars() {
        assert_eq!(
            safe_filename("a/b\\c:d*e?f\"g<h>i|j.wav"),
            "a_b_c_d_e_f_g_h_i_j.wav"
        );
    }

    #[test]
    fn save_recording_uses_unique_sanitised_destination() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("take.wav"), []).unwrap();

        let path =
            save_recording(dir.path().to_path_buf(), "take.wav".into(), "AQID".into()).unwrap();

        assert_eq!(path, dir.path().join("take (1).wav"));
        assert_eq!(std::fs::read(path).unwrap(), [1, 2, 3]);
    }
}
