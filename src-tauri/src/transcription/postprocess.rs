//! Post-processing of whisper segment output: collapses lock-in loops
//! and drops known hallucinated boilerplate.

use std::sync::OnceLock;

/// Runs of ≥ REPETITION_THRESHOLD consecutive segments that normalize to
/// the same string collapse to a single surviving segment (the first);
/// shorter runs pass through unchanged. Two-in-a-row is plausibly
/// intentional emphasis ("I mean it. I mean it."); three or more is the
/// unmistakable shape of a whisper lock-in loop.
const REPETITION_THRESHOLD: usize = 3;

pub(crate) fn collapse_repetitions(segments: &[String]) -> Vec<String> {
    if segments.is_empty() {
        return Vec::new();
    }

    // Two-pass: first identify maximal runs, then emit keeping at most one
    // element per run that reaches REPETITION_THRESHOLD.
    let norms: Vec<String> = segments.iter().map(|s| normalize(s)).collect();
    let n = norms.len();

    // run_len[i] = length of the maximal run that contains index i.
    let mut run_len = vec![0usize; n];
    let mut i = 0;
    while i < n {
        let mut j = i + 1;
        while j < n && norms[j] == norms[i] {
            j += 1;
        }
        let len = j - i;
        for k in i..j {
            run_len[k] = len;
        }
        i = j;
    }

    // Emit: if run length < THRESHOLD, keep all; otherwise keep only the
    // first element of the run.
    let mut out: Vec<String> = Vec::with_capacity(n);
    let mut i = 0;
    while i < n {
        let len = run_len[i];
        if len < REPETITION_THRESHOLD {
            for k in i..i + len {
                out.push(segments[k].clone());
            }
        } else {
            out.push(segments[i].clone());
        }
        i += len;
    }
    out
}

/// Lowercase, trim, collapse internal whitespace, strip a single run of
/// trailing punctuation (.,!?…). Matches "Yes." and "yes" as equal.
fn normalize(s: &str) -> String {
    let trimmed = s.trim();
    let mut collapsed = String::with_capacity(trimmed.len());
    let mut prev_ws = false;
    for ch in trimmed.chars() {
        if ch.is_whitespace() {
            if !prev_ws { collapsed.push(' '); }
            prev_ws = true;
        } else {
            collapsed.push(ch.to_ascii_lowercase());
            prev_ws = false;
        }
    }
    let mut result: &str = &collapsed;
    while let Some(last) = result.chars().next_back() {
        if matches!(last, '.' | ',' | '!' | '?' | '…') {
            result = &result[..result.len() - last.len_utf8()];
        } else {
            break;
        }
    }
    result.to_string()
}

const LITERAL_BLOCKLIST: &[&str] = &[
    "thank you for watching",
    "thanks for watching",
    "please subscribe",
    "like and subscribe",
    "don't forget to subscribe",
    "see you in the next video",
    "see you next time",
    "[music]",
    "[applause]",
    "[laughter]",
    "♪",
    "ご視聴ありがとうございました",
    "字幕 by",
    "translated by",
];

fn full_segment_patterns() -> &'static [regex::Regex] {
    static PATTERNS: OnceLock<Vec<regex::Regex>> = OnceLock::new();
    PATTERNS.get_or_init(|| {
        [
            r"^(?i)(you|thank you|thanks)[.!\s]*$",
            r"^\s*\.*\s*$",
            r"^\[?\s*(?i)inaudible\s*\]?$",
        ]
        .iter()
        .map(|p| regex::Regex::new(p).expect("hallucination pattern should compile"))
        .collect()
    })
}

/// True if a segment matches any known hallucination pattern. Matched
/// case-insensitively against the trimmed segment. Literal entries are
/// substring matches; regex entries are full-segment matches.
pub(crate) fn is_hallucination(segment: &str) -> bool {
    let trimmed = segment.trim();
    if trimmed.is_empty() {
        return true;
    }
    let lower = trimmed.to_lowercase();
    if LITERAL_BLOCKLIST.iter().any(|needle| lower.contains(needle)) {
        return true;
    }
    full_segment_patterns().iter().any(|re| re.is_match(trimmed))
}

pub(crate) fn filter_hallucinations(segments: &[String]) -> Vec<String> {
    segments
        .iter()
        .filter(|s| !is_hallucination(s))
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn segs(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|s| (*s).to_string()).collect()
    }

    #[test]
    fn collapse_triple_run_to_one() {
        let input = segs(&["A", "A", "A", "B"]);
        let out = collapse_repetitions(&input);
        assert_eq!(out, segs(&["A", "B"]));
    }

    #[test]
    fn collapse_keeps_doubles_as_emphasis() {
        let input = segs(&["A", "A", "B"]);
        let out = collapse_repetitions(&input);
        assert_eq!(out, input);
    }

    #[test]
    fn collapse_long_run_to_one() {
        let input: Vec<String> = std::iter::repeat("hello".to_string()).take(10).collect();
        let out = collapse_repetitions(&input);
        assert_eq!(out, segs(&["hello"]));
    }

    #[test]
    fn collapse_preserves_alternation() {
        let input = segs(&["A", "B", "A", "B", "A", "B"]);
        let out = collapse_repetitions(&input);
        assert_eq!(out, input);
    }

    #[test]
    fn collapse_interleaved_runs() {
        // A A A B A A A A → A B A
        let input = segs(&["A", "A", "A", "B", "A", "A", "A", "A"]);
        let out = collapse_repetitions(&input);
        assert_eq!(out, segs(&["A", "B", "A"]));
    }

    #[test]
    fn collapse_normalizes_case_and_punctuation() {
        // "Yes." and "yes" should be considered equal for run detection.
        // A 4-run of yes-normalized entries collapses to 1; "no." follows.
        let input = segs(&["Yes.", "yes", "YES", "yes!", "no."]);
        let out = collapse_repetitions(&input);
        assert_eq!(out, segs(&["Yes.", "no."]));
    }

    #[test]
    fn collapse_iron_rod_fixture() {
        // Reproduces the real-world failure: 23 copies of one loop
        // followed by 34 copies of another. Expected: 2 surviving segments.
        let mut input: Vec<String> = Vec::new();
        for _ in 0..23 {
            input.push("And the iron rod and cement are growing up.".to_string());
        }
        for _ in 0..34 {
            input.push("I think it's a little different.".to_string());
        }
        let out = collapse_repetitions(&input);
        assert_eq!(out, segs(&[
            "And the iron rod and cement are growing up.",
            "I think it's a little different.",
        ]));
    }

    #[test]
    fn collapse_empty_input_returns_empty() {
        let out = collapse_repetitions(&[]);
        assert!(out.is_empty());
    }

    #[test]
    fn hallucination_matches_youtube_boilerplate() {
        assert!(is_hallucination("Thank you for watching!"));
        assert!(is_hallucination("thanks for watching"));
        assert!(is_hallucination("Please subscribe and hit the bell."));
        assert!(is_hallucination("like and subscribe"));
        assert!(is_hallucination("Don't forget to subscribe."));
        assert!(is_hallucination("See you in the next video."));
        assert!(is_hallucination("See you next time."));
    }

    #[test]
    fn hallucination_matches_bracket_markers() {
        assert!(is_hallucination("[Music]"));
        assert!(is_hallucination("[MUSIC]"));
        assert!(is_hallucination("[Applause]"));
        assert!(is_hallucination("[Laughter]"));
        assert!(is_hallucination("♪"));
    }

    #[test]
    fn hallucination_matches_non_english_boilerplate() {
        assert!(is_hallucination("ご視聴ありがとうございました"));
        assert!(is_hallucination("字幕 by someone"));
        assert!(is_hallucination("Translated by fan-sub team"));
    }

    #[test]
    fn hallucination_matches_short_forms() {
        assert!(is_hallucination("you"));
        assert!(is_hallucination("You."));
        assert!(is_hallucination("Thank you."));
        assert!(is_hallucination("Thanks."));
        assert!(is_hallucination(""));
        assert!(is_hallucination("   "));
        assert!(is_hallucination("..."));
        assert!(is_hallucination("[inaudible]"));
        assert!(is_hallucination("inaudible"));
    }

    #[test]
    fn hallucination_preserves_legit_content() {
        assert!(!is_hallucination("I think we should ship next week."));
        assert!(!is_hallucination("Thanks, John."));
        assert!(!is_hallucination("This is a video about Kubernetes."));
        assert!(!is_hallucination("[inaudible speech at 2:34]"));
        assert!(!is_hallucination("Yes, exactly."));
        assert!(!is_hallucination("You should try the new API."));
    }

    #[test]
    fn filter_drops_hallucinations_preserves_order() {
        let input = segs(&[
            "Good morning.",
            "[Music]",
            "Let's start the review.",
            "Thank you for watching.",
            "Next item on the agenda.",
        ]);
        let out = filter_hallucinations(&input);
        assert_eq!(out, segs(&[
            "Good morning.",
            "Let's start the review.",
            "Next item on the agenda.",
        ]));
    }

    #[test]
    fn compound_collapse_then_filter_real_sample() {
        let mut input: Vec<String> = Vec::new();
        for _ in 0..23 {
            input.push("And the iron rod and cement are growing up.".to_string());
        }
        input.push("Thank you for watching!".to_string());
        for _ in 0..34 {
            input.push("I think it's a little different.".to_string());
        }
        let collapsed = collapse_repetitions(&input);
        let out = filter_hallucinations(&collapsed);
        assert_eq!(out, segs(&[
            "And the iron rod and cement are growing up.",
            "I think it's a little different.",
        ]));
    }
}
