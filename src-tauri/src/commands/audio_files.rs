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
    tokio::task::spawn_blocking(move || {
        let ms = audio::probe_duration_ms(&path)?;
        if path.exists() {
            let mut meta = audio::meta::load(&path).unwrap_or_default();
            if meta.duration_ms != Some(ms) {
                meta.duration_ms = Some(ms);
                let _ = audio::meta::save(&path, &meta);
            }
        }
        Some(ms)
    })
    .await
    .ok()
    .flatten()
}

#[tauri::command]
pub async fn audio_waveform(path: PathBuf, bins: usize) -> Result<Vec<f32>> {
    tokio::task::spawn_blocking(move || audio::waveform_peaks(&path, bins))
        .await
        .map_err(|e| Error::Transcribe(format!("audio_waveform join: {e}")))?
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
pub async fn apply_trim(path: PathBuf) -> Result<Option<u64>> {
    tokio::task::spawn_blocking(move || -> Result<Option<u64>> {
        let meta = audio::meta::load(&path).unwrap_or_default();
        if meta.is_default() {
            return Ok(audio::probe_duration_ms(&path));
        }
        audio::ffmpeg::apply_trim(&path, meta.trim_start_ms, meta.trim_end_ms)?;
        audio::meta::save(&path, &audio::AudioMeta::default())?;
        let evicted = crate::transcriber::cache::invalidate_for_source(&path).unwrap_or(0);
        logfile::info(&format!(
            "apply_trim {} -> start={}ms end={:?}ms; cache evicted={evicted}",
            path.display(),
            meta.trim_start_ms,
            meta.trim_end_ms
        ));
        Ok(audio::probe_duration_ms(&path))
    })
    .await
    .map_err(|e| Error::Transcribe(format!("apply_trim join: {e}")))?
}

#[tauri::command]
pub async fn read_audio_bytes(path: PathBuf) -> Result<tauri::ipc::Response> {
    tokio::task::spawn_blocking(move || {
        let bytes = std::fs::read(&path)?;
        Ok(tauri::ipc::Response::new(bytes))
    })
    .await
    .map_err(|e| Error::Transcribe(format!("read_audio_bytes join: {e}")))?
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

fn header_b64_utf8(request: &tauri::ipc::Request<'_>, key: &str) -> Result<String> {
    let raw = request
        .headers()
        .get(key)
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| Error::Config(format!("missing {key} header")))?;
    let bytes = STANDARD
        .decode(raw.as_bytes())
        .map_err(|e| Error::Config(format!("invalid {key} header: {e}")))?;
    String::from_utf8(bytes).map_err(|e| Error::Config(format!("invalid {key} utf8: {e}")))
}

fn write_recording(workdir: &PathBuf, filename: &str, bytes: &[u8]) -> Result<PathBuf> {
    std::fs::create_dir_all(workdir)?;
    let safe = safe_filename(filename);
    let safe_name = OsString::from(&safe);
    let dst = unique_child_path(workdir, &safe_name, "recording")
        .ok_or_else(|| Error::Config(format!("too many copies of {safe:?} in workdir")))?;
    std::fs::write(&dst, bytes)?;
    logfile::info(&format!(
        "save_recording {} bytes -> {}",
        bytes.len(),
        dst.display()
    ));
    Ok(dst)
}

#[tauri::command]
pub fn save_recording(request: tauri::ipc::Request<'_>) -> Result<PathBuf> {
    let workdir = PathBuf::from(header_b64_utf8(&request, "x-workdir")?);
    let filename = header_b64_utf8(&request, "x-filename")?;
    let bytes: &[u8] = match request.body() {
        tauri::ipc::InvokeBody::Raw(b) => b.as_slice(),
        tauri::ipc::InvokeBody::Json(_) => {
            return Err(Error::Config(
                "save_recording: expected raw body, got json".into(),
            ));
        }
    };
    write_recording(&workdir, &filename, bytes)
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
    fn write_recording_uses_unique_sanitised_destination() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("take.wav"), []).unwrap();

        let path = write_recording(&dir.path().to_path_buf(), "take.wav", &[1, 2, 3]).unwrap();

        assert_eq!(path, dir.path().join("take (1).wav"));
        assert_eq!(std::fs::read(path).unwrap(), [1, 2, 3]);
    }
}
