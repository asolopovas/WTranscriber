#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::{
    audio,
    config::Config,
    diarizer::{self, Segment as DiarSegment},
    engine,
    error::Result,
    transcriber::transcript::{self, Meta, Transcript},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub input: PathBuf,
    pub config: Config,
}

pub async fn run(job: &Job) -> Result<Transcript> {
    let input = job.input.clone();
    let config = job.config.clone();

    tokio::task::spawn_blocking(move || run_blocking(&input, &config))
        .await
        .map_err(|e| crate::error::Error::Transcribe(format!("task: {e}")))?
}

fn run_blocking(input: &Path, config: &Config) -> Result<Transcript> {
    let samples = audio::load_samples(input)?;
    let audio_dur_sec = samples.len() as f64 / f64::from(audio::WHISPER_SAMPLE_RATE);
    let duration_ms = (audio_dur_sec * 1000.0) as u64;

    let mut on_progress = |_pct: f64| {};
    let (segments, detected_language, _rtf) =
        engine::run_whisper(&samples, audio_dur_sec, config, &mut on_progress)?;

    let (diar_segs, diar_name) = if config.diarize {
        run_diarize(input, &samples, audio_dur_sec, config.speakers.unwrap_or(0))
            .map_or((Vec::new(), None), |(s, n)| (s, Some(n)))
    } else {
        (Vec::new(), None)
    };

    Ok(transcript::build(
        &segments,
        &diar_segs,
        Meta {
            model: config.model.clone(),
            language: if detected_language.is_empty() {
                config.language.clone()
            } else {
                detected_language
            },
            duration_ms,
            diarizer: diar_name,
            device: Some(format!("{:?}", config.device).to_lowercase()),
        },
    ))
}

fn run_diarize(
    input: &Path,
    samples: &[f32],
    audio_dur_sec: f64,
    speakers: u32,
) -> Result<(Vec<DiarSegment>, String)> {
    let backend = diarizer::new(speakers)?;
    let wav = ensure_wav_for_diarize(input, samples)?;
    let mut on_progress = |_pct: f64| {};
    let segs = backend.diarize(&wav, speakers, audio_dur_sec, &mut on_progress)?;
    Ok((segs, backend.name()))
}

fn ensure_wav_for_diarize(input: &Path, samples: &[f32]) -> Result<PathBuf> {
    if input.extension().is_some_and(|e| e.eq_ignore_ascii_case("wav")) {
        return Ok(input.to_path_buf());
    }
    let cache_dir = crate::paths::cache_dir()?;
    let key = audio::audio_cache_key(input)?;
    let cached = cache_dir.join(key);
    if cached.exists() {
        return Ok(cached);
    }
    audio::write_pcm16_wav(&cached, samples, audio::WHISPER_SAMPLE_RATE)?;
    Ok(cached)
}
