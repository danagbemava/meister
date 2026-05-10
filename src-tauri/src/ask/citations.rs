//! Verbatim and whitespace-tolerant matching of model-emitted citation quotes
//! against the transcript text.

use crate::ask::types::AskCitation;

/// Match each `quote` against `transcript`. Returns one `AskCitation` per
/// input quote, with byte offsets `[start, end)` into the transcript, or
/// `start: None, end: None` if no plausible location was found. The first
/// occurrence wins.
pub fn match_quotes(transcript: &str, quotes: &[String]) -> Vec<AskCitation> {
    quotes.iter().map(|q| match_one(transcript, q)).collect()
}

fn match_one(transcript: &str, quote: &str) -> AskCitation {
    let trimmed = quote.trim();
    if trimmed.is_empty() {
        return AskCitation { quote: quote.to_string(), start: None, end: None };
    }
    if let Some(start) = transcript.find(trimmed) {
        return AskCitation {
            quote: quote.to_string(),
            start: Some(start),
            end: Some(start + trimmed.len()),
        };
    }
    if let Some((start, end)) = fuzzy_match(transcript, trimmed) {
        return AskCitation { quote: quote.to_string(), start: Some(start), end: Some(end) };
    }
    AskCitation { quote: quote.to_string(), start: None, end: None }
}

/// Whitespace- and quote-style-tolerant search.
///
/// Normalizes both the transcript and the needle (collapse all whitespace
/// runs to a single space, fold curly quotes to straight, lowercase ASCII)
/// while remembering the byte position in the original transcript that each
/// normalized character came from. A successful normalized substring search
/// then maps back to original `[start, end)`.
fn fuzzy_match(transcript: &str, needle: &str) -> Option<(usize, usize)> {
    let (norm_t, map) = normalize_with_map(transcript);
    let norm_n = normalize_string(needle);
    if norm_n.is_empty() { return None; }
    let pos = norm_t.find(&norm_n)?;
    let end = pos + norm_n.len();
    // map[i] is the byte offset in `transcript` of normalized char `i`.
    // For end, look up the byte AFTER the last matched normalized byte.
    let start_byte = map[pos];
    let end_byte = if end < map.len() { map[end] } else { transcript.len() };
    Some((start_byte, end_byte))
}

fn normalize_string(s: &str) -> String {
    let (n, _) = normalize_with_map(s);
    n
}

fn normalize_with_map(s: &str) -> (String, Vec<usize>) {
    let mut out = String::with_capacity(s.len());
    let mut map: Vec<usize> = Vec::with_capacity(s.len());
    let mut prev_was_space = true; // suppress leading whitespace
    for (idx, ch) in s.char_indices() {
        let folded = match ch {
            '\u{2018}' | '\u{2019}' | '\u{201A}' | '\u{2032}' => '\'',
            '\u{201C}' | '\u{201D}' | '\u{201E}' | '\u{2033}' => '"',
            c if c.is_whitespace() => ' ',
            c => c,
        };
        if folded == ' ' {
            if prev_was_space { continue; }
            prev_was_space = true;
            for b in 0..folded.len_utf8() {
                let _ = b;
                map.push(idx);
                out.push(' ');
            }
            continue;
        }
        prev_was_space = false;
        let lower = folded.to_ascii_lowercase();
        let mut tmp = [0u8; 4];
        let bytes = lower.encode_utf8(&mut tmp).as_bytes().to_vec();
        for _ in &bytes {
            map.push(idx);
        }
        out.push_str(std::str::from_utf8(&bytes).unwrap());
    }
    // Trailing space cleanup: drop a single trailing space if present.
    while out.ends_with(' ') {
        out.pop();
        map.pop();
    }
    (out, map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn quote(s: &str) -> Vec<String> { vec![s.to_string()] }

    #[test]
    fn exact_substring_matches() {
        let t = "hello world this is fine";
        let cites = match_quotes(t, &quote("world this"));
        assert_eq!(cites[0].start, Some(6));
        assert_eq!(cites[0].end, Some(16));
    }

    #[test]
    fn returns_first_occurrence() {
        let t = "yes yes yes";
        let cites = match_quotes(t, &quote("yes"));
        assert_eq!(cites[0].start, Some(0));
        assert_eq!(cites[0].end, Some(3));
    }

    #[test]
    fn whitespace_tolerant_match() {
        // Transcript has a hard newline; model echoes a single-space version.
        let t = "we delayed the\nQ3 budget review";
        let cites = match_quotes(t, &quote("we delayed the Q3 budget review"));
        assert!(cites[0].start.is_some());
        assert_eq!(cites[0].start, Some(0));
        let end = cites[0].end.unwrap();
        assert_eq!(&t[..end], t);
    }

    #[test]
    fn curly_quote_tolerant_match() {
        let t = "she said \u{201C}ship it\u{201D} firmly";
        let cites = match_quotes(t, &quote(r#"she said "ship it" firmly"#));
        assert!(cites[0].start.is_some(), "fuzzy match should fold curly quotes");
    }

    #[test]
    fn no_match_returns_none_offsets() {
        let cites = match_quotes("hello world", &quote("nothing relevant"));
        assert_eq!(cites[0].start, None);
        assert_eq!(cites[0].end, None);
    }

    #[test]
    fn empty_quote_returns_none() {
        let cites = match_quotes("hello", &quote(""));
        assert_eq!(cites[0].start, None);
        assert_eq!(cites[0].end, None);
    }

    #[test]
    fn quote_longer_than_transcript_returns_none() {
        let cites = match_quotes("hi", &quote("hi there fine thanks"));
        assert_eq!(cites[0].start, None);
    }

    #[test]
    fn multiple_quotes_each_resolved_independently() {
        let t = "alpha beta gamma";
        let q = vec!["alpha".into(), "missing".into(), "gamma".into()];
        let cites = match_quotes(t, &q);
        assert_eq!(cites[0].start, Some(0));
        assert_eq!(cites[1].start, None);
        assert_eq!(cites[2].start, Some(11));
    }

    #[test]
    fn trims_surrounding_whitespace_before_searching() {
        let t = "alpha beta gamma";
        let cites = match_quotes(t, &quote("  beta  "));
        assert_eq!(cites[0].start, Some(6));
        assert_eq!(cites[0].end, Some(10));
    }
}
