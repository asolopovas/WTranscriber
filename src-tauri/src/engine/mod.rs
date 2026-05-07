mod canary;
mod chunk;
mod nemo_ctc;
mod recognizer;
mod runtime;
mod sherpa;
mod transducer;
mod whisper;

#[allow(unused_imports)]
pub use chunk::{Chunk, run_chunked, run_single, split_chunks};
#[allow(unused_imports)]
pub use runtime::{Provider, provider, threads};
#[allow(unused_imports)]
pub use sherpa::{SherpaResult, find_binary, parse_json, run_cmd};
#[allow(unused_imports)]
pub use whisper::run as run_whisper;

use crate::{
    config::{Config, Engine},
    error::{Error, Result},
    transcriber::{Segment, Token},
};

pub fn run(
    samples: &[f32],
    audio_dur_sec: f64,
    config: &Config,
    on_progress: &mut dyn FnMut(f64),
    cancelled: &dyn Fn() -> bool,
    on_chunk: &mut dyn FnMut(Vec<Segment>, f64),
) -> Result<(String, f64)> {
    if use_in_process(config) {
        return run_in_process(samples, audio_dur_sec, config, cancelled, on_chunk);
    }
    let (segs, lang, rtf) = match config.engine {
        Engine::WhisperOnnx => {
            whisper::run(samples, audio_dur_sec, config, on_progress, cancelled)?
        }
        Engine::Zipformer => transducer::run(
            transducer::Kind::Zipformer,
            samples,
            audio_dur_sec,
            config,
            on_progress,
            cancelled,
        )?,
        Engine::Parakeet => transducer::run(
            transducer::Kind::Parakeet,
            samples,
            audio_dur_sec,
            config,
            on_progress,
            cancelled,
        )?,
        Engine::Canary => canary::run(samples, audio_dur_sec, config, on_progress, cancelled)?,
        Engine::NemoCtc => nemo_ctc::run(samples, audio_dur_sec, config, on_progress, cancelled)?,
    };
    on_chunk(segs, audio_dur_sec);
    Ok((lang, rtf))
}

fn use_in_process(config: &Config) -> bool {
    if std::env::var("WT_USE_SUBPROCESS")
        .ok()
        .is_some_and(|v| v == "1")
    {
        return false;
    }
    matches!(
        config.engine,
        Engine::WhisperOnnx | Engine::Zipformer | Engine::Parakeet | Engine::NemoCtc
    )
}

const WHISPER_MAX_CHUNK_SEC: f64 = 15.0;

#[allow(clippy::significant_drop_tightening)]
fn run_in_process(
    samples: &[f32],
    audio_dur_sec: f64,
    config: &Config,
    cancelled: &dyn Fn() -> bool,
    on_chunk: &mut dyn FnMut(Vec<Segment>, f64),
) -> Result<(String, f64)> {
    if cancelled() {
        return Err(Error::Cancelled);
    }
    recognizer::ensure(config)?;
    let mut guard = recognizer::lock();
    let loaded = guard
        .as_mut()
        .ok_or_else(|| Error::Transcribe("recognizer cache empty after ensure".into()))?;
    let sample_rate = i32::try_from(crate::audio::WHISPER_SAMPLE_RATE).unwrap_or(16_000);
    let chunk_sec = match config.engine {
        Engine::WhisperOnnx => WHISPER_MAX_CHUNK_SEC,
        _ => audio_dur_sec.max(1.0),
    };
    let chunks = chunk::split_chunks(samples, chunk_sec);

    let t0 = std::time::Instant::now();
    for ch in &chunks {
        if cancelled() {
            return Err(Error::Cancelled);
        }
        let stream = loaded.recognizer.create_stream();
        stream.accept_waveform(sample_rate, ch.samples);
        loaded.recognizer.decode(&stream);
        let chunk_dur = ch.end_sec - ch.start_sec;
        let result = stream
            .get_result()
            .ok_or_else(|| Error::Transcribe("empty result from recognizer".into()))?;
        let mut chunk_segs: Vec<Segment> = Vec::new();
        for mut seg in build_segments(&result, chunk_dur) {
            let offset_ms = f64_ms(ch.start_sec);
            seg.start_ms = seg.start_ms.saturating_add(offset_ms);
            seg.end_ms = seg.end_ms.saturating_add(offset_ms);
            for tok in &mut seg.tokens {
                tok.start_ms = tok.start_ms.saturating_add(offset_ms);
                tok.end_ms = tok.end_ms.saturating_add(offset_ms);
            }
            chunk_segs.push(seg);
        }
        on_chunk(chunk_segs, ch.end_sec);
    }
    let elapsed = t0.elapsed().as_secs_f64();
    let rtf = if elapsed > 0.0 {
        audio_dur_sec / elapsed
    } else {
        0.0
    };
    let detected = if config.language == "auto" || config.language.is_empty() {
        String::new()
    } else {
        config.language.clone()
    };
    Ok((detected, rtf))
}

fn build_segments(
    result: &sherpa_onnx::OfflineRecognizerResult,
    audio_dur_sec: f64,
) -> Vec<Segment> {
    let text = result.text.trim();
    if text.is_empty() && result.tokens.is_empty() {
        return Vec::new();
    }
    let timestamps = result.timestamps.as_ref();
    if let Some(ts) = timestamps
        && !result.tokens.is_empty()
        && ts.len() == result.tokens.len()
    {
        return vec![coalesce_word_segment(&result.tokens, ts, audio_dur_sec)];
    }
    vec![Segment {
        text: text.to_owned(),
        start_ms: 0,
        end_ms: f64_ms(audio_dur_sec),
        tokens: Vec::new(),
    }]
}

fn coalesce_word_segment(tokens: &[String], timestamps: &[f32], audio_dur_sec: f64) -> Segment {
    struct Word {
        text: String,
        start: f64,
        end: f64,
    }
    let mut words: Vec<Word> = Vec::with_capacity(tokens.len() / 2 + 1);
    for (i, tok) in tokens.iter().enumerate() {
        if tok.is_empty() {
            continue;
        }
        let is_boundary = i == 0 || tok.starts_with(' ');
        let piece = tok.strip_prefix(' ').unwrap_or(tok);
        if is_boundary || words.is_empty() {
            words.push(Word {
                text: piece.to_owned(),
                start: f64::from(timestamps[i]),
                end: 0.0,
            });
        } else {
            words.last_mut().unwrap().text.push_str(piece);
        }
    }
    if words.is_empty() {
        return Segment {
            text: String::new(),
            start_ms: 0,
            end_ms: f64_ms(audio_dur_sec),
            tokens: Vec::new(),
        };
    }
    for i in 0..words.len() {
        words[i].end = if i + 1 < words.len() {
            words[i + 1].start
        } else {
            audio_dur_sec
        };
    }
    let parts: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
    let toks: Vec<Token> = words
        .iter()
        .map(|w| Token {
            text: w.text.clone(),
            start_ms: f64_ms(w.start),
            end_ms: f64_ms(w.end),
            confidence: 0.0,
        })
        .collect();
    Segment {
        text: parts.join(" "),
        start_ms: f64_ms(words.first().unwrap().start),
        end_ms: f64_ms(words.last().unwrap().end),
        tokens: toks,
    }
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn f64_ms(sec: f64) -> u64 {
    (sec * 1000.0) as u64
}
