mod cache;
pub mod decode;
pub mod ffmpeg;
pub mod meta;
mod wav;

use std::path::Path;

pub use cache::audio_cache_key;
pub use ffmpeg::find_ffmpeg;

pub fn probe_duration_ms(path: &std::path::Path) -> Option<u64> {
    ffmpeg::probe_duration_ms(path).or_else(|| decode::probe_duration_ms(path))
}
pub use meta::AudioMeta;
#[allow(unused_imports, dead_code)]
pub use wav::write_pcm16_wav;
pub use wav::{WHISPER_SAMPLE_RATE, read_pcm16_wav};

use crate::error::{Error, Result};

pub fn load_samples(path: &Path) -> Result<Vec<f32>> {
    if !path.exists() {
        return Err(Error::Transcribe(format!(
            "audio file not found: {}",
            path.display()
        )));
    }
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
        && let Ok(samples) = read_pcm16_wav(path)
    {
        return Ok(samples);
    }
    convert_and_load(path)
}

fn convert_and_load(path: &Path) -> Result<Vec<f32>> {
    read_pcm16_wav(&ensure_decoded_wav(path, true)?)
}

fn run_decoder(input: &Path, output: &Path) -> Result<()> {
    if let Some(ffmpeg) = find_ffmpeg() {
        return ffmpeg::run(&ffmpeg, input, output);
    }
    decode::decode_to_wav(input, output)
}

fn ensure_decoded_wav(path: &Path, allow_temp_fallback: bool) -> Result<std::path::PathBuf> {
    let cache_dir = crate::paths::cache_dir()?;
    let cached = match audio_cache_key(path) {
        Ok(name) => Some(cache_dir.join(name)),
        Err(_) if allow_temp_fallback => None,
        Err(err) => return Err(err),
    };
    let target = cached
        .unwrap_or_else(|| std::env::temp_dir().join(format!("wt-{}.wav", std::process::id())));

    if target.exists() {
        return Ok(target);
    }
    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if let Err(err) = run_decoder(path, &target) {
        let _ = std::fs::remove_file(&target);
        return Err(err);
    }
    Ok(target)
}

pub fn ensure_cached_wav(path: &Path) -> Result<std::path::PathBuf> {
    if path
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
    {
        return Ok(path.to_path_buf());
    }
    ensure_decoded_wav(path, false)
}

pub fn clear_cache() -> Result<u64> {
    let cache_dir = crate::paths::cache_dir()?;
    std::fs::create_dir_all(&cache_dir)?;
    let mut removed = 0_u64;
    for entry in std::fs::read_dir(cache_dir)? {
        let path = entry?.path();
        if path.is_file()
            && path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("wav"))
        {
            std::fs::remove_file(path)?;
            removed += 1;
        }
    }
    Ok(removed)
}

pub fn waveform_peaks(path: &Path, bins: usize) -> Result<Vec<f32>> {
    let samples = load_samples(path)?;
    if samples.is_empty() || bins == 0 {
        return Ok(Vec::new());
    }
    let bins = bins.min(samples.len());
    let step = samples.len().div_ceil(bins);
    let mut peaks = Vec::with_capacity(bins);
    for chunk in samples.chunks(step) {
        let peak = chunk.iter().fold(0.0_f32, |acc, v| acc.max(v.abs()));
        peaks.push(peak.min(1.0));
    }
    Ok(peaks)
}
