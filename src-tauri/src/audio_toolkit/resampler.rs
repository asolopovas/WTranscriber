use rubato::{FftFixedIn, Resampler};

const RESAMPLER_CHUNK_SIZE: usize = 1024;

pub struct FrameResampler {
    resampler: Option<FftFixedIn<f32>>,
    chunk_in: usize,
    in_buf: Vec<f32>,
    frame_samples: usize,
    pending: Vec<f32>,
}

impl FrameResampler {
    #[must_use]
    pub fn new(in_hz: usize, out_hz: usize, frame_samples: usize) -> Self {
        let chunk_in = RESAMPLER_CHUNK_SIZE;
        let resampler = (in_hz != out_hz).then(|| {
            FftFixedIn::<f32>::new(in_hz, out_hz, chunk_in, 1, 1)
                .expect("failed to create resampler")
        });
        Self {
            resampler,
            chunk_in,
            in_buf: Vec::with_capacity(chunk_in),
            frame_samples,
            pending: Vec::with_capacity(frame_samples),
        }
    }

    pub fn push(&mut self, mut src: &[f32], mut emit: impl FnMut(&[f32])) {
        if self.resampler.is_none() {
            self.emit_frames(src, &mut emit);
            return;
        }
        while !src.is_empty() {
            let space = self.chunk_in - self.in_buf.len();
            let take = space.min(src.len());
            self.in_buf.extend_from_slice(&src[..take]);
            src = &src[take..];
            if self.in_buf.len() == self.chunk_in {
                if let Ok(out) = self
                    .resampler
                    .as_mut()
                    .unwrap()
                    .process(&[&self.in_buf[..]], None)
                {
                    self.emit_frames(&out[0], &mut emit);
                }
                self.in_buf.clear();
            }
        }
    }

    pub fn finish(&mut self, mut emit: impl FnMut(&[f32])) {
        if let Some(ref mut resampler) = self.resampler
            && !self.in_buf.is_empty()
        {
            self.in_buf.resize(self.chunk_in, 0.0);
            if let Ok(out) = resampler.process(&[&self.in_buf[..]], None) {
                self.emit_frames(&out[0], &mut emit);
            }
            self.in_buf.clear();
        }
        if !self.pending.is_empty() {
            self.pending.resize(self.frame_samples, 0.0);
            emit(&self.pending);
            self.pending.clear();
        }
    }

    fn emit_frames(&mut self, mut data: &[f32], emit: &mut impl FnMut(&[f32])) {
        while !data.is_empty() {
            let space = self.frame_samples - self.pending.len();
            let take = space.min(data.len());
            self.pending.extend_from_slice(&data[..take]);
            data = &data[take..];
            if self.pending.len() == self.frame_samples {
                emit(&self.pending);
                self.pending.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn collect(in_hz: usize, out_hz: usize, frame: usize, src: &[f32]) -> Vec<Vec<f32>> {
        let mut r = FrameResampler::new(in_hz, out_hz, frame);
        let mut frames = Vec::new();
        r.push(src, |f| frames.push(f.to_vec()));
        r.finish(|f| frames.push(f.to_vec()));
        frames
    }

    #[test]
    fn passthrough_emits_full_frames_at_matching_rate() {
        let src = vec![0.25_f32; 1024];
        let frames = collect(16_000, 16_000, 256, &src);
        assert_eq!(frames.len(), 4);
        for f in &frames {
            assert_eq!(f.len(), 256);
        }
    }

    #[test]
    fn passthrough_pads_partial_trailing_frame_on_finish() {
        let src = vec![0.5_f32; 300];
        let frames = collect(16_000, 16_000, 256, &src);
        assert_eq!(frames.len(), 2);
        assert_eq!(frames[0].len(), 256);
        assert_eq!(frames[1].len(), 256);
        assert!((frames[1][0] - 0.5).abs() < 1e-6);
        assert!(frames[1][255].abs() < 1e-6);
    }

    #[test]
    fn passthrough_no_emit_for_empty_input() {
        let frames = collect(16_000, 16_000, 256, &[]);
        assert!(frames.is_empty());
    }

    #[test]
    fn resampling_changes_total_frame_count() {
        let src = vec![0.0_f32; 4 * 1024];
        let upsampled = collect(16_000, 32_000, 256, &src);
        let total: usize = upsampled.iter().map(Vec::len).sum();
        assert!(total > src.len());
    }
}
