pub mod model;
pub mod regions;
mod silero;

pub use regions::Region;
pub use silero::SileroVad;

use crate::error::Result;

pub enum VadFrame<'a> {
    Speech(&'a [f32]),
    Noise,
}

pub trait VoiceActivityDetector: Send {
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>>;
}
