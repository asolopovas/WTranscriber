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
    match format {
        Format::Txt => write_txt(&mut w, transcript)?,
        Format::Csv => write_csv(&mut w, transcript)?,
        Format::Json => write_json(&mut w, transcript)?,
        Format::Srt => write_srt(&mut w, transcript)?,
        Format::Vtt => write_vtt(&mut w, transcript)?,
    }
    w.flush()?;
    Ok(())
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
        writeln!(
            w,
            "{} --> {}",
            format_srt(u.start_ms),
            format_srt(u.end_ms)
        )?;
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
        writeln!(
            w,
            "{} --> {}",
            format_vtt(u.start_ms),
            format_vtt(u.end_ms)
        )?;
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
