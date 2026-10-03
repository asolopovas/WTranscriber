mod cache;
pub mod decode;
pub mod ffmpeg;
pub mod meta;
mod wav;
mod waveform;

use std::{path::Path, sync::Mutex};

static DECODE_LOCK: Mutex<()> = Mutex::new(());

pub use cache::audio_cache_key;
pub use ffmpeg::find_ffmpeg;

pub fn probe_duration_ms(path: &std::path::Path) -> Option<u64> {
    ffmpeg::probe_duration_ms(path).or_else(|| decode::probe_duration_ms(path))
}
pub use meta::AudioMeta;
pub use wav::write_pcm16_wav;
pub use wav::{WHISPER_SAMPLE_RATE, read_pcm16_wav};
pub use waveform::waveform_peaks;

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
    let _guard = DECODE_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
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
    crate::fs_utils::ensure_parent_dir(&target)?;
    let pending = tempfile::Builder::new()
        .suffix(".wav")
        .tempfile_in(target.parent().expect("decoded cache has a parent"))?;
    run_decoder(path, pending.path())?;
    pending.persist(&target).map_err(|err| err.error)?;
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

pub fn playback_segment(path: &Path, start_ms: u64, end_ms: u64) -> Result<Vec<u8>> {
    if end_ms <= start_ms {
        return Err(Error::Config(
            "audio segment end must follow its start".into(),
        ));
    }
    let wav = ensure_decoded_wav(path, false)?;
    wav::read_pcm16_wav_segment(&wav, start_ms, end_ms)
}

pub fn clear_cache() -> Result<u64> {
    waveform::clear_cache();
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

#[cfg(test)]
mod playback_tests {
    use super::*;

    #[test]
    #[ignore = "requires WT_TEST_PLAYBACK_AUDIO pointing to a real recording"]
    fn real_recording_produces_a_pcm_fragment() {
        let _guard = crate::paths::PATHS_TEST_LOCK
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let source = std::env::var_os("WT_TEST_PLAYBACK_AUDIO").expect("recording path");
        let dir = tempfile::tempdir().unwrap();
        crate::paths::init(
            dir.path().join("config"),
            dir.path().join("data"),
            dir.path().join("cache"),
        );
        let bytes = playback_segment(Path::new(&source), 1000, 2000).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(bytes.len(), 32044);
        assert_eq!(
            playback_segment(Path::new(&source), 1000, 2000).unwrap(),
            bytes
        );
        if let Some(output) = std::env::var_os("WT_TEST_PLAYBACK_WAV") {
            std::fs::write(output, bytes).unwrap();
        }
        crate::paths::clear_test_overrides();
    }
}
