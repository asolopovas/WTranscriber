use crate::transcriber::Token;

const MAX_NGRAM: usize = 5;
const MIN_RUN_BY_N1: usize = 4;
const MIN_RUN_BY_N2: usize = 3;
const MIN_RUN_BY_LARGE: usize = 2;

const BRIDGED_MIN_N: usize = 4;
const BRIDGED_MAX_N: usize = 12;
const BRIDGED_MAX_BRIDGE: usize = 6;

const STOPWORDS: &[&str] = &[
    "the", "a", "an", "and", "or", "but", "so", "of", "to", "in", "on", "at", "by", "for", "with",
    "from", "as", "is", "it", "be", "are", "was", "were", "been", "i", "you", "he", "she", "we",
    "they", "me", "him", "her", "us", "them", "my", "your", "his", "our", "their", "this", "that",
    "these", "those", "do", "does", "did", "will", "would", "can", "could", "should", "have",
    "has", "had", "yes", "no", "yeah", "ok", "okay", "well", "just", "like", "um", "uh",
];

const fn min_run_for_ngram(n: usize) -> usize {
    match n {
        1 => MIN_RUN_BY_N1,
        2 => MIN_RUN_BY_N2,
        _ => MIN_RUN_BY_LARGE,
    }
}

fn canon(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

fn tokens_equal(tokens: &[Token], a: usize, b: usize, n: usize) -> bool {
    if a + n > tokens.len() || b + n > tokens.len() {
        return false;
    }
    (0..n).all(|i| canon(&tokens[a + i].text) == canon(&tokens[b + i].text))
}

pub fn collapse_repeats(tokens: &[Token]) -> Vec<Token> {
    if tokens.len() < 2 {
        return tokens.to_vec();
    }
    let mut out = Vec::with_capacity(tokens.len());
    let mut i = 0;
    while i < tokens.len() {
        let mut best_n = 0usize;
        let mut best_runs = 1usize;
        let mut n = 1;
        while n <= MAX_NGRAM && i + n * 2 <= tokens.len() {
            let mut runs = 1;
            let mut j = i + n;
            while j + n <= tokens.len() && tokens_equal(tokens, i, j, n) {
                runs += 1;
                j += n;
            }
            if runs >= min_run_for_ngram(n) && (n * runs) > (best_n * best_runs) {
                best_n = n;
                best_runs = runs;
            }
            n += 1;
        }
        if best_n > 0 {
            out.extend_from_slice(&tokens[i..i + best_n]);
            i += best_n * best_runs;
        } else {
            out.push(tokens[i].clone());
            i += 1;
        }
    }
    out
}

fn ngram_has_content(tokens: &[Token], start: usize, n: usize) -> bool {
    for k in 0..n {
        let c = canon(&tokens[start + k].text);
        if c.chars().count() >= 4 && !STOPWORDS.contains(&c.as_str()) {
            return true;
        }
    }
    false
}

fn ngram_equal(tokens: &[Token], a: usize, b: usize, n: usize, keep: &[bool]) -> bool {
    (0..n).all(|k| {
        keep[a + k] && keep[b + k] && canon(&tokens[a + k].text) == canon(&tokens[b + k].text)
    })
}

pub fn collapse_bridged_repeats(tokens: &[Token]) -> Vec<Token> {
    let len = tokens.len();
    if len < 2 * BRIDGED_MIN_N {
        return tokens.to_vec();
    }
    let mut keep = vec![true; len];
    let mut i = 0;
    while i + BRIDGED_MIN_N <= len {
        if !keep[i] {
            i += 1;
            continue;
        }
        let mut collapsed_to: Option<usize> = None;
        let max_n = BRIDGED_MAX_N.min(len - i);
        'outer: for n in (BRIDGED_MIN_N..=max_n).rev() {
            if !ngram_has_content(tokens, i, n) {
                continue;
            }
            for gap in 0..=BRIDGED_MAX_BRIDGE {
                let j = i + n + gap;
                if j + n > len {
                    break;
                }
                if !keep[j] {
                    continue;
                }
                if ngram_equal(tokens, i, j, n, &keep) {
                    for slot in keep.iter_mut().skip(j).take(n) {
                        *slot = false;
                    }
                    crate::logfile::info(&format!(
                        "dedup: collapsed bridged repeat n={n} gap={gap} at token {i} (\"{}\")",
                        (0..n)
                            .map(|k| tokens[i + k].text.as_str())
                            .collect::<Vec<_>>()
                            .join(" ")
                    ));
                    collapsed_to = Some(j + n);
                    break 'outer;
                }
            }
        }
        i = collapsed_to.unwrap_or(i + 1);
    }
    tokens
        .iter()
        .enumerate()
        .filter(|&(k, _)| keep[k])
        .map(|(_, t)| t.clone())
        .collect()
}

pub fn collapse_in_text(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    if words.len() < 2 {
        return text.to_owned();
    }
    let synth: Vec<Token> = words
        .iter()
        .map(|w| Token {
            text: (*w).to_owned(),
            start_ms: 0,
            end_ms: 0,
            confidence: 0.0,
        })
        .collect();
    let collapsed = collapse_repeats(&synth);
    if collapsed.len() == synth.len() {
        text.to_owned()
    } else {
        collapsed
            .into_iter()
            .map(|t| t.text)
            .collect::<Vec<_>>()
            .join(" ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tok(text: &str) -> Token {
        Token {
            text: text.into(),
            start_ms: 0,
            end_ms: 0,
            confidence: 0.0,
        }
    }

    #[test]
    fn collapses_quadruple_unigram() {
        let tokens = vec![tok("yes"), tok("yes"), tok("yes"), tok("yes"), tok("done")];
        let out = collapse_repeats(&tokens);
        assert_eq!(out.len(), 2);
        assert_eq!(out[0].text, "yes");
        assert_eq!(out[1].text, "done");
    }

    #[test]
    fn collapses_repeated_bigram() {
        let tokens: Vec<Token> = ["ab", "cd", "ab", "cd", "ab", "cd", "end"]
            .into_iter()
            .map(tok)
            .collect();
        let out = collapse_repeats(&tokens);
        assert_eq!(
            out.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(),
            vec!["ab", "cd", "end"]
        );
    }

    #[test]
    fn keeps_non_repeating_text() {
        let tokens = vec![tok("hello"), tok("there"), tok("friend")];
        assert_eq!(collapse_repeats(&tokens).len(), 3);
    }

    #[test]
    fn collapse_in_text_handles_strings() {
        assert_eq!(collapse_in_text("yes yes yes yes go"), "yes go");
    }

    #[test]
    fn canon_strips_punctuation() {
        assert_eq!(canon("Hello,"), "hello");
        assert_eq!(canon("Hello"), canon("hello!"));
    }

    #[test]
    fn collapses_repeated_trigram() {
        let tokens: Vec<Token> = ["a", "b", "c", "a", "b", "c", "x"]
            .into_iter()
            .map(tok)
            .collect();
        let out = collapse_repeats(&tokens);
        assert_eq!(
            out.iter().map(|t| t.text.as_str()).collect::<Vec<_>>(),
            vec!["a", "b", "c", "x"]
        );
    }

    #[test]
    fn keeps_double_unigram_below_threshold() {
        let tokens = vec![tok("hi"), tok("hi"), tok("done")];
        let out = collapse_repeats(&tokens);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn collapse_in_text_preserves_unrepeated() {
        assert_eq!(collapse_in_text("alpha beta gamma"), "alpha beta gamma");
    }

    #[test]
    fn collapse_in_text_returns_short_input_unchanged() {
        assert_eq!(collapse_in_text(""), "");
        assert_eq!(collapse_in_text("solo"), "solo");
    }

    #[test]
    fn collapse_repeats_handles_short_input() {
        assert!(collapse_repeats(&[]).is_empty());
        let single = vec![tok("only")];
        assert_eq!(collapse_repeats(&single).len(), 1);
    }

    #[test]
    fn canon_normalises_unicode_case() {
        assert_eq!(canon("Ω"), canon("ω"));
    }

    fn toks(words: &[&str]) -> Vec<Token> {
        words.iter().copied().map(tok).collect()
    }

    fn texts(out: &[Token]) -> Vec<&str> {
        out.iter().map(|t| t.text.as_str()).collect()
    }

    #[test]
    fn bridged_collapses_kensington_repeat() {
        let words = "so we don't work with Kensington and Chelsea \
            we are Hammersmith and Fulham \
            so we don't work with Kensington and Chelsea \
            we will use this report";
        let input = toks(&words.split_whitespace().collect::<Vec<_>>());
        let out = collapse_bridged_repeats(&input);
        let joined = texts(&out).join(" ");
        assert_eq!(
            joined
                .matches("so we don't work with Kensington and Chelsea")
                .count(),
            1,
            "expected one copy of the phrase, got: {joined}"
        );
        assert!(
            joined.contains("Hammersmith and Fulham"),
            "bridge must be preserved, got: {joined}"
        );
        assert!(
            joined.ends_with("will use this report"),
            "tail must be preserved, got: {joined}"
        );
    }

    #[test]
    fn bridged_collapses_exams_repeat() {
        let words = "have exams coming up and in those exams you will \
            then have the opportunity to \
            have exams coming up and in those exams you will \
            opportunity to show what you know";
        let input = toks(&words.split_whitespace().collect::<Vec<_>>());
        let out = collapse_bridged_repeats(&input);
        let joined = texts(&out).join(" ");
        assert!(
            joined
                .matches("have exams coming up and in those exams you will")
                .count()
                == 1,
            "expected one copy of the exams phrase, got: {joined}"
        );
    }

    #[test]
    fn bridged_collapses_okay_i_will_give_it_to() {
        let words = "okay i will give it to you okay i will give it to windbreaker";
        let input = toks(&words.split_whitespace().collect::<Vec<_>>());
        let out = collapse_bridged_repeats(&input);
        let joined = texts(&out).join(" ");
        assert_eq!(joined.matches("i will give it to").count(), 1);
        assert!(joined.ends_with("windbreaker"));
    }

    #[test]
    fn bridged_preserves_distant_rhetorical_repeats() {
        let words = "your attitude will determine how well this goes \
            and many many other things in between to make this a long bridge \
            your attitude will determine how well this goes";
        let input = toks(&words.split_whitespace().collect::<Vec<_>>());
        let out = collapse_bridged_repeats(&input);
        assert_eq!(out.len(), input.len());
    }

    #[test]
    fn bridged_skips_stopword_only_ngrams() {
        let words = "i was on the way to the on the way to the shop";
        let input = toks(&words.split_whitespace().collect::<Vec<_>>());
        let out = collapse_bridged_repeats(&input);
        assert_eq!(out.len(), input.len());
    }

    #[test]
    fn bridged_handles_short_input() {
        let single = toks(&["solo"]);
        assert_eq!(collapse_bridged_repeats(&single).len(), 1);
        assert!(collapse_bridged_repeats(&[]).is_empty());
    }
}
