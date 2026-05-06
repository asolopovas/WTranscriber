#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use std::path::{Path, PathBuf};

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::{
    audio,
    config::Config,
    diarizer::{self, Segment as DiarSegment},
    engine,
    error::Result,
    transcriber::{
        cache::{self, Entry as CacheEntry, build_key_params, compute_key},
        dedup,
        transcript::{self, Meta, Segment, Transcript},
    },
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
    let speakers = config.speakers.unwrap_or(0);
    let key_params = build_key_params(
        input,
        &config.model,
        &config.language,
        speakers,
        !config.diarize,
    )?;
    let key = compute_key(&key_params);

    if let Some(cached) = cache::load(&key)? {
        return Ok(cached);
    }

    let samples = audio::load_samples(input)?;
    let audio_dur_sec = samples.len() as f64 / f64::from(audio::WHISPER_SAMPLE_RATE);
    let duration_ms = (audio_dur_sec * 1000.0) as u64;

    let mut on_progress = |_pct: f64| {};
    let (mut segments, detected_language, _rtf) =
        engine::run(&samples, audio_dur_sec, config, &mut on_progress)?;
    apply_dedup(&mut segments);

    let (diar_segs, diar_name) = if config.diarize {
        run_diarize(input, &samples, audio_dur_sec, speakers)
            .map_or((Vec::new(), None), |(s, n)| (s, Some(n)))
    } else {
        (Vec::new(), None)
    };

    let language = if detected_language.is_empty() {
        config.language.clone()
    } else {
        detected_language
    };

    let transcript = transcript::build(
        &segments,
        &diar_segs,
        Meta {
            model: config.model.clone(),
            language: language.clone(),
            duration_ms,
            diarizer: diar_name,
            device: Some(format!("{:?}", config.device).to_lowercase()),
        },
    );

    let entry = CacheEntry {
        key,
        source_path: key_params.source_path.clone(),
        source_name: key_params
            .source_path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        model: config.model.clone(),
        language,
        speakers,
        no_diarize: !config.diarize,
        utterances: transcript.utterances.len(),
        duration_ms,
        created_at: Utc::now(),
        size_bytes: 0,
    };
    cache::store(entry, &transcript)?;
    Ok(transcript)
}

fn apply_dedup(segments: &mut Vec<Segment>) {
    for seg in segments.iter_mut() {
        if seg.tokens.len() >= 2 {
            let collapsed = dedup::collapse_repeats(&seg.tokens);
            if collapsed.len() != seg.tokens.len() {
                seg.tokens = collapsed;
                rebuild_from_tokens(seg);
            }
        } else if !seg.text.trim().is_empty() {
            seg.text = dedup::collapse_in_text(seg.text.trim());
        }
    }
    segments.retain(|s| !s.tokens.is_empty() || !s.text.trim().is_empty());
}

fn rebuild_from_tokens(seg: &mut Segment) {
    if seg.tokens.is_empty() {
        seg.text.clear();
        return;
    }
    seg.text = seg
        .tokens
        .iter()
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    seg.start_ms = seg.tokens.first().unwrap().start_ms;
    seg.end_ms = seg.tokens.last().unwrap().end_ms;
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
    if input
        .extension()
        .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
    {
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
