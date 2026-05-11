#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use std::{
    io::BufReader,
    path::Path,
    process::{Child, Stdio},
    sync::Arc,
    sync::atomic::{AtomicBool, Ordering},
};

use byteorder::{LittleEndian, ReadBytesExt};

use crate::{
    audio::find_ffmpeg,
    audio_toolkit::constants::WHISPER_SAMPLE_RATE,
    error::{Error, Result},
    process::quiet_command,
};

enum Backend {
    Ffmpeg {
        child: Option<Child>,
        reader: BufReader<std::process::ChildStdout>,
    },
    Memory {
        samples: Vec<f32>,
        pos: usize,
    },
}

pub struct StreamSource {
    backend: Backend,
    cancel: Arc<AtomicBool>,
    finished: bool,
}

impl StreamSource {
    pub const fn from_ffmpeg(
        reader: BufReader<std::process::ChildStdout>,
        child: Child,
        cancel: Arc<AtomicBool>,
    ) -> Self {
        Self {
            backend: Backend::Ffmpeg {
                child: Some(child),
                reader,
            },
            cancel,
            finished: false,
        }
    }

    pub const fn from_samples(samples: Vec<f32>, cancel: Arc<AtomicBool>) -> Self {
        Self {
            backend: Backend::Memory { samples, pos: 0 },
            cancel,
            finished: false,
        }
    }

    pub fn read_into(&mut self, buf: &mut [f32]) -> Result<usize> {
        if self.finished {
            return Ok(0);
        }
        if self.cancel.load(Ordering::SeqCst) {
            self.kill_child();
            return Err(Error::Cancelled);
        }
        match &mut self.backend {
            Backend::Memory { samples, pos } => {
                let remaining = samples.len() - *pos;
                let n = remaining.min(buf.len());
                if n == 0 {
                    self.finished = true;
                    return Ok(0);
                }
                buf[..n].copy_from_slice(&samples[*pos..*pos + n]);
                *pos += n;
                Ok(n)
            }
            Backend::Ffmpeg { reader, .. } => {
                let mut filled = 0;
                while filled < buf.len() {
                    if self.cancel.load(Ordering::SeqCst) {
                        self.kill_child();
                        return Err(Error::Cancelled);
                    }
                    match reader.read_i16::<LittleEndian>() {
                        Ok(s) => {
                            buf[filled] = f32::from(s) / f32::from(i16::MAX);
                            filled += 1;
                        }
                        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                            self.finished = true;
                            break;
                        }
                        Err(e) => {
                            self.kill_child();
                            return Err(Error::Transcribe(format!("stream read: {e}")));
                        }
                    }
                }
                Ok(filled)
            }
        }
    }

    fn kill_child(&mut self) {
        if let Backend::Ffmpeg { child, .. } = &mut self.backend
            && let Some(mut c) = child.take()
        {
            let _ = c.kill();
            let _ = c.wait();
        }
    }
}

impl Drop for StreamSource {
    fn drop(&mut self) {
        if let Backend::Ffmpeg { child, .. } = &mut self.backend
            && let Some(mut c) = child.take()
        {
            let _ = c.wait();
        }
    }
}

pub fn ffmpeg_stream(
    input: &Path,
    start_ms: u64,
    end_ms: Option<u64>,
    cancel: Arc<AtomicBool>,
) -> Result<StreamSource> {
    let Some(ffmpeg) = find_ffmpeg() else {
        return symphonia_stream(input, start_ms, end_ms, cancel);
    };
    let mut cmd = quiet_command(ffmpeg.as_os_str());
    if start_ms > 0 {
        cmd.arg("-ss").arg(format_ms(start_ms));
    }
    cmd.arg("-i").arg(input);
    if let Some(end) = end_ms {
        if end > start_ms {
            cmd.arg("-t").arg(format_ms(end - start_ms));
        }
    }
    cmd.args([
        "-vn",
        "-ac",
        "1",
        "-ar",
        &WHISPER_SAMPLE_RATE.to_string(),
        "-f",
        "s16le",
        "-loglevel",
        "error",
        "-",
    ]);
    cmd.stdout(Stdio::piped()).stderr(Stdio::null());
    let mut child = cmd
        .spawn()
        .map_err(|e| Error::Transcribe(format!("ffmpeg spawn: {e}")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::Transcribe("ffmpeg has no stdout".into()))?;
    Ok(StreamSource::from_ffmpeg(
        BufReader::new(stdout),
        child,
        cancel,
    ))
}

fn symphonia_stream(
    input: &Path,
    start_ms: u64,
    end_ms: Option<u64>,
    cancel: Arc<AtomicBool>,
) -> Result<StreamSource> {
    let samples = crate::audio::load_samples(input)?;
    let total = samples.len();
    let start = sample_index(start_ms, total);
    let end = end_ms.map_or(total, |ms| sample_index(ms, total).max(start));
    let slice = if start == 0 && end == total {
        samples
    } else {
        samples[start..end.min(total)].to_vec()
    };
    Ok(StreamSource::from_samples(slice, cancel))
}

fn sample_index(ms: u64, total: usize) -> usize {
    let idx = (ms as f64 / 1000.0 * f64::from(WHISPER_SAMPLE_RATE)) as usize;
    idx.min(total)
}

pub fn stream_slabs<F>(
    input: &Path,
    trim_start_ms: u64,
    trim_end_ms: Option<u64>,
    slab_sec: f64,
    cancel: Arc<AtomicBool>,
    mut on_slab: F,
) -> Result<f64>
where
    F: FnMut(crate::audio_toolkit::vad::Region) -> Result<bool>,
{
    let mut src = ffmpeg_stream(input, trim_start_ms, trim_end_ms, cancel)?;
    let slab_samples = (slab_sec * f64::from(WHISPER_SAMPLE_RATE)) as usize;
    let mut buf = vec![0.0_f32; slab_samples];
    let trim_offset_sec = trim_start_ms as f64 / 1000.0;
    let mut cursor_samples: usize = 0;
    loop {
        let n = src.read_into(&mut buf)?;
        if n == 0 {
            break;
        }
        let start_sec = cursor_samples as f64 / f64::from(WHISPER_SAMPLE_RATE) + trim_offset_sec;
        let end_sec =
            (cursor_samples + n) as f64 / f64::from(WHISPER_SAMPLE_RATE) + trim_offset_sec;
        let region = crate::audio_toolkit::vad::Region {
            start_sec,
            end_sec,
            samples: buf[..n].to_vec(),
        };
        cursor_samples += n;
        if !on_slab(region)? {
            break;
        }
    }
    Ok(cursor_samples as f64 / f64::from(WHISPER_SAMPLE_RATE) + trim_offset_sec)
}

fn format_ms(ms: u64) -> String {
    let secs = ms / 1000;
    let frac = ms % 1000;
    format!("{secs}.{frac:03}")
}
