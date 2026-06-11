use crate::transcriber::{
    dedup,
    transcript::{Segment, Token},
};

pub(super) fn apply_dedup(segments: &mut Vec<Segment>) {
    for seg in segments.iter_mut() {
        if seg.tokens.len() >= 2 {
            let before = seg.tokens.len();
            let collapsed = dedup::collapse_repeats(&seg.tokens);
            let bridged = dedup::collapse_bridged_repeats(&collapsed);
            if bridged.len() != before {
                seg.tokens = bridged;
                rebuild_from_tokens(seg);
            }
        } else if seg.tokens.is_empty() && !seg.text.trim().is_empty() {
            seg.text = dedup::collapse_in_text(seg.text.trim());
        }
    }
    collapse_across_segments(segments);
    segments.retain(|s| !s.tokens.is_empty() || !s.text.trim().is_empty());
}

fn collapse_across_segments(segments: &mut [Segment]) {
    let flat: Vec<Token> = segments
        .iter()
        .flat_map(|s| s.tokens.iter().cloned())
        .collect();
    if flat.len() < 2 {
        return;
    }
    let collapsed = dedup::collapse_bridged_repeats(&dedup::collapse_repeats(&flat));
    if collapsed.len() == flat.len() {
        return;
    }
    let mut keep = vec![false; flat.len()];
    let mut ci = 0;
    for (i, tok) in flat.iter().enumerate() {
        if ci < collapsed.len()
            && tok.text == collapsed[ci].text
            && tok.start_ms == collapsed[ci].start_ms
            && tok.end_ms == collapsed[ci].end_ms
        {
            keep[i] = true;
            ci += 1;
        }
    }
    if ci != collapsed.len() {
        return;
    }
    let mut idx = 0;
    for seg in segments.iter_mut() {
        let n = seg.tokens.len();
        if n == 0 {
            continue;
        }
        let kept: Vec<Token> = seg
            .tokens
            .drain(..)
            .enumerate()
            .filter_map(|(k, t)| keep[idx + k].then_some(t))
            .collect();
        idx += n;
        let changed = kept.len() != n;
        seg.tokens = kept;
        if changed {
            rebuild_from_tokens(seg);
        }
    }
}

pub(super) fn shift_segments(segments: &mut [Segment], offset_ms: u64) {
    if offset_ms == 0 {
        return;
    }
    for seg in segments.iter_mut() {
        seg.start_ms = seg.start_ms.saturating_add(offset_ms);
        seg.end_ms = seg.end_ms.saturating_add(offset_ms);
        for tok in &mut seg.tokens {
            tok.start_ms = tok.start_ms.saturating_add(offset_ms);
            tok.end_ms = tok.end_ms.saturating_add(offset_ms);
        }
    }
}

fn rebuild_from_tokens(seg: &mut Segment) {
    if seg.tokens.is_empty() {
        seg.text.clear();
        return;
    }
    seg.text = seg
        .tokens
        .iter()
        .map(|t| t.text.as_str())
        .collect::<Vec<_>>()
        .join(" ");
    let (Some(first), Some(last)) = (seg.tokens.first(), seg.tokens.last()) else {
        seg.text.clear();
        return;
    };
    seg.start_ms = first.start_ms;
    seg.end_ms = last.end_ms;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transcriber::transcript::Token;

    fn tok(text: &str, start: u64, end: u64) -> Token {
        Token {
            text: text.into(),
            start_ms: start,
            end_ms: end,
            confidence: 1.0,
        }
    }

    fn seg(text: &str, start: u64, end: u64, tokens: Vec<Token>) -> Segment {
        Segment {
            text: text.into(),
            start_ms: start,
            end_ms: end,
            tokens,
        }
    }

    #[test]
    fn shift_segments_no_op_when_offset_zero() {
        let mut segs = vec![seg("hi", 100, 200, vec![tok("hi", 100, 200)])];
        let before = segs.clone();
        shift_segments(&mut segs, 0);
        assert_eq!(segs[0].start_ms, before[0].start_ms);
        assert_eq!(segs[0].end_ms, before[0].end_ms);
        assert_eq!(segs[0].tokens[0].start_ms, before[0].tokens[0].start_ms);
    }

    #[test]
    fn shift_segments_adds_offset_to_segments_and_tokens() {
        let mut segs = vec![seg("x", 10, 20, vec![tok("x", 10, 15), tok("y", 16, 20)])];
        shift_segments(&mut segs, 1_000);
        assert_eq!(segs[0].start_ms, 1_010);
        assert_eq!(segs[0].end_ms, 1_020);
        assert_eq!(segs[0].tokens[0].start_ms, 1_010);
        assert_eq!(segs[0].tokens[1].end_ms, 1_020);
    }

    #[test]
    fn shift_segments_saturates_at_u64_max() {
        let mut segs = vec![seg("x", u64::MAX - 5, u64::MAX - 1, vec![])];
        shift_segments(&mut segs, 1_000);
        assert_eq!(segs[0].start_ms, u64::MAX);
        assert_eq!(segs[0].end_ms, u64::MAX);
    }

    #[test]
    fn rebuild_from_tokens_clears_when_empty() {
        let mut s = seg("stale", 100, 200, vec![]);
        rebuild_from_tokens(&mut s);
        assert!(s.text.is_empty());
    }

    #[test]
    fn rebuild_from_tokens_recomputes_bounds_and_text() {
        let mut s = seg(
            "old",
            999,
            999,
            vec![tok("hello", 10, 20), tok("world", 21, 30)],
        );
        rebuild_from_tokens(&mut s);
        assert_eq!(s.text, "hello world");
        assert_eq!(s.start_ms, 10);
        assert_eq!(s.end_ms, 30);
    }

    #[test]
    fn apply_dedup_removes_empty_segments() {
        let mut segs = vec![
            seg("", 0, 0, vec![]),
            seg("ok", 10, 20, vec![tok("ok", 10, 20)]),
            seg("   ", 30, 40, vec![]),
        ];
        apply_dedup(&mut segs);
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "ok");
    }

    #[test]
    fn apply_dedup_collapses_token_repeats_and_rebuilds() {
        let mut segs = vec![seg(
            "the the the the",
            100,
            500,
            vec![
                tok("the", 100, 200),
                tok("the", 200, 300),
                tok("the", 300, 400),
                tok("the", 400, 500),
            ],
        )];
        apply_dedup(&mut segs);
        assert_eq!(segs.len(), 1);

        assert_eq!(segs[0].tokens.len(), 1);
        assert_eq!(segs[0].text, "the");
        assert_eq!(segs[0].start_ms, 100);
        assert_eq!(segs[0].end_ms, 200);
    }

    #[test]
    fn apply_dedup_collapses_word_per_segment_repetition_loops() {
        let mut segs: Vec<Segment> = Vec::new();
        for run in 0..6u64 {
            let base = run * 1_000;
            segs.push(seg(
                "thank",
                base,
                base + 400,
                vec![tok("thank", base, base + 400)],
            ));
            segs.push(seg(
                "you.",
                base + 500,
                base + 900,
                vec![tok("you.", base + 500, base + 900)],
            ));
        }
        apply_dedup(&mut segs);
        assert_eq!(segs.len(), 2, "loop should collapse to one thank/you pair");
        assert_eq!(segs[0].text, "thank");
        assert_eq!(segs[1].text, "you.");
    }

    #[test]
    fn apply_dedup_keeps_distinct_word_segments() {
        let mut segs = vec![
            seg("the", 0, 100, vec![tok("the", 0, 100)]),
            seg("quick", 110, 200, vec![tok("quick", 110, 200)]),
            seg("brown", 210, 300, vec![tok("brown", 210, 300)]),
            seg("fox", 310, 400, vec![tok("fox", 310, 400)]),
        ];
        apply_dedup(&mut segs);
        assert_eq!(segs.len(), 4);
    }

    #[test]
    fn apply_dedup_collapses_in_plain_text_when_no_tokens() {
        let mut segs = vec![seg("hello hello hello hello world", 0, 100, vec![])];
        apply_dedup(&mut segs);
        assert_eq!(segs.len(), 1);
        assert!(
            !segs[0].text.contains("hello hello"),
            "dedup should leave at most one 'hello' run, got {:?}",
            segs[0].text
        );
    }
}
