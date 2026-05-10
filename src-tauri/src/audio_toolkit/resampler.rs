use rubato::audioadapter_buffers::direct::SequentialSlice;
use rubato::{Fft, FixedSync, Indexing, Resampler};

const RESAMPLER_CHUNK_SIZE: usize = 1024;

pub struct FrameResampler {
    resampler: Option<Fft<f32>>,
    chunk_in: usize,
    in_buf: Vec<f32>,
    out_buf: Vec<f32>,
    out_capacity: usize,
    frame_samples: usize,
    pending: Vec<f32>,
}

impl FrameResampler {
    #[must_use]
    pub fn new(in_hz: usize, out_hz: usize, frame_samples: usize) -> Self {
        let chunk_in = RESAMPLER_CHUNK_SIZE;
        let resampler = (in_hz != out_hz).then(|| {
            Fft::<f32>::new(in_hz, out_hz, chunk_in, 1, 1, FixedSync::Input)
                .expect("failed to create resampler")
        });
        let out_capacity = resampler.as_ref().map_or(0, Resampler::output_frames_max);
        Self {
            resampler,
            chunk_in,
            in_buf: Vec::with_capacity(chunk_in),
            out_buf: vec![0.0_f32; out_capacity],
            out_capacity,
            frame_samples,
            pending: Vec::with_capacity(frame_samples),
        }
    }

    pub fn push(&mut self, mut src: &[f32], mut emit: impl FnMut(&[f32])) {
        if self.resampler.is_none() {
            emit_chunked(&mut self.pending, self.frame_samples, src, &mut emit);
            return;
        }
        while !src.is_empty() {
            let space = self.chunk_in - self.in_buf.len();
            let take = space.min(src.len());
            self.in_buf.extend_from_slice(&src[..take]);
            src = &src[take..];
            if self.in_buf.len() == self.chunk_in {
                let n_out = process_chunk(
                    self.resampler.as_mut().expect("resampler present"),
                    &self.in_buf,
                    &mut self.out_buf,
                    self.chunk_in,
                    self.out_capacity,
                    None,
                );
                self.in_buf.clear();
                emit_chunked(
                    &mut self.pending,
                    self.frame_samples,
                    &self.out_buf[..n_out],
                    &mut emit,
                );
            }
        }
    }

    pub fn finish(&mut self, mut emit: impl FnMut(&[f32])) {
        if let Some(ref mut r) = self.resampler
            && !self.in_buf.is_empty()
        {
            let partial_len = self.in_buf.len();
            self.in_buf.resize(self.chunk_in, 0.0);
            let indexing = Indexing {
                input_offset: 0,
                output_offset: 0,
                active_channels_mask: None,
                partial_len: Some(partial_len),
            };
            let n_out = process_chunk(
                r,
                &self.in_buf,
                &mut self.out_buf,
                self.chunk_in,
                self.out_capacity,
                Some(&indexing),
            );
            self.in_buf.clear();
            emit_chunked(
                &mut self.pending,
                self.frame_samples,
                &self.out_buf[..n_out],
                &mut emit,
            );
        }
        if !self.pending.is_empty() {
            self.pending.resize(self.frame_samples, 0.0);
            emit(&self.pending);
            self.pending.clear();
        }
    }
}

fn process_chunk(
    resampler: &mut Fft<f32>,
    in_buf: &[f32],
    out_buf: &mut [f32],
    chunk_in: usize,
    chunk_out_max: usize,
    indexing: Option<&Indexing>,
) -> usize {
    let Ok(input) = SequentialSlice::new(in_buf, 1, chunk_in) else {
        return 0;
    };
    let Ok(mut output) = SequentialSlice::new_mut(out_buf, 1, chunk_out_max) else {
        return 0;
    };
    match resampler.process_into_buffer(&input, &mut output, indexing) {
        Ok((_, n_out)) => n_out,
        Err(_) => 0,
    }
}

fn emit_chunked(
    pending: &mut Vec<f32>,
    frame_samples: usize,
    mut data: &[f32],
    emit: &mut impl FnMut(&[f32]),
) {
    while !data.is_empty() {
        let space = frame_samples - pending.len();
        let take = space.min(data.len());
        pending.extend_from_slice(&data[..take]);
        data = &data[take..];
        if pending.len() == frame_samples {
            emit(pending);
            pending.clear();
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
