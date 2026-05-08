#![allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss
)]

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::diarizer;

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

fn detect_script_lang(text: &str) -> Option<String> {
    let mut latin = 0usize;
    let mut cyrillic = 0usize;
    let mut cjk = 0usize;
    for c in text.chars() {
        let u = c as u32;
        if (0x0400..=0x04FF).contains(&u) || (0x0500..=0x052F).contains(&u) {
            cyrillic += 1;
        } else if (0x0041..=0x024F).contains(&u) {
            latin += 1;
        } else if (0x4E00..=0x9FFF).contains(&u)
            || (0x3040..=0x30FF).contains(&u)
            || (0xAC00..=0xD7AF).contains(&u)
        {
            cjk += 1;
        }
    }
    let total = latin + cyrillic + cjk;
    if total == 0 {
        return None;
    }
    if cyrillic * 2 >= total {
        Some("ru".into())
    } else if cjk * 2 >= total {
        Some("zh".into())
    } else if latin > 0 {
        Some("en".into())
    } else {
        None
    }
}

fn resolve_language(meta_lang: &str, utts: &[Utterance]) -> String {
    let explicit = !meta_lang.is_empty() && meta_lang != "auto";
    if explicit {
        return meta_lang.into();
    }
    let mut seen: Vec<String> = Vec::new();
    for u in utts {
        if let Some(l) = &u.language
            && !seen.contains(l)
        {
            seen.push(l.clone());
        }
    }
    match seen.len() {
        0 => meta_lang.into(),
        1 => seen.into_iter().next().unwrap(),
        _ => seen.join(","),
    }
}

fn smooth_flickers(words: &mut [Word]) {
    let n = words.len();
    if n < 3 {
        return;
    }
    for i in 1..n - 1 {
        if words[i].speaker != words[i - 1].speaker && words[i - 1].speaker == words[i + 1].speaker
        {
            let prev = words[i - 1].speaker.clone();
            words[i].speaker.clone_from(&prev);
        }
    }
}

fn is_sentence_end(text: &str) -> bool {
    text.trim_end_matches(['"', '\'', ')', ']', '}', '\u{201D}', '\u{2019}'])
        .chars()
        .next_back()
        .is_some_and(|c| matches!(c, '.' | '?' | '!'))
}

fn group_words(words: &[Word]) -> Vec<Utterance> {
    if words.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::with_capacity(words.len() / 4 + 1);
    let mut cur_start = words[0].start_ms;
    let mut cur_end = words[0].end_ms;
    let mut cur_spk = words[0].speaker.clone();
    let mut parts = vec![words[0].text.clone()];
    let mut prev_end = is_sentence_end(&words[0].text);

    let flush =
        |out: &mut Vec<Utterance>, start: u64, end: u64, spk: &Option<String>, parts: &[String]| {
            out.push(Utterance {
                start_ms: start,
                end_ms: end,
                speaker: spk.clone(),
                text: join_words(parts),
                language: None,
            });
        };

    for w in &words[1..] {
        if w.speaker != cur_spk || prev_end {
            flush(&mut out, cur_start, cur_end, &cur_spk, &parts);
            cur_start = w.start_ms;
            cur_spk.clone_from(&w.speaker);
            parts.clear();
        }
        cur_end = w.end_ms;
        parts.push(w.text.clone());
        prev_end = is_sentence_end(&w.text);
    }
    flush(&mut out, cur_start, cur_end, &cur_spk, &parts);
    out
}

fn join_words(parts: &[String]) -> String {
    let mut s = parts.join(" ");
    for p in [" ,", " .", " ?", " !", " ;", " :"] {
        s = s.replace(p, &p[1..]);
    }
    s
}

#[cfg(test)]
mod tests {
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

    #[test]
    fn smooths_isolated_flicker() {
        let mut words = vec![
            Word {
                text: "a".into(),
                start_ms: 0,
                end_ms: 1,
                speaker: Some("A".into()),
                confidence: 0.0,
            },
            Word {
                text: "b".into(),
                start_ms: 1,
                end_ms: 2,
                speaker: Some("B".into()),
                confidence: 0.0,
            },
            Word {
                text: "c".into(),
                start_ms: 2,
                end_ms: 3,
                speaker: Some("A".into()),
                confidence: 0.0,
            },
        ];
        smooth_flickers(&mut words);
        assert_eq!(words[1].speaker.as_deref(), Some("A"));
    }
}
