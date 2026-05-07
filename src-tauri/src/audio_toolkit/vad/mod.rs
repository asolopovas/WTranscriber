pub mod model;
pub mod regions;
mod silero;
mod smoothed;

pub use regions::{Region, RegionStream, RegionStreamConfig};
pub use silero::SileroVad;
pub use smoothed::SmoothedVad;

use crate::error::Result;

pub enum VadFrame<'a> {
    Speech(&'a [f32]),
    Noise,
}

pub trait VoiceActivityDetector: Send {
    fn push_frame<'a>(&'a mut self, frame: &'a [f32]) -> Result<VadFrame<'a>>;
    fn is_voice(&mut self, frame: &[f32]) -> Result<bool> {
        Ok(matches!(self.push_frame(frame)?, VadFrame::Speech(_)))
    }
    fn reset(&mut self) {}
}
