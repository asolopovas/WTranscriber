use super::{Utterance, Word};

pub(super) fn smooth_flickers(words: &mut [Word]) {
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

pub(super) fn is_sentence_end(text: &str) -> bool {
    text.trim_end_matches(['"', '\'', ')', ']', '}', '\u{201D}', '\u{2019}'])
        .chars()
        .next_back()
        .is_some_and(|c| matches!(c, '.' | '?' | '!'))
}

pub(super) fn group_words(words: &[Word]) -> Vec<Utterance> {
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

pub(super) fn join_words(parts: &[String]) -> String {
    let mut s = parts.join(" ");
    for p in [" ,", " .", " ?", " !", " ;", " :"] {
        s = s.replace(p, &p[1..]);
    }
    s
}
