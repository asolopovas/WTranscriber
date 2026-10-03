#![allow(clippy::cast_precision_loss)]

use std::{io::Write, path::Path};

use super::{Backend, Segment};
use crate::{
    audio,
    error::Result,
    progress::{Phase, Sink},
};

pub fn run(
    backend: &dyn Backend,
    input: &Path,
    trim: &audio::AudioMeta,
    speakers: u32,
    sink: &dyn Sink,
) -> Result<Vec<Segment>> {
    if sink.is_cancelled() {
        return Err(crate::error::Error::Cancelled);
    }
    let bytes = audio::playback_segment(
        input,
        trim.trim_start_ms,
        trim.trim_end_ms.unwrap_or(u64::MAX),
    )?;
    let duration = (bytes.len().saturating_sub(44)) as f64 / 32_000.0;
    let mut wav = tempfile::Builder::new()
        .suffix(".wav")
        .tempfile_in(crate::paths::cache_dir()?)?;
    wav.write_all(&bytes)?;
    wav.flush()?;
    drop(bytes);
    if sink.is_cancelled() {
        return Err(crate::error::Error::Cancelled);
    }
    let mut segments = backend.diarize(
        wav.path(),
        speakers,
        duration,
        &|| sink.is_cancelled(),
        &mut |pct| sink.report_pct(Phase::Diarizing, pct),
    )?;
    let offset = trim.trim_start_ms as f64 / 1000.0;
    segments.retain(|s| {
        s.start_sec.is_finite()
            && s.end_sec.is_finite()
            && s.end_sec > 0.0
            && s.start_sec < duration
            && s.end_sec > s.start_sec
    });
    for segment in &mut segments {
        segment.start_sec = segment.start_sec.clamp(0.0, duration) + offset;
        segment.end_sec = segment.end_sec.clamp(0.0, duration) + offset;
    }
    Ok(segments)
}

#[cfg(test)]
mod tests {
    use super::*;
    struct Probe;
    impl Backend for Probe {
        fn name(&self) -> String {
            "probe".into()
        }
        fn diarize(
            &self,
            wav: &Path,
            _: u32,
            duration: f64,
            _: &dyn Fn() -> bool,
            _: super::super::Progress<'_>,
        ) -> Result<Vec<Segment>> {
            let samples = audio::read_pcm16_wav(wav)?;
            assert_eq!(samples.len(), 16_000);
            assert!(samples.iter().all(|v| *v > 0.49 && *v < 0.51));
            assert_eq!(duration, 1.0);
            Ok(vec![Segment {
                speaker: 7,
                start_sec: -0.2,
                end_sec: 1.2,
            }])
        }
    }
    #[test]
    fn sends_only_trimmed_samples_and_restores_original_time() {
        let _lock = crate::paths::PATHS_TEST_LOCK.lock().unwrap();
        let dir = tempfile::tempdir().unwrap();
        crate::paths::init(dir.path().into(), dir.path().into(), dir.path().into());
        let source = dir.path().join("source.wav");
        let mut samples = vec![-0.5; 48_000];
        samples[16_000..32_000].fill(0.5);
        for (frames, end) in [(48_000, Some(2000)), (32_000, None), (32_000, Some(4000))] {
            audio::write_pcm16_wav(&source, &samples[..frames], 16_000).unwrap();
            let trim = audio::AudioMeta {
                trim_start_ms: 1000,
                trim_end_ms: end,
                duration_ms: Some(3000),
            };
            let segments = run(&Probe, &source, &trim, 0, &crate::progress::NoopSink).unwrap();
            assert_eq!(
                segments,
                vec![Segment {
                    speaker: 7,
                    start_sec: 1.0,
                    end_sec: 2.0
                }]
            );
        }
        crate::paths::clear_test_overrides();
    }
}
