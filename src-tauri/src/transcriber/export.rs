use std::{
    fs::File,
    io::{BufWriter, Write},
    path::Path,
};

use crate::{
    error::{Error, Result},
    transcriber::Transcript,
};

#[derive(Debug, Clone, Copy, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Txt,
    Csv,
    Json,
    Srt,
    Vtt,
}

pub fn write(transcript: &Transcript, dest: &Path, format: Format) -> Result<()> {
    if let Some(parent) = dest.parent()
        && !parent.as_os_str().is_empty()
    {
        std::fs::create_dir_all(parent)?;
    }
    let file = File::create(dest)?;
    let mut w = BufWriter::new(file);
    write_to(transcript, &mut w, format)?;
    w.flush()?;
    Ok(())
}

pub fn write_to<W: Write>(transcript: &Transcript, w: &mut W, format: Format) -> Result<()> {
    match format {
        Format::Txt => write_txt(w, transcript),
        Format::Csv => write_csv(w, transcript),
        Format::Json => write_json(w, transcript),
        Format::Srt => write_srt(w, transcript),
        Format::Vtt => write_vtt(w, transcript),
    }
}

fn speaker(u: &crate::transcriber::Utterance) -> &str {
    u.speaker.as_deref().unwrap_or("")
}

fn write_txt<W: Write>(w: &mut W, t: &Transcript) -> Result<()> {
    for u in &t.utterances {
        let sp = speaker(u);
        let stamp = format_clock(u.start_ms);
        if sp.is_empty() {
            writeln!(w, "[{stamp}] {}", u.text.trim())?;
        } else {
            writeln!(w, "[{stamp}] {sp}: {}", u.text.trim())?;
        }
    }
    Ok(())
}

fn write_csv<W: Write>(w: &mut W, t: &Transcript) -> Result<()> {
    writeln!(w, "start,end,speaker,text")?;
    for u in &t.utterances {
        writeln!(
            w,
            "{},{},{},{}",
            format_clock(u.start_ms),
            format_clock(u.end_ms),
            csv_field(speaker(u)),
            csv_field(u.text.trim()),
        )?;
    }
    Ok(())
}

fn csv_field(s: &str) -> String {
    if s.contains([',', '"', '\n', '\r']) {
        format!("\"{}\"", s.replace('"', "\"\""))
    } else {
        s.to_owned()
    }
}

fn write_json<W: Write>(w: &mut W, t: &Transcript) -> Result<()> {
    serde_json::to_writer_pretty(w, t).map_err(|e| Error::Transcribe(format!("json: {e}")))?;
    Ok(())
}

fn write_srt<W: Write>(w: &mut W, t: &Transcript) -> Result<()> {
    for (i, u) in t.utterances.iter().enumerate() {
        writeln!(w, "{}", i + 1)?;
        writeln!(w, "{} --> {}", format_srt(u.start_ms), format_srt(u.end_ms))?;
        let sp = speaker(u);
        if sp.is_empty() {
            writeln!(w, "{}", u.text.trim())?;
        } else {
            writeln!(w, "{sp}: {}", u.text.trim())?;
        }
        writeln!(w)?;
    }
    Ok(())
}

fn write_vtt<W: Write>(w: &mut W, t: &Transcript) -> Result<()> {
    writeln!(w, "WEBVTT")?;
    writeln!(w)?;
    for u in &t.utterances {
        writeln!(w, "{} --> {}", format_vtt(u.start_ms), format_vtt(u.end_ms))?;
        let sp = speaker(u);
        if sp.is_empty() {
            writeln!(w, "{}", u.text.trim())?;
        } else {
            writeln!(w, "<v {sp}>{}", u.text.trim())?;
        }
        writeln!(w)?;
    }
    Ok(())
}

fn format_clock(ms: u64) -> String {
    let s = ms / 1000;
    format!("{:02}:{:02}:{:02}", s / 3600, (s % 3600) / 60, s % 60)
}

fn format_srt(ms: u64) -> String {
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1000;
    let r = ms % 1000;
    format!("{h:02}:{m:02}:{s:02},{r:03}")
}

fn format_vtt(ms: u64) -> String {
    let h = ms / 3_600_000;
    let m = (ms % 3_600_000) / 60_000;
    let s = (ms % 60_000) / 1000;
    let r = ms % 1000;
    format!("{h:02}:{m:02}:{s:02}.{r:03}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcriber::{Transcript, Utterance};

    fn sample() -> Transcript {
        Transcript {
            model: "m".into(),
            language: "en".into(),
            duration_ms: 5_000,
            diarizer: None,
            device: None,
            speakers_detected: 2,
            utterances: vec![
                Utterance {
                    start_ms: 0,
                    end_ms: 1_500,
                    speaker: Some("SPEAKER_01".into()),
                    text: "hello, world".into(),
                    language: None,
                },
                Utterance {
                    start_ms: 2_000,
                    end_ms: 4_500,
                    speaker: None,
                    text: " un-spoken ".into(),
                    language: None,
                },
            ],
            words: Vec::new(),
        }
    }

    fn write_to_string(t: &Transcript, fmt: Format) -> String {
        let dir = tempfile::tempdir().unwrap();
        let dst = dir.path().join("out");
        write(t, &dst, fmt).unwrap();
        std::fs::read_to_string(&dst).unwrap()
    }

    #[test]
    fn formats_clock_padding() {
        assert_eq!(format_clock(0), "00:00:00");
        assert_eq!(format_clock(3_661_000), "01:01:01");
    }

    #[test]
    fn srt_uses_comma_subsecond_separator() {
        assert_eq!(format_srt(3_661_123), "01:01:01,123");
    }

    #[test]
    fn vtt_uses_dot_subsecond_separator() {
        assert_eq!(format_vtt(3_661_123), "01:01:01.123");
    }

    #[test]
    fn csv_field_quotes_embedded_specials() {
        assert_eq!(csv_field("plain"), "plain");
        assert_eq!(csv_field("a,b"), "\"a,b\"");
        assert_eq!(csv_field("she said \"hi\""), "\"she said \"\"hi\"\"\"");
        assert_eq!(csv_field("line\nbreak"), "\"line\nbreak\"");
    }

    #[test]
    fn writes_txt_with_speaker_prefix_and_clock() {
        let out = write_to_string(&sample(), Format::Txt);
        assert!(out.contains("[00:00:00] SPEAKER_01: hello, world"));
        assert!(out.contains("[00:00:02] un-spoken"));
    }

    #[test]
    fn writes_csv_with_header() {
        let out = write_to_string(&sample(), Format::Csv);
        let mut lines = out.lines();
        assert_eq!(lines.next(), Some("start,end,speaker,text"));
        assert!(out.contains("\"hello, world\""));
    }

    #[test]
    fn writes_srt_with_index_and_arrow() {
        let out = write_to_string(&sample(), Format::Srt);
        assert!(out.starts_with("1\n") || out.starts_with("1\r\n"));
        assert!(out.contains("00:00:00,000 --> 00:00:01,500"));
        assert!(out.contains("SPEAKER_01: hello, world"));
    }

    #[test]
    fn writes_vtt_with_header_and_voice_tag() {
        let out = write_to_string(&sample(), Format::Vtt);
        assert!(out.starts_with("WEBVTT"));
        assert!(out.contains("00:00:00.000 --> 00:00:01.500"));
        assert!(out.contains("<v SPEAKER_01>hello, world"));
    }

    #[test]
    fn writes_json_roundtrip() {
        let out = write_to_string(&sample(), Format::Json);
        let parsed: Transcript = serde_json::from_str(&out).unwrap();
        assert_eq!(parsed.utterances.len(), 2);
    }

    #[test]
    fn write_creates_missing_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let dst = dir.path().join("nested").join("deep").join("out.txt");
        write(&sample(), &dst, Format::Txt).unwrap();
        assert!(dst.exists());
    }
}
