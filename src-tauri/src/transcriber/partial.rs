use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::{
    error::Result,
    transcriber::{
        cache::transcript_path,
        transcript::Segment,
    },
};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Partial {
    pub key: String,
    #[serde(default)]
    pub last_done_sec: f64,
    #[serde(default)]
    pub segments: Vec<SerSegment>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerSegment {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    #[serde(default)]
    pub tokens: Vec<SerToken>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SerToken {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    #[serde(default)]
    pub confidence: f32,
}

impl From<&Segment> for SerSegment {
    fn from(s: &Segment) -> Self {
        Self {
            text: s.text.clone(),
            start_ms: s.start_ms,
            end_ms: s.end_ms,
            tokens: s
                .tokens
                .iter()
                .map(|t| SerToken {
                    text: t.text.clone(),
                    start_ms: t.start_ms,
                    end_ms: t.end_ms,
                    confidence: t.confidence,
                })
                .collect(),
        }
    }
}

impl From<SerSegment> for Segment {
    fn from(s: SerSegment) -> Self {
        Self {
            text: s.text,
            start_ms: s.start_ms,
            end_ms: s.end_ms,
            tokens: s
                .tokens
                .into_iter()
                .map(|t| crate::transcriber::transcript::Token {
                    text: t.text,
                    start_ms: t.start_ms,
                    end_ms: t.end_ms,
                    confidence: t.confidence,
                })
                .collect(),
        }
    }
}

fn partial_path(key: &str) -> Result<PathBuf> {
    let p = transcript_path(key)?;
    let mut s = p.into_os_string();
    s.push(".partial.json");
    Ok(PathBuf::from(s))
}

#[must_use]
pub fn load(key: &str) -> Option<Partial> {
    let path = partial_path(key).ok()?;
    let raw = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&raw).ok()
}

pub fn save(p: &Partial) -> Result<()> {
    let path = partial_path(&p.key)?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let raw = serde_json::to_string(p)?;
    std::fs::write(path, raw)?;
    Ok(())
}

pub fn clear(key: &str) -> Result<()> {
    let path = partial_path(key)?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    Ok(())
}
