#![allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]

use std::path::Path;

use hound::{SampleFormat, WavSpec, WavWriter};

use crate::error::{Error, Result};

pub fn write_pcm16_wav(path: &Path, samples: &[f32], sample_rate: u32) -> Result<()> {
    let spec = WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: SampleFormat::Int,
    };
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut w = WavWriter::create(path, spec).map_err(|e| map_hound(&e))?;
    for s in samples {
        let v = (s.clamp(-1.0, 1.0) * f32::from(i16::MAX)) as i16;
        w.write_sample(v).map_err(|e| map_hound(&e))?;
    }
    w.finalize().map_err(|e| map_hound(&e))?;
    Ok(())
}

fn map_hound(e: &hound::Error) -> Error {
    Error::Transcribe(format!("wav: {e}"))
}
