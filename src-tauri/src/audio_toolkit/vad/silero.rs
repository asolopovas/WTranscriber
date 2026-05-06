use std::path::Path;

use vad_rs::Vad;

use super::{VadFrame, VoiceActivityDetector};
use crate::{
    audio_toolkit::constants::{FRAME_SAMPLES, WHISPER_SAMPLE_RATE},
    error::{Error, Result},
};

pub struct SileroVad {
    engine: Vad,
    threshold: f32,
}

impl SileroVad {
    pub fn new<P: AsRef<Path>>(model_path: P, threshold: f32) -> Result<Self> {
        if !(0.0..=1.0).contains(&threshold) {
            return Err(Error::Config(
                "vad threshold must be between 0.0 and 1.0".into(),
            ));
        }
        let engine = Vad::new(&model_path, WHISPER_SAMPLE_RATE as usize)
            .map_err(|e| Error::Config(format!("silero vad: {e}")))?;
        Ok(Self { engine, threshold })
    }
}

impl VoiceActivityDetector for SileroVad {
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>> {
        if frame.len() != FRAME_SAMPLES {
            return Err(Error::Transcribe(format!(
                "vad frame size mismatch: expected {FRAME_SAMPLES}, got {}",
                frame.len()
            )));
        }
        let result = self
            .engine
            .compute(frame)
            .map_err(|e| Error::Transcribe(format!("silero compute: {e}")))?;
        if result.prob > self.threshold {
            Ok(VadFrame::Speech(frame))
        } else {
            Ok(VadFrame::Noise)
        }
    }
}
