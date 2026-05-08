#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
};

use crate::error::{Error, Result};

pub const WHISPER_SAMPLE_RATE: u32 = 16_000;

const PCM_FORMAT: u16 = 1;
const I16_SCALE: f32 = 1.0 / i16::MAX as f32;

pub fn read_pcm16_wav(path: &Path) -> Result<Vec<f32>> {
    let mut r = BufReader::new(File::open(path)?);

    let mut header = [0u8; 12];
    r.read_exact(&mut header)?;
    if &header[0..4] != b"RIFF" {
        return Err(Error::Transcribe("not a RIFF file".into()));
    }
    if &header[8..12] != b"WAVE" {
        return Err(Error::Transcribe("not a WAVE file".into()));
    }

    let mut sample_rate = 0u32;
    let mut channels = 0u16;
    let mut bits = 0u16;
    let mut format = 0u16;
    let mut found_fmt = false;

    loop {
        let mut chunk = [0u8; 8];
        if r.read_exact(&mut chunk).is_err() {
            break;
        }
        let id = &chunk[0..4];
        let size = u32::from_le_bytes(chunk[4..8].try_into().unwrap());

        match id {
            b"fmt " => {
                if size < 16 {
                    return Err(Error::Transcribe(format!("fmt chunk too small: {size}")));
                }
                let mut buf = [0u8; 16];
                r.read_exact(&mut buf)?;
                format = u16::from_le_bytes(buf[0..2].try_into().unwrap());
                channels = u16::from_le_bytes(buf[2..4].try_into().unwrap());
                sample_rate = u32::from_le_bytes(buf[4..8].try_into().unwrap());
                bits = u16::from_le_bytes(buf[14..16].try_into().unwrap());
                let remaining = i64::from(size) - 16;
                if remaining > 0 {
                    r.seek(SeekFrom::Current(remaining))?;
                }
                found_fmt = true;
            }
            b"data" => {
                if !found_fmt {
                    return Err(Error::Transcribe("data chunk before fmt chunk".into()));
                }
                if format != PCM_FORMAT {
                    return Err(Error::Transcribe(format!("not PCM (format={format})")));
                }
                if sample_rate != WHISPER_SAMPLE_RATE {
                    return Err(Error::Transcribe(format!(
                        "wrong sample rate: {sample_rate}"
                    )));
                }
                if channels != 1 {
                    return Err(Error::Transcribe(format!("not mono: {channels} channels")));
                }
                if bits != 16 {
                    return Err(Error::Transcribe(format!("not 16-bit: {bits}")));
                }
                let count = (size as usize) / 2;
                return stream_pcm_to_f32(&mut r, count);
            }
            _ => {
                let skip = i64::from(size) + i64::from(size % 2);
                r.seek(SeekFrom::Current(skip))?;
            }
        }
    }

    Err(Error::Transcribe("no data chunk found".into()))
}

fn stream_pcm_to_f32<R: Read>(r: &mut R, count: usize) -> Result<Vec<f32>> {
    let mut samples = Vec::with_capacity(count);
    let mut buf = vec![0u8; 1 << 20];
    let mut remaining = count;
    while remaining > 0 {
        let want = (remaining * 2).min(buf.len() & !1);
        let n = r.read(&mut buf[..want])?;
        if n == 0 {
            break;
        }
        let n = n & !1;
        for chunk in buf[..n].chunks_exact(2) {
            let s = i16::from_le_bytes([chunk[0], chunk[1]]);
            samples.push(f32::from(s) * I16_SCALE);
            remaining -= 1;
        }
    }
    Ok(samples)
}

const CHANNELS: u16 = 1;
const BITS: u16 = 16;

pub fn write_pcm16_wav(path: &Path, samples: &[f32], sample_rate: u32) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut w = BufWriter::with_capacity(256 * 1024, File::create(path)?);

    let byte_rate = sample_rate * u32::from(CHANNELS) * u32::from(BITS) / 8;
    let block_align = CHANNELS * BITS / 8;
    let data_size = u32::try_from(samples.len() * 2).unwrap_or(u32::MAX);
    let chunk_size = 36 + data_size;

    w.write_all(b"RIFF")?;
    w.write_all(&chunk_size.to_le_bytes())?;
    w.write_all(b"WAVEfmt ")?;
    w.write_all(&16u32.to_le_bytes())?;
    w.write_all(&PCM_FORMAT.to_le_bytes())?;
    w.write_all(&CHANNELS.to_le_bytes())?;
    w.write_all(&sample_rate.to_le_bytes())?;
    w.write_all(&byte_rate.to_le_bytes())?;
    w.write_all(&block_align.to_le_bytes())?;
    w.write_all(&BITS.to_le_bytes())?;
    w.write_all(b"data")?;
    w.write_all(&data_size.to_le_bytes())?;

    for s in samples {
        let v = (s * 32767.0).clamp(-32768.0, 32767.0) as i16;
        w.write_all(&v.to_le_bytes())?;
    }
    w.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_then_read_roundtrip_preserves_samples() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip.wav");
        let samples: Vec<f32> = (0..4_000).map(|i| (i as f32 / 4_000.0) - 0.5).collect();
        write_pcm16_wav(&path, &samples, WHISPER_SAMPLE_RATE).unwrap();
        let read = read_pcm16_wav(&path).unwrap();
        assert_eq!(read.len(), samples.len());
        for (a, b) in samples.iter().zip(read.iter()) {
            assert!((a - b).abs() < 1e-3);
        }
    }

    #[test]
    fn read_rejects_non_riff_header() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("bogus.wav");
        std::fs::write(&path, b"NOTAWAVEFILEPADDED").unwrap();
        assert!(read_pcm16_wav(&path).is_err());
    }

    #[test]
    fn read_rejects_wrong_sample_rate() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("48k.wav");
        write_pcm16_wav(&path, &[0.0; 16], 48_000).unwrap();
        assert!(read_pcm16_wav(&path).is_err());
    }

    #[test]
    fn write_clamps_extreme_input_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("clip.wav");
        write_pcm16_wav(&path, &[5.0, -5.0, 0.0], WHISPER_SAMPLE_RATE).unwrap();
        let read = read_pcm16_wav(&path).unwrap();
        assert!((read[0] - 1.0).abs() < 1e-3);
        assert!((read[1] - -1.0).abs() < 1e-3);
        assert!(read[2].abs() < 1e-3);
    }
}
