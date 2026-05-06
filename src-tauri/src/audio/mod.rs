mod cache;
mod ffmpeg;
mod wav;

use std::path::Path;

pub use cache::audio_cache_key;
pub use ffmpeg::{find_ffmpeg, probe_duration_ms};
#[allow(unused_imports)]
pub use ffmpeg::find_ffprobe;
pub use wav::{WHISPER_SAMPLE_RATE, read_pcm16_wav};
#[allow(unused_imports, dead_code)]
pub use wav::write_pcm16_wav;

use crate::error::{Error, Result};

pub fn load_samples(path: &Path) -> Result<Vec<f32>> {
    if !path.exists() {
        return Err(Error::Transcribe(format!(
            "audio file not found: {}",
            path.display()
        )));
    }
    if path.extension().is_some_and(|e| e.eq_ignore_ascii_case("wav"))
        && let Ok(samples) = read_pcm16_wav(path)
    {
        return Ok(samples);
    }
    convert_and_load(path)
}

fn convert_and_load(path: &Path) -> Result<Vec<f32>> {
    let ffmpeg = find_ffmpeg().ok_or_else(|| {
        Error::Transcribe(
            "ffmpeg not found; install ffmpeg or provide a 16 kHz mono WAV file".into(),
        )
    })?;

    let cache_dir = crate::paths::cache_dir()?;
    let cached = audio_cache_key(path).ok().map(|name| cache_dir.join(name));

    if let Some(target) = &cached
        && target.exists()
    {
        return read_pcm16_wav(target);
    }

    let target = cached.unwrap_or_else(|| {
        std::env::temp_dir().join(format!(
            "wt-{}.wav",
            std::process::id()
        ))
    });

    if let Some(parent) = target.parent() {
        std::fs::create_dir_all(parent)?;
    }

    if let Err(err) = ffmpeg::run(&ffmpeg, path, &target) {
        let _ = std::fs::remove_file(&target);
        return Err(err);
    }

    read_pcm16_wav(&target)
}
