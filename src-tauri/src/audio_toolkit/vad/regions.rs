#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss
)]

use std::{path::Path, sync::Arc, sync::atomic::AtomicBool};

use crate::{
    audio_toolkit::{
        constants::{FRAME_MS, FRAME_SAMPLES, WHISPER_SAMPLE_RATE},
        resampler::FrameResampler,
        stream::ffmpeg_stream,
        vad::{SileroVad, SmoothedVad, VadFrame, VoiceActivityDetector},
    },
    error::Result,
};

#[derive(Debug, Clone)]
pub struct Region {
    pub start_sec: f64,
    pub end_sec: f64,
    pub samples: Vec<f32>,
}

#[derive(Debug, Clone, Copy)]
pub struct RegionStreamConfig {
    pub max_region_sec: f64,
    pub min_region_sec: f64,
    pub prefill_frames: usize,
    pub hangover_frames: usize,
    pub onset_frames: usize,
    pub threshold: f32,
}

impl Default for RegionStreamConfig {
    fn default() -> Self {
        Self {
            max_region_sec: 25.0,
            min_region_sec: 0.4,
            prefill_frames: 8,
            hangover_frames: 24,
            onset_frames: 3,
            threshold: 0.5,
        }
    }
}

pub struct RegionStream;

impl RegionStream {
    pub fn run<F>(
        input: &Path,
        trim_start_ms: u64,
        trim_end_ms: Option<u64>,
        vad_model: &Path,
        cfg: RegionStreamConfig,
        cancel: Arc<AtomicBool>,
        mut on_region: F,
    ) -> Result<f64>
    where
        F: FnMut(Region) -> Result<()>,
    {
        let inner = SileroVad::new(vad_model, cfg.threshold)?;
        let mut vad = SmoothedVad::new(
            Box::new(inner),
            cfg.prefill_frames,
            cfg.hangover_frames,
            cfg.onset_frames,
        );

        let mut src = ffmpeg_stream(input, trim_start_ms, trim_end_ms, cancel)?;
        let mut resampler = FrameResampler::new(
            WHISPER_SAMPLE_RATE as usize,
            WHISPER_SAMPLE_RATE as usize,
            FRAME_SAMPLES,
        );

        let mut buf = vec![0.0_f32; FRAME_SAMPLES];
        let mut frame_index: u64 = 0;
        let mut region_buf: Vec<f32> = Vec::new();
        let mut region_start_ms: Option<u64> = None;
        let mut last_speech_frame: u64 = 0;
        let max_region_samples = (cfg.max_region_sec * f64::from(WHISPER_SAMPLE_RATE)) as usize;
        let min_region_samples = (cfg.min_region_sec * f64::from(WHISPER_SAMPLE_RATE)) as usize;
        let trim_offset_sec = trim_start_ms as f64 / 1000.0;

        let mut emit = |region_buf: &mut Vec<f32>,
                        region_start_ms: &mut Option<u64>,
                        last_speech_frame: u64|
         -> Result<()> {
            if region_buf.len() < min_region_samples {
                region_buf.clear();
                *region_start_ms = None;
                return Ok(());
            }
            let start_sec = region_start_ms.map_or(0.0, |ms| ms as f64 / 1000.0) + trim_offset_sec;
            let end_ms = (last_speech_frame + 1) * u64::from(FRAME_MS);
            let end_sec = end_ms as f64 / 1000.0 + trim_offset_sec;
            let samples = std::mem::take(region_buf);
            *region_start_ms = None;
            on_region(Region {
                start_sec,
                end_sec,
                samples,
            })
        };

        loop {
            let read = src.read_into(&mut buf)?;
            if read == 0 {
                break;
            }
            if read < buf.len() {
                for v in &mut buf[read..] {
                    *v = 0.0;
                }
            }
            let frame_input = buf[..].to_vec();
            let mut emitted_frames: Vec<Vec<f32>> = Vec::new();
            resampler.push(&frame_input, |out| emitted_frames.push(out.to_vec()));
            for frame in emitted_frames {
                let is_speech = match vad.push_frame(&frame)? {
                    VadFrame::Speech(prefilled) => {
                        if region_start_ms.is_none() {
                            let start_frame = frame_index.saturating_sub(
                                (prefilled.len() / FRAME_SAMPLES).saturating_sub(1) as u64,
                            );
                            region_start_ms = Some(start_frame * u64::from(FRAME_MS));
                        }
                        region_buf.extend_from_slice(prefilled);
                        last_speech_frame = frame_index;
                        true
                    }
                    VadFrame::Noise => {
                        if region_start_ms.is_some() {
                            emit(&mut region_buf, &mut region_start_ms, last_speech_frame)?;
                        }
                        false
                    }
                };
                if is_speech && region_buf.len() >= max_region_samples {
                    emit(&mut region_buf, &mut region_start_ms, last_speech_frame)?;
                }
                frame_index += 1;
            }
        }

        let mut tail: Vec<Vec<f32>> = Vec::new();
        resampler.finish(|out| tail.push(out.to_vec()));
        for frame in tail {
            if let VadFrame::Speech(prefilled) = vad.push_frame(&frame)? {
                if region_start_ms.is_none() {
                    region_start_ms = Some(frame_index * u64::from(FRAME_MS));
                }
                region_buf.extend_from_slice(prefilled);
                last_speech_frame = frame_index;
            }
            frame_index += 1;
        }
        if region_start_ms.is_some() {
            emit(&mut region_buf, &mut region_start_ms, last_speech_frame)?;
        }

        let total_sec = (frame_index * u64::from(FRAME_MS)) as f64 / 1000.0;
        Ok(total_sec + trim_offset_sec)
    }
}
