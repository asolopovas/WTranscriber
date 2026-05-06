use crate::transcriber::Token;

const MAX_NGRAM: usize = 5;
const MIN_RUN_BY_N1: usize = 4;
const MIN_RUN_BY_N2: usize = 3;
const MIN_RUN_BY_LARGE: usize = 2;

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
}
