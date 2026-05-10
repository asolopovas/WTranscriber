#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

mod lang;
mod words;

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::diarizer;

use self::lang::{detect_script_lang, resolve_language};
use self::words::{group_words, smooth_flickers};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Transcript {
    pub model: String,
    pub language: String,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub diarizer: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device: Option<String>,
    pub speakers_detected: usize,
    pub utterances: Vec<Utterance>,
    pub words: Vec<Word>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Utterance {
    pub start_ms: u64,
    pub end_ms: u64,
    pub speaker: Option<String>,
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Word {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    pub speaker: Option<String>,
    pub confidence: f32,
}

#[derive(Debug, Clone, Default)]
pub struct Meta {
    pub model: String,
    pub language: String,
    pub duration_ms: u64,
    pub diarizer: Option<String>,
    pub device: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Token {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    #[serde(default)]
    pub confidence: f32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Segment {
    pub text: String,
    pub start_ms: u64,
    pub end_ms: u64,
    #[serde(default)]
    pub tokens: Vec<Token>,
}

pub use diarizer::Segment as DiarSegment;

pub fn build(segments: &[Segment], diar: &[DiarSegment], meta: Meta) -> Transcript {
    let mut hint: Option<u32> = None;
    let mut labels: HashMap<u32, String> = HashMap::new();
    let mut next: u32 = 1;
    let mut label_for = |id: Option<u32>| -> Option<String> {
        let id = id?;
        if let Some(l) = labels.get(&id) {
            return Some(l.clone());
        }
        let l = format!("SPEAKER_{next:02}");
        labels.insert(id, l.clone());
        next += 1;
        Some(l)
    };

    let mut lookup = |start_ms: u64, end_ms: u64| -> Option<u32> {
        let id = diarizer::speaker_id_for_time(
            start_ms as f64 / 1000.0,
            end_ms as f64 / 1000.0,
            diar,
            hint,
        );
        if let Some(v) = id {
            hint = Some(v);
        }
        id
    };

    let mut words = Vec::new();
    for seg in segments {
        if seg.tokens.is_empty() {
            let speaker = label_for(lookup(seg.start_ms, seg.end_ms));
            words.push(Word {
                text: seg.text.clone(),
                start_ms: seg.start_ms,
                end_ms: seg.end_ms,
                speaker,
                confidence: 0.0,
            });
            continue;
        }
        for tok in &seg.tokens {
            let speaker = label_for(lookup(tok.start_ms, tok.end_ms));
            words.push(Word {
                text: tok.text.clone(),
                start_ms: tok.start_ms,
                end_ms: tok.end_ms,
                speaker,
                confidence: tok.confidence,
            });
        }
    }

    smooth_flickers(&mut words);
    let mut utterances = group_words(&words);
    for u in &mut utterances {
        u.language = detect_script_lang(&u.text);
    }

    let mut speakers = std::collections::HashSet::new();
    for w in &words {
        if let Some(s) = &w.speaker {
            speakers.insert(s.clone());
        }
    }

    let language = resolve_language(&meta.language, &utterances);

    Transcript {
        model: meta.model,
        language,
        duration_ms: meta.duration_ms,
        diarizer: meta.diarizer,
        device: meta.device,
        speakers_detected: speakers.len(),
        utterances,
        words,
    }
}

#[cfg(test)]
mod tests {
    use super::lang::{detect_script_lang, resolve_language};
    use super::words::{group_words, is_sentence_end, join_words, smooth_flickers};
    use super::*;

    #[test]
    fn detects_sentence_end() {
        assert!(is_sentence_end("hello."));
        assert!(is_sentence_end("really?"));
        assert!(!is_sentence_end("ongoing"));
    }

    #[test]
    fn joins_words_collapsing_punctuation() {
        let parts = vec!["hello".into(), ",".into(), "world".into(), ".".into()];
        assert_eq!(join_words(&parts), "hello, world.");
    }

    fn word(text: &str, start_ms: u64, end_ms: u64, speaker: Option<&str>) -> Word {
        Word {
            text: text.into(),
            start_ms,
            end_ms,
            speaker: speaker.map(str::to_owned),
            confidence: 0.0,
        }
    }

    #[test]
    fn smooths_isolated_flicker() {
        let mut words = vec![
            word("a", 0, 1, Some("A")),
            word("b", 1, 2, Some("B")),
            word("c", 2, 3, Some("A")),
        ];
        smooth_flickers(&mut words);
        assert_eq!(words[1].speaker.as_deref(), Some("A"));
    }

    #[test]
    fn smooth_flickers_noop_for_short_inputs() {
        let mut empty: Vec<Word> = Vec::new();
        smooth_flickers(&mut empty);
        let mut single = vec![word("a", 0, 1, Some("A"))];
        smooth_flickers(&mut single);
        assert_eq!(single[0].speaker.as_deref(), Some("A"));
    }

    #[test]
    fn group_words_splits_on_speaker_change() {
        let words = vec![word("hi", 0, 1, Some("A")), word("there", 1, 2, Some("B"))];
        let utts = group_words(&words);
        assert_eq!(utts.len(), 2);
        assert_eq!(utts[0].speaker.as_deref(), Some("A"));
        assert_eq!(utts[1].speaker.as_deref(), Some("B"));
    }

    #[test]
    fn group_words_breaks_after_sentence_end() {
        let words = vec![
            word("hello.", 0, 1, Some("A")),
            word("again", 1, 2, Some("A")),
        ];
        let utts = group_words(&words);
        assert_eq!(utts.len(), 2);
        assert_eq!(utts[0].text, "hello.");
        assert_eq!(utts[1].text, "again");
    }

    #[test]
    fn group_words_returns_empty_for_empty_input() {
        assert!(group_words(&[]).is_empty());
    }

    #[test]
    fn detect_script_lang_picks_dominant_script() {
        assert_eq!(detect_script_lang("hello world").as_deref(), Some("en"));
        assert_eq!(detect_script_lang("Привет мир").as_deref(), Some("ru"));
        assert_eq!(detect_script_lang("你好世界").as_deref(), Some("zh"));
        assert_eq!(detect_script_lang("123 ...").as_deref(), None);
    }

    #[test]
    fn resolve_language_prefers_explicit_meta() {
        assert_eq!(resolve_language("en", &[]), "en");
    }

    #[test]
    fn resolve_language_falls_back_to_detected_when_auto() {
        let utts = vec![Utterance {
            start_ms: 0,
            end_ms: 1,
            speaker: None,
            text: "hello world".into(),
            language: Some("en".into()),
        }];
        assert_eq!(resolve_language("auto", &utts), "en");
    }

    #[test]
    fn resolve_language_joins_multiple_detected_languages() {
        let utts = vec![
            Utterance {
                start_ms: 0,
                end_ms: 1,
                speaker: None,
                text: "hi".into(),
                language: Some("en".into()),
            },
            Utterance {
                start_ms: 1,
                end_ms: 2,
                speaker: None,
                text: "Привет".into(),
                language: Some("ru".into()),
            },
        ];
        assert_eq!(resolve_language("", &utts), "en,ru");
    }

    #[test]
    fn build_assigns_speaker_labels_from_diarization() {
        let segs = vec![Segment {
            text: "hello world".into(),
            start_ms: 0,
            end_ms: 2_000,
            tokens: vec![
                Token {
                    text: "hello".into(),
                    start_ms: 0,
                    end_ms: 1_000,
                    confidence: 0.0,
                },
                Token {
                    text: "world".into(),
                    start_ms: 1_000,
                    end_ms: 2_000,
                    confidence: 0.0,
                },
            ],
        }];
        let diar = vec![
            DiarSegment {
                speaker: 7,
                start_sec: 0.0,
                end_sec: 1.0,
            },
            DiarSegment {
                speaker: 9,
                start_sec: 1.0,
                end_sec: 2.0,
            },
        ];
        let t = build(
            &segs,
            &diar,
            Meta {
                model: "m".into(),
                language: "en".into(),
                duration_ms: 2_000,
                diarizer: None,
                device: None,
            },
        );
        assert_eq!(t.speakers_detected, 2);
        assert_eq!(t.words[0].speaker.as_deref(), Some("SPEAKER_01"));
        assert_eq!(t.words[1].speaker.as_deref(), Some("SPEAKER_02"));
    }
}
