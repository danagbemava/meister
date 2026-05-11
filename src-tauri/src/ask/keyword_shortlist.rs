//! Pure-Rust keyword-overlap prefilter for path B.
//!
//! Scores each chunk by how many distinct non-stopword tokens from the question
//! also appear in the chunk (case-insensitive). Returns the top `k` chunks with
//! score > 0, in `(original_index, chunk_text)` order. If no chunk scores above
//! zero, returns an empty Vec — the caller falls back to the full chunk list.

use std::collections::HashSet;

/// Common English stopwords that should not contribute to scoring. Intentionally
/// short — we want to filter only words so common they add no signal.
const STOPWORDS: &[&str] = &[
    "a", "an", "the", "and", "or", "but", "if", "to", "of", "in", "on", "at",
    "for", "with", "by", "from", "as", "is", "was", "are", "were", "be", "been",
    "being", "have", "has", "had", "do", "does", "did", "will", "would", "should",
    "could", "may", "might", "must", "can", "this", "that", "these", "those",
    "i", "you", "he", "she", "it", "we", "they", "me", "him", "her", "us", "them",
    "what", "when", "where", "who", "why", "how", "which",
    "about", "into", "than", "then", "so", "not", "no",
];

fn tokenize(s: &str) -> Vec<String> {
    s.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .map(|w| w.to_string())
        .collect()
}

fn question_terms(question: &str) -> HashSet<String> {
    let stop: HashSet<&str> = STOPWORDS.iter().copied().collect();
    tokenize(question)
        .into_iter()
        .filter(|t| !stop.contains(t.as_str()))
        .collect()
}

/// Score = number of distinct question terms that appear in the chunk.
fn score_chunk(terms: &HashSet<String>, chunk: &str) -> usize {
    if terms.is_empty() {
        return 0;
    }
    let chunk_lower = chunk.to_lowercase();
    terms
        .iter()
        .filter(|t| chunk_lower.contains(t.as_str()))
        .count()
}

/// Top-K shortlist. Returns `(original_index, chunk.clone())` tuples for the
/// top-scoring `k` chunks with score > 0, in descending score order. Ties are
/// broken by original index (earlier wins). Empty if no chunk overlaps.
pub fn keyword_shortlist(chunks: &[String], question: &str, k: usize) -> Vec<(usize, String)> {
    let terms = question_terms(question);
    if terms.is_empty() {
        return Vec::new();
    }
    let mut scored: Vec<(usize, usize, &String)> = chunks
        .iter()
        .enumerate()
        .map(|(i, c)| (i, score_chunk(&terms, c), c))
        .filter(|(_, s, _)| *s > 0)
        .collect();
    // Sort by score desc, then original index asc.
    scored.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    scored
        .into_iter()
        .take(k)
        .map(|(i, _, c)| (i, c.clone()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_empty_when_question_is_only_stopwords() {
        let chunks = vec!["alpha beta".to_string(), "gamma delta".to_string()];
        let out = keyword_shortlist(&chunks, "what is the?", 5);
        assert!(out.is_empty());
    }

    #[test]
    fn returns_empty_when_no_chunk_overlaps() {
        let chunks = vec!["alpha beta".to_string(), "gamma delta".to_string()];
        let out = keyword_shortlist(&chunks, "epsilon zeta?", 5);
        assert!(out.is_empty());
    }

    #[test]
    fn picks_highest_overlap_chunk_first() {
        let chunks = vec![
            "we talked about marketing briefly".to_string(),                          // 1 term: marketing
            "the financials and budget were the main topic".to_string(),              // 2 terms: financials, budget
            "marketing budget financials were all deferred to Q4".to_string(),        // 3 terms: marketing, budget, financials
        ];
        let out = keyword_shortlist(&chunks, "what about the budget and marketing financials?", 5);
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].0, 2, "chunk with 3 matching terms should come first");
        assert_eq!(out[1].0, 1, "chunk with 2 matching terms should come second");
        assert_eq!(out[2].0, 0, "chunk with 1 matching term should come third");
    }

    #[test]
    fn caps_at_k() {
        let chunks: Vec<String> = (0..10)
            .map(|i| format!("budget item number {i}"))
            .collect();
        let out = keyword_shortlist(&chunks, "budget?", 3);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn ties_break_by_original_index() {
        let chunks = vec![
            "budget".to_string(),
            "budget".to_string(),
            "budget".to_string(),
        ];
        let out = keyword_shortlist(&chunks, "budget?", 5);
        assert_eq!(out.iter().map(|(i, _)| *i).collect::<Vec<_>>(), vec![0, 1, 2]);
    }

    #[test]
    fn is_case_insensitive() {
        let chunks = vec!["The BUDGET was reviewed".to_string()];
        let out = keyword_shortlist(&chunks, "budget", 5);
        assert_eq!(out.len(), 1);
    }

    #[test]
    fn ignores_stopwords_in_question() {
        // "the" is a stopword; only "budget" should count.
        let chunks = vec!["budget".to_string(), "the the the".to_string()];
        let out = keyword_shortlist(&chunks, "the budget", 5);
        assert_eq!(out.len(), 1);
        assert_eq!(out[0].0, 0);
    }
}
