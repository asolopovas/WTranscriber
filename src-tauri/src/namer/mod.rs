use std::{path::Path, sync::LazyLock};

use chrono::{DateTime, Local};
use regex::Regex;
use serde::{Deserialize, Serialize};

use crate::{
    error::{Error, Result},
    llm::{Options, Runner},
    transcriber::Transcript,
};

const FILENAME_GRAMMAR: &str = r#"root ::= "{" ws "\"topic\":" ws "\"" topic "\"" ws "}"
topic ::= slugChar{5,60}
slugChar ::= [a-z0-9-]
ws ::= [ \t\n]*
"#;

const EXCERPT_LIMIT: usize = 6000;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Suggestion {
    pub topic: String,
    pub stamp: String,
}

impl Suggestion {
    #[must_use]
    pub fn filename(&self, ext: &str) -> String {
        let ext = ext.trim_start_matches('.');
        if ext.is_empty() {
            format!("{}_{}", self.topic, self.stamp)
        } else {
            format!("{}_{}.{}", self.topic, self.stamp, ext)
        }
    }
}

#[derive(Deserialize)]
struct LlmReply {
    topic: String,
}

static SLUG_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"[^a-z0-9-]+").unwrap());
static MULTI_RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"-+").unwrap());

#[must_use]
pub fn extract_text(transcript: &Transcript) -> String {
    let mut out = String::new();
    for u in &transcript.utterances {
        if let Some(spk) = &u.speaker {
            out.push_str(spk);
            out.push_str(": ");
        }
        out.push_str(&u.text);
        out.push('\n');
    }
    out
}

pub fn suggest(transcript: &Transcript, fallback_date: DateTime<Local>) -> Result<Suggestion> {
    let runner = Runner::new()?;
    let text = extract_text(transcript);
    let excerpt = if text.len() > EXCERPT_LIMIT {
        &text[..EXCERPT_LIMIT]
    } else {
        text.as_str()
    };
    let prompt = build_prompt(excerpt);

    let raw = runner.generate(&Options {
        prompt,
        grammar: Some(FILENAME_GRAMMAR.to_owned()),
        max_tokens: 80,
        temp: 0.1,
    })?;

    let reply: LlmReply = serde_json::from_str(&raw)
        .map_err(|e| Error::Transcribe(format!("parsing LLM JSON {raw:?}: {e}")))?;

    let mut topic = sanitize_topic(&reply.topic);
    if topic.is_empty() {
        topic = "untitled".into();
    }
    Ok(Suggestion {
        topic,
        stamp: fallback_date.format("%y%m%d-%H%M%S").to_string(),
    })
}

fn build_prompt(excerpt: &str) -> String {
    format!(
        "You are a filename topic generator. Read the conversation transcript below and respond with a single JSON object: {{\"topic\": \"<slug>\"}}.\n\n\
         The topic must be a kebab-case slug of 3-7 lowercase words joined with hyphens that captures the main subject, setting, or purpose of the conversation. Use only ASCII letters, digits, and hyphens. Max 60 characters. Be specific (e.g. \"fulham-boys-school-admission-interview\", not \"interview\"; \"kitchen-renovation-quote\", not \"renovation\"; \"weekly-sales-team-standup\", not \"meeting\"). Avoid generic single words like \"sports\", \"talk\", \"meeting\".\n\n\
         Output ONLY the JSON object, no prose, no commentary, no markdown.\n\n\
         Transcript:\n{excerpt}\n\nJSON:"
    )
}

#[must_use]
pub fn sanitize_topic(s: &str) -> String {
    let s = s.trim().to_lowercase();
    let s = SLUG_RE.replace_all(&s, "-");
    let s = MULTI_RE.replace_all(&s, "-");
    let mut s = s.trim_matches('-').to_owned();
    if s.len() > 60 {
        s.truncate(60);
        s = s.trim_matches('-').to_owned();
    }
    s
}

pub fn rename_with_suggestion(
    original: &Path,
    suggestion: &Suggestion,
) -> Result<std::path::PathBuf> {
    let parent = original.parent().unwrap_or_else(|| Path::new("."));
    let ext = original
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or_default();
    let new_name = suggestion.filename(ext);
    let target = parent.join(&new_name);
    std::fs::rename(original, &target)?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_punctuation_and_case() {
        assert_eq!(
            sanitize_topic("  Kitchen Renovation Quote!  "),
            "kitchen-renovation-quote"
        );
    }

    #[test]
    fn sanitizes_multiple_separators() {
        assert_eq!(sanitize_topic("a__b---c"), "a-b-c");
    }

    #[test]
    fn truncates_to_sixty_chars() {
        let long = "a".repeat(120);
        let out = sanitize_topic(&long);
        assert!(out.len() <= 60);
    }

    #[test]
    fn filename_with_extension() {
        let s = Suggestion {
            topic: "test-topic".into(),
            stamp: "240101-120000".into(),
        };
        assert_eq!(s.filename("ogg"), "test-topic_240101-120000.ogg");
        assert_eq!(s.filename(".wav"), "test-topic_240101-120000.wav");
        assert_eq!(s.filename(""), "test-topic_240101-120000");
    }

    #[test]
    fn sanitize_topic_strips_diacritics_to_hyphens() {
        assert_eq!(sanitize_topic("naïve café"), "na-ve-caf");
    }

    #[test]
    fn sanitize_topic_collapses_to_empty_for_pure_punctuation() {
        assert!(sanitize_topic("!!!").is_empty());
    }

    #[test]
    fn extract_text_includes_speaker_prefix() {
        let t = Transcript {
            model: "m".into(),
            language: "en".into(),
            duration_ms: 0,
            diarizer: None,
            device: None,
            speakers_detected: 1,
            utterances: vec![crate::transcriber::Utterance {
                start_ms: 0,
                end_ms: 1,
                speaker: Some("SPEAKER_01".into()),
                text: "hello".into(),
                language: None,
            }],
            words: Vec::new(),
        };
        assert_eq!(extract_text(&t), "SPEAKER_01: hello\n");
    }

    #[test]
    fn extract_text_omits_speaker_when_absent() {
        let t = Transcript {
            model: "m".into(),
            language: "en".into(),
            duration_ms: 0,
            diarizer: None,
            device: None,
            speakers_detected: 0,
            utterances: vec![crate::transcriber::Utterance {
                start_ms: 0,
                end_ms: 1,
                speaker: None,
                text: "lone line".into(),
                language: None,
            }],
            words: Vec::new(),
        };
        assert_eq!(extract_text(&t), "lone line\n");
    }
}
