//! Prompt construction and answer parsing for the Ask feature.

use crate::ask::types::AskTurnIn;

const CITATION_MARKER: &str = "--- citations ---";

/// Builds the answer prompt for path A and the reduce step of path B.
///
/// Includes any prior conversation `history` inline so the model has context
/// for follow-up questions.
pub fn build_answer_prompt(transcript_or_chunks: &str, question: &str, history: &[AskTurnIn]) -> String {
    let mut history_block = String::new();
    if !history.is_empty() {
        history_block.push_str("Previous conversation:\n");
        for turn in history {
            let label = match turn.role.as_str() {
                "user" => "User",
                "assistant" => "Assistant",
                _ => "User",
            };
            history_block.push_str(&format!("{label}: {}\n", turn.content.trim()));
        }
        history_block.push('\n');
    }

    format!(
        r#"You are answering a question about a transcript. Use ONLY the transcript below — never invent facts. If the transcript doesn't contain the answer, say so plainly.

For every factual claim in your answer, quote verbatim from the transcript.

After the prose answer, output the literal marker on its own line:

{marker}

Then list each verbatim quote you used, one per line, copied EXACTLY as it appears in the transcript (including any punctuation and casing). Do not paraphrase the citations.

{history}TRANSCRIPT:
"""
{transcript}
"""

QUESTION: {question}

Answer now:"#,
        marker = CITATION_MARKER,
        history = history_block,
        transcript = transcript_or_chunks,
        question = question,
    )
}

/// Builds the per-chunk relevance prompt for path B's map pass.
pub fn build_relevance_prompt(chunk: &str, index: usize, total: usize, question: &str) -> String {
    format!(
        r#"You are scanning ONE SEGMENT of a longer transcript (segment {n} of {total}) to decide whether it is relevant to a user's question.

Reply with a single JSON object on one line, with two fields:
- "relevant": true or false
- "gist": one short sentence summarizing what the segment says about the question (or "" if not relevant).

Do not output anything else — no explanation, no markdown.

QUESTION: {question}

SEGMENT:
"""
{chunk}
"""

JSON:"#,
        n = index + 1,
        total = total,
        question = question,
        chunk = chunk,
    )
}

/// Splits a model response on the citation marker. Returns the prose answer
/// (left of marker) and a list of citation lines (right of marker, each line
/// non-empty and trimmed). If the marker is absent, returns `(response, [])`.
pub fn parse_answer(response: &str) -> (String, Vec<String>) {
    match response.split_once(CITATION_MARKER) {
        Some((answer, cites_block)) => {
            let cites = cites_block
                .lines()
                .map(|l| l.trim())
                .filter(|l| !l.is_empty())
                .map(|l| strip_quotes(l).to_string())
                .collect();
            (answer.trim_end().to_string(), cites)
        }
        None => (response.trim().to_string(), Vec::new()),
    }
}

fn strip_quotes(s: &str) -> &str {
    let s = s.trim();
    let s = s.strip_prefix('"').unwrap_or(s);
    let s = s.strip_suffix('"').unwrap_or(s);
    s
}

#[derive(Debug, PartialEq, Eq)]
pub struct RelevanceVerdict {
    pub relevant: bool,
    pub gist: String,
}

/// Best-effort parse of a one-line JSON verdict from the relevance pass.
/// On any parse failure, falls back to `relevant: false` so the chunk is
/// excluded from the reduce pass — safer than including suspect content.
pub fn parse_relevance(s: &str) -> RelevanceVerdict {
    // Find the first `{` and last `}` to tolerate stray prefix/suffix text.
    let start = s.find('{');
    let end = s.rfind('}');
    let json = match (start, end) {
        (Some(a), Some(b)) if b >= a => &s[a..=b],
        _ => return RelevanceVerdict { relevant: false, gist: String::new() },
    };
    let v: serde_json::Value = match serde_json::from_str(json) {
        Ok(v) => v,
        Err(_) => return RelevanceVerdict { relevant: false, gist: String::new() },
    };
    RelevanceVerdict {
        relevant: v.get("relevant").and_then(|x| x.as_bool()).unwrap_or(false),
        gist: v.get("gist").and_then(|x| x.as_str()).unwrap_or("").to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_answer_prompt_includes_transcript_and_question() {
        let p = build_answer_prompt("the only line", "what is it?", &[]);
        assert!(p.contains("the only line"));
        assert!(p.contains("what is it?"));
        assert!(p.contains(CITATION_MARKER));
        assert!(!p.contains("Previous conversation"));
    }

    #[test]
    fn build_answer_prompt_appends_history() {
        let history = vec![
            AskTurnIn { role: "user".into(), content: "first q".into() },
            AskTurnIn { role: "assistant".into(), content: "first a".into() },
        ];
        let p = build_answer_prompt("t", "follow up?", &history);
        assert!(p.contains("Previous conversation:"));
        assert!(p.contains("User: first q"));
        assert!(p.contains("Assistant: first a"));
    }

    #[test]
    fn build_relevance_prompt_includes_chunk_and_question() {
        let p = build_relevance_prompt("seg body", 0, 3, "Q?");
        assert!(p.contains("segment 1 of 3"));
        assert!(p.contains("seg body"));
        assert!(p.contains("Q?"));
        assert!(p.contains("\"relevant\""));
    }

    #[test]
    fn parse_answer_splits_on_marker() {
        let resp = "the answer text here.\n\n--- citations ---\n\"first quote\"\n\"second quote\"\n";
        let (text, cites) = parse_answer(resp);
        assert_eq!(text, "the answer text here.");
        assert_eq!(cites, vec!["first quote", "second quote"]);
    }

    #[test]
    fn parse_answer_returns_empty_citations_when_marker_absent() {
        let (text, cites) = parse_answer("plain answer with no citations");
        assert_eq!(text, "plain answer with no citations");
        assert!(cites.is_empty());
    }

    #[test]
    fn parse_answer_strips_blank_lines_in_citation_block() {
        let resp = "answer\n--- citations ---\n\n\"q1\"\n\n\n\"q2\"\n\n";
        let (_, cites) = parse_answer(resp);
        assert_eq!(cites, vec!["q1", "q2"]);
    }

    #[test]
    fn parse_relevance_handles_clean_json() {
        let v = parse_relevance(r#"{"relevant": true, "gist": "discusses Q3 budget"}"#);
        assert_eq!(v, RelevanceVerdict { relevant: true, gist: "discusses Q3 budget".into() });
    }

    #[test]
    fn parse_relevance_falls_back_to_false_on_garbage() {
        let v = parse_relevance("yes definitely relevant!!");
        assert_eq!(v.relevant, false);
        assert_eq!(v.gist, "");
    }

    #[test]
    fn parse_relevance_tolerates_surrounding_whitespace_or_text() {
        let v = parse_relevance("Sure thing: {\"relevant\": false, \"gist\": \"\"} done.");
        assert_eq!(v.relevant, false);
    }
}
