#![allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]

use std::{
    fs::File,
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::Path,
};

use crate::{
    error::{Error, Result},
    fs_utils,
};

pub const WHISPER_SAMPLE_RATE: u32 = 16_000;

const PCM_FORMAT: u16 = 1;
const I16_SCALE: f32 = 1.0 / i16::MAX as f32;

pub fn read_pcm16_wav(path: &Path) -> Result<Vec<f32>> {
    let mut r = BufReader::new(File::open(path)?);
    let size = pcm_data_size(&mut r)?;
    stream_pcm_to_f32(&mut r, size as usize / 2)
}

fn pcm_data_size(r: &mut (impl Read + Seek)) -> Result<u32> {
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
        let size = u32::from_le_bytes(
            chunk[4..8]
                .try_into()
                .expect("slice of 4 bytes converts to [u8; 4]"),
        );

        match id {
            b"fmt " => {
                if size < 16 {
                    return Err(Error::Transcribe(format!("fmt chunk too small: {size}")));
                }
                let mut buf = [0u8; 16];
                r.read_exact(&mut buf)?;
                let two = |start: usize| -> [u8; 2] {
                    buf[start..start + 2]
                        .try_into()
                        .expect("slice of 2 bytes converts to [u8; 2]")
                };
                let four = |start: usize| -> [u8; 4] {
                    buf[start..start + 4]
                        .try_into()
                        .expect("slice of 4 bytes converts to [u8; 4]")
                };
                format = u16::from_le_bytes(two(0));
                channels = u16::from_le_bytes(two(2));
                sample_rate = u32::from_le_bytes(four(4));
                bits = u16::from_le_bytes(two(14));
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
                return Ok(size);
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
        for chunk in buf[..n].as_chunks::<2>().0 {
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
    fs_utils::ensure_parent_dir(path)?;
    let mut w = BufWriter::with_capacity(256 * 1024, File::create(path)?);

    let data_size = u32::try_from(samples.len() * 2).unwrap_or(u32::MAX);
    write_pcm_header(&mut w, sample_rate, data_size)?;

    for s in samples {
        let v = (s * 32767.0).clamp(-32768.0, 32767.0) as i16;
        w.write_all(&v.to_le_bytes())?;
    }
    w.flush()?;
    Ok(())
}

fn write_pcm_header(w: &mut impl Write, sample_rate: u32, data_size: u32) -> Result<()> {
    let byte_rate = sample_rate * u32::from(CHANNELS) * u32::from(BITS) / 8;
    let block_align = CHANNELS * BITS / 8;
    let chunk_size = data_size
        .checked_add(36)
        .ok_or_else(|| Error::Transcribe("audio fragment is too large".into()))?;

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

    Ok(())
}

pub(super) fn read_pcm16_wav_segment(path: &Path, start_ms: u64, end_ms: u64) -> Result<Vec<u8>> {
    if end_ms <= start_ms {
        return Err(Error::Config(
            "audio segment end must follow its start".into(),
        ));
    }
    let mut reader = BufReader::new(File::open(path)?);
    let declared_size = u64::from(pcm_data_size(&mut reader)?);
    let data_start = reader.stream_position()?;
    let available = reader
        .get_ref()
        .metadata()?
        .len()
        .saturating_sub(data_start)
        .min(declared_size);
    let frames = available / 2;
    let start = (start_ms.saturating_mul(u64::from(WHISPER_SAMPLE_RATE)) / 1000).min(frames);
    let end = (end_ms.saturating_mul(u64::from(WHISPER_SAMPLE_RATE)) / 1000).min(frames);
    if end <= start {
        return Err(Error::Config(
            "audio segment is outside the recording".into(),
        ));
    }
    let size = u32::try_from((end - start) * 2)
        .map_err(|_| Error::Config("audio fragment is too large".into()))?;
    let mut bytes = Vec::with_capacity(size as usize + 44);
    write_pcm_header(&mut bytes, WHISPER_SAMPLE_RATE, size)?;
    reader.seek(SeekFrom::Start(data_start + start * 2))?;
    bytes.resize(size as usize + 44, 0);
    reader.read_exact(&mut bytes[44..])?;
    Ok(bytes)
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
    #[test]
    fn extracts_only_requested_samples_as_a_complete_wav() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.wav");
        let mut samples = vec![-0.5; 16000];
        samples.extend(vec![0.5; 16000]);
        write_pcm16_wav(&source, &samples, WHISPER_SAMPLE_RATE).unwrap();
        let bytes = read_pcm16_wav_segment(&source, 900, 1100).unwrap();
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(bytes.len(), 44 + 3200 * 2);
        let fragment = dir.path().join("fragment.wav");
        std::fs::write(&fragment, bytes).unwrap();
        let decoded = read_pcm16_wav(&fragment).unwrap();
        assert_eq!(decoded.len(), 3200);
        assert!(decoded[..1600].iter().all(|value| *value < -0.49));
        assert!(decoded[1600..].iter().all(|value| *value > 0.49));
    }

    #[test]
    fn clips_to_track_end_and_rejects_empty_ranges() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.wav");
        write_pcm16_wav(&source, &vec![0.25; 16000], WHISPER_SAMPLE_RATE).unwrap();
        assert_eq!(
            read_pcm16_wav_segment(&source, 900, 2000).unwrap().len(),
            44 + 1600 * 2
        );
        assert!(read_pcm16_wav_segment(&source, 1000, 2000).is_err());
        assert!(read_pcm16_wav_segment(&source, 500, 400).is_err());
        assert!(read_pcm16_wav_segment(&source, 500, 500).is_err());
    }

    #[test]
    fn extracts_audio_after_extra_padded_riff_chunks() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("source.wav");
        write_pcm16_wav(&source, &vec![0.25; 16000], WHISPER_SAMPLE_RATE).unwrap();
        let mut bytes = std::fs::read(&source).unwrap();
        let mut junk = b"JUNK".to_vec();
        junk.extend(3_u32.to_le_bytes());
        junk.extend([1, 2, 3, 0]);
        bytes.splice(36..36, junk);
        let riff_size = u32::try_from(bytes.len() - 8).unwrap();
        bytes[4..8].copy_from_slice(&riff_size.to_le_bytes());
        std::fs::write(&source, bytes).unwrap();
        assert_eq!(
            read_pcm16_wav_segment(&source, 0, 100).unwrap().len(),
            44 + 1600 * 2
        );
    }
}
