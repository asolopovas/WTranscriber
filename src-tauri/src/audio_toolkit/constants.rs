pub use crate::audio::WHISPER_SAMPLE_RATE;

pub const FRAME_MS: u32 = 30;
pub const FRAME_SAMPLES: usize = (WHISPER_SAMPLE_RATE * FRAME_MS / 1000) as usize;
