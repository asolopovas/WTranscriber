#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    clippy::cast_possible_wrap,
    clippy::float_cmp,
    dead_code
)]

use std::path::Path;

use crate::{
    audio::{WHISPER_SAMPLE_RATE, write_pcm16_wav},
    error::Result,
    transcriber::{Segment, Token},
};

const DEFAULT_CHUNK_SEC: f64 = 600.0;
const BOUNDARY_SEARCH_SEC: f64 = 2.0;
const BOUNDARY_WINDOW_SEC: f64 = 0.2;
const BOUNDARY_MIN_ADVANCE_SEC: f64 = 0.5;

#[derive(Debug, Clone)]
pub struct Chunk<'a> {
    pub start_sec: f64,
    pub end_sec: f64,
    pub samples: &'a [f32],
}

const fn samples_at(sec: f64) -> usize {
    (sec * WHISPER_SAMPLE_RATE as f64) as usize
}

fn snap_boundary(samples: &[f32], target: usize) -> usize {
    let window = samples_at(BOUNDARY_SEARCH_SEC);
    let lo = target
        .saturating_sub(window)
        .max(samples_at(BOUNDARY_MIN_ADVANCE_SEC));
    let hi = (target + window).min(samples.len());
    if lo >= hi {
        return target;
    }
    let step = samples_at(BOUNDARY_WINDOW_SEC).max(1);
    let stride = (step / 2).max(1);
    let mut best_pos = target;
    let mut best_energy: Option<f64> = None;
    let mut pos = lo;
    while pos + step <= hi {
        let energy: f64 = samples[pos..pos + step]
            .iter()
            .map(|v| f64::from(*v) * f64::from(*v))
            .sum();
        if best_energy.is_none_or(|b| energy < b) {
            best_energy = Some(energy);
            best_pos = pos + step / 2;
        }
        pos += stride;
    }
    best_pos
}

pub fn split_chunks(samples: &[f32], sec: f64) -> Vec<Chunk<'_>> {
    let sec = if sec <= 0.0 { DEFAULT_CHUNK_SEC } else { sec };
    let stride = samples_at(sec).max(1);
    let mut out = Vec::with_capacity(samples.len() / stride + 1);
    let mut off = 0usize;
    while off < samples.len() {
        let mut end = (off + stride).min(samples.len());
        if end < samples.len() {
            end = snap_boundary(samples, end).max(off + 1);
        }
        out.push(Chunk {
            start_sec: off as f64 / f64::from(WHISPER_SAMPLE_RATE),
            end_sec: end as f64 / f64::from(WHISPER_SAMPLE_RATE),
            samples: &samples[off..end],
        });
        if end == samples.len() {
            break;
        }
        off = end;
    }
    out
}

fn shift(segs: &mut [Segment], start_sec: f64, end_sec: f64) {
    let off_ms = ms(start_sec);
    let limit_ms = ms(end_sec);
    let clamp = |t: u64| (t + off_ms).clamp(off_ms, limit_ms);
    for s in segs.iter_mut() {
        s.start_ms = clamp(s.start_ms);
        s.end_ms = clamp(s.end_ms);
        for tok in &mut s.tokens {
            tok.start_ms = clamp(tok.start_ms);
            tok.end_ms = clamp(tok.end_ms);
        }
    }
}

pub fn segments_from_sherpa(r: &super::SherpaResult, chunk_dur_sec: f64) -> Vec<Segment> {
    if let Some(seg) = coalesce_segment(&r.tokens, r.timestamps.iter().copied(), chunk_dur_sec) {
        return vec![seg];
    }
    let text = r.text.trim();
    if text.is_empty() {
        return Vec::new();
    }
    vec![Segment {
        text: text.to_owned(),
        start_ms: 0,
        end_ms: ms(chunk_dur_sec),
        tokens: Vec::new(),
    }]
}

struct Word {
    text: String,
    start: f64,
    end: f64,
}

pub fn coalesce_segment<I>(tokens: &[String], timestamps: I, audio_dur_sec: f64) -> Option<Segment>
where
    I: IntoIterator<Item = f64>,
{
    let stamps: Vec<f64> = timestamps.into_iter().collect();
    if tokens.is_empty() || tokens.len() != stamps.len() {
        return None;
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
                start: stamps[i],
                end: 0.0,
            });
        } else {
            words.last_mut().unwrap().text.push_str(piece);
        }
    }
    if words.is_empty() {
        return None;
    }
    for i in 0..words.len() {
        words[i].end = if i + 1 < words.len() {
            words[i + 1].start
        } else {
            audio_dur_sec
        };
    }
    let parts: Vec<&str> = words.iter().map(|w| w.text.as_str()).collect();
    let toks = words
        .iter()
        .map(|w| Token {
            text: w.text.clone(),
            start_ms: ms(w.start),
            end_ms: ms(w.end),
            confidence: 0.0,
        })
        .collect();
    Some(Segment {
        text: parts.join(" "),
        start_ms: ms(words.first().unwrap().start),
        end_ms: ms(words.last().unwrap().end),
        tokens: toks,
    })
}

pub const fn ms(sec: f64) -> u64 {
    (sec * 1000.0) as u64
}

pub trait ChunkProcessor {
    fn process(&mut self, wav: &Path, chunk_dur_sec: f64) -> Result<Vec<Segment>>;
    fn is_cancelled(&self) -> bool {
        false
    }
}

pub fn run_chunked<P: ChunkProcessor>(
    samples: &[f32],
    audio_dur_sec: f64,
    mut processor: P,
    on_progress: &mut dyn FnMut(f64),
) -> Result<(Vec<Segment>, f64)> {
    let chunks = split_chunks(samples, DEFAULT_CHUNK_SEC);
    if chunks.is_empty() {
        return Ok((Vec::new(), 0.0));
    }

    let mut merged = Vec::new();
    let mut total_audio = 0.0;
    let mut total_elapsed = 0.0;

    for (i, ch) in chunks.iter().enumerate() {
        if processor.is_cancelled() {
            return Err(crate::error::Error::Cancelled);
        }
        let dir = tempfile::tempdir()?;
        let wav = dir.path().join("input.wav");
        write_pcm16_wav(&wav, ch.samples, WHISPER_SAMPLE_RATE)?;

        let chunk_dur = ch.end_sec - ch.start_sec;
        let start = std::time::Instant::now();
        let mut segs = processor.process(&wav, chunk_dur)?;
        if processor.is_cancelled() {
            return Err(crate::error::Error::Cancelled);
        }
        let elapsed = start.elapsed().as_secs_f64();

        shift(&mut segs, ch.start_sec, ch.end_sec);
        merged.extend(segs);

        total_audio += chunk_dur;
        total_elapsed += elapsed;

        let pct = ((ch.end_sec / audio_dur_sec) * 100.0).min(100.0);
        on_progress(pct);
        let _ = i;
    }

    let rtf = if total_elapsed > 0.0 {
        total_audio / total_elapsed
    } else {
        0.0
    };
    Ok((merged, rtf))
}

pub fn run_single<P: ChunkProcessor>(
    samples: &[f32],
    audio_dur_sec: f64,
    mut processor: P,
    on_progress: &mut dyn FnMut(f64),
) -> Result<(Vec<Segment>, f64)> {
    if samples.is_empty() {
        return Ok((Vec::new(), 0.0));
    }
    if processor.is_cancelled() {
        return Err(crate::error::Error::Cancelled);
    }
    let dir = tempfile::tempdir()?;
    let wav = dir.path().join("input.wav");
    write_pcm16_wav(&wav, samples, WHISPER_SAMPLE_RATE)?;
    let start = std::time::Instant::now();
    let segs = processor.process(&wav, audio_dur_sec)?;
    if processor.is_cancelled() {
        return Err(crate::error::Error::Cancelled);
    }
    let elapsed = start.elapsed().as_secs_f64();
    on_progress(100.0);
    let rtf = if elapsed > 0.0 {
        audio_dur_sec / elapsed
    } else {
        0.0
    };
    Ok((segs, rtf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_into_expected_count() {
        let samples = vec![0.0f32; (WHISPER_SAMPLE_RATE * 75) as usize];
        let chunks = split_chunks(&samples, 30.0);
        assert!((2..=3).contains(&chunks.len()));
        assert_eq!(chunks.first().unwrap().start_sec, 0.0);
        assert!((chunks.last().unwrap().end_sec - 75.0).abs() < 0.1);
    }

    #[test]
    fn empty_samples_yields_no_chunks() {
        assert!(split_chunks(&[], 30.0).is_empty());
    }

    #[test]
    fn nonpositive_chunk_size_uses_default() {
        let samples = vec![0.0f32; (WHISPER_SAMPLE_RATE * 5) as usize];
        let chunks = split_chunks(&samples, 0.0);
        assert_eq!(chunks.len(), 1);
    }

    #[test]
    fn ms_converts_seconds_to_milliseconds() {
        assert_eq!(ms(0.0), 0);
        assert_eq!(ms(1.0), 1_000);
        assert_eq!(ms(1.234), 1_234);
    }

    #[test]
    fn coalesce_segment_groups_subword_tokens() {
        let tokens = vec![" hello".into(), " world".into()];
        let stamps = vec![0.0, 0.5];
        let seg = coalesce_segment(&tokens, stamps, 1.0).unwrap();
        assert_eq!(seg.text, "hello world");
        assert_eq!(seg.tokens.len(), 2);
        assert_eq!(seg.tokens[0].start_ms, 0);
        assert_eq!(seg.tokens[0].end_ms, 500);
        assert_eq!(seg.tokens[1].end_ms, 1_000);
    }

    #[test]
    fn coalesce_segment_merges_continuation_tokens() {
        let tokens = vec![" hel".into(), "lo".into(), " world".into()];
        let stamps = vec![0.0, 0.2, 0.5];
        let seg = coalesce_segment(&tokens, stamps, 1.0).unwrap();
        assert_eq!(seg.text, "hello world");
        assert_eq!(seg.tokens.len(), 2);
    }

    #[test]
    fn coalesce_segment_rejects_mismatched_lengths() {
        assert!(coalesce_segment(&["a".into()], [0.0, 0.1], 1.0).is_none());
        assert!(coalesce_segment(&[], std::iter::empty::<f64>(), 1.0).is_none());
    }
}
