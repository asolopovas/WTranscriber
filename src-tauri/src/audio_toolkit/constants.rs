pub const WHISPER_SAMPLE_RATE: u32 = 16_000;
pub const FRAME_MS: u32 = 30;
pub const FRAME_SAMPLES: usize = (WHISPER_SAMPLE_RATE * FRAME_MS / 1000) as usize;
