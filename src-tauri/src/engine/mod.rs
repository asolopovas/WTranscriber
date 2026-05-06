mod canary;
mod chunk;
mod nemo_ctc;
mod runtime;
mod sherpa;
mod transducer;
mod whisper;

#[allow(unused_imports)]
pub use chunk::{Chunk, run_chunked, split_chunks};
#[allow(unused_imports)]
pub use runtime::{Provider, provider, threads};
#[allow(unused_imports)]
pub use sherpa::{SherpaResult, find_binary, parse_json, run_cmd};
#[allow(unused_imports)]
pub use whisper::run as run_whisper;

use crate::{
    config::{Config, Engine},
    error::Result,
    transcriber::Segment,
};

pub fn run(
    samples: &[f32],
    audio_dur_sec: f64,
    config: &Config,
    on_progress: &mut dyn FnMut(f64),
) -> Result<(Vec<Segment>, String, f64)> {
    match config.engine {
        Engine::WhisperOnnx => whisper::run(samples, audio_dur_sec, config, on_progress),
        Engine::Zipformer => transducer::run(
            transducer::Kind::Zipformer,
            samples,
            audio_dur_sec,
            config,
            on_progress,
        ),
        Engine::Parakeet => transducer::run(
            transducer::Kind::Parakeet,
            samples,
            audio_dur_sec,
            config,
            on_progress,
        ),
        Engine::Canary => canary::run(samples, audio_dur_sec, config, on_progress),
        Engine::NemoCtc => nemo_ctc::run(samples, audio_dur_sec, config, on_progress),
    }
}
