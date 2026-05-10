//! Path B: map (relevance shortlist) + reduce (single answer over the
//! relevant chunks). Map runs sequentially — Ollama is single-tenant in
//! practice and parallel relevance probes risk model swapping.

use crate::ask::citations::match_quotes;
use crate::ask::prompts::{build_answer_prompt, build_relevance_prompt, parse_answer, parse_relevance};
use crate::ask::types::{AskAnswer, AskTurnIn};
use crate::cancellation::CancelToken;
use crate::commands::{ollama_stream, preprocess_pub as preprocess, split_into_chunks_pub as split_into_chunks};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Phase {
    Search { index: usize, total: usize },
    Answer { chars: usize },
}

pub async fn run_map_shortlist(
    ollama_url: &str,
    model: &str,
    transcript: &str,
    question: &str,
    history: &[AskTurnIn],
    on_phase: &mut (dyn FnMut(Phase) + Send),
    cancel: Option<&CancelToken>,
) -> Result<AskAnswer, String> {
    let cleaned = preprocess(transcript);
    let chunks = split_into_chunks(&cleaned);
    let total = chunks.len();

    let mut relevant: Vec<(usize, String)> = Vec::new();
    for (idx, chunk) in chunks.iter().enumerate() {
        on_phase(Phase::Search { index: idx, total });
        let prompt = build_relevance_prompt(chunk, idx, total, question);
        let mut sink = |_: &str| {};
        let raw = ollama_stream(ollama_url, model, prompt, 4096, &mut sink, cancel).await?;
        let verdict = parse_relevance(&raw);
        if verdict.relevant {
            relevant.push((idx, chunk.clone()));
        }
    }

    if relevant.is_empty() {
        return Ok(AskAnswer {
            text: "I couldn't find anything about that in the transcript.".to_string(),
            citations: Vec::new(),
        });
    }

    let mut joined = String::new();
    for (idx, chunk) in &relevant {
        joined.push_str(&format!("[chunk {}]\n", idx + 1));
        joined.push_str(chunk);
        joined.push_str("\n\n");
    }

    let prompt = build_answer_prompt(joined.trim_end(), question, history);
    let mut total_chars: usize = 0;
    let mut on_token = |piece: &str| {
        total_chars += piece.chars().count();
        on_phase(Phase::Answer { chars: total_chars });
    };
    let raw = ollama_stream(ollama_url, model, prompt, 8192, &mut on_token, cancel).await?;
    let (text, quotes) = parse_answer(&raw);
    // Match against the ORIGINAL (unprocessed) transcript so click-to-scroll
    // lands on real text. Some quotes may not match because preprocess
    // removed fillers — that's acceptable; the citation just renders as
    // unclickable.
    let citations = match_quotes(transcript, &quotes);
    Ok(AskAnswer { text, citations })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockOllama;

    fn long_transcript() -> String {
        // Designed to split into multiple chunks. THRESHOLD = 4000 tokens × 4
        // chars/token = 16k chars; we go well over.
        let sentence = "This is a long sentence with words. ";
        sentence.repeat(800)
    }

    #[tokio::test]
    async fn relevant_chunks_only_reach_reduce() {
        let transcript = long_transcript();
        let mock = MockOllama::spawn(|body| {
            let prompt = body["prompt"].as_str().unwrap_or("");
            if prompt.contains("Reply with a single JSON object") {
                // Mark first chunk relevant, rest not.
                if prompt.contains("segment 1 of") {
                    r#"{"relevant": true, "gist": "the answer"}"#.to_string()
                } else {
                    r#"{"relevant": false, "gist": ""}"#.to_string()
                }
            } else {
                "Reduced answer.\n--- citations ---\n\"This is a long sentence with words.\"\n".to_string()
            }
        }).await;

        let ans = run_map_shortlist(
            &mock.base_url, "gemma3", &transcript, "what?", &[],
            &mut |_| {}, None,
        ).await.unwrap();

        assert!(ans.text.contains("Reduced answer"));
        let calls = mock.calls.lock().unwrap();
        // N relevance + 1 reduce. We can't pin N exactly without re-running
        // split_into_chunks, but it's >= 2 (multiple chunks) and the LAST
        // call must be the reduce.
        assert!(calls.len() >= 3);
        let last = calls.last().unwrap();
        let last_prompt = last["prompt"].as_str().unwrap();
        assert!(last_prompt.contains("--- citations ---") || last_prompt.contains("QUESTION:"));
        assert!(last_prompt.contains("[chunk"));
        // Reduce must NOT include the irrelevant chunks: only one [chunk N]
        // header should appear in the joined transcript section.
        let header_count = last_prompt.matches("[chunk ").count();
        assert_eq!(header_count, 1, "reduce should see only relevant chunks");
    }

    #[tokio::test]
    async fn zero_relevant_chunks_short_circuits_without_reduce() {
        let transcript = long_transcript();
        let mock = MockOllama::spawn(|body| {
            let prompt = body["prompt"].as_str().unwrap_or("");
            if prompt.contains("Reply with a single JSON object") {
                r#"{"relevant": false, "gist": ""}"#.to_string()
            } else {
                panic!("reduce pass should never run when nothing is relevant")
            }
        }).await;

        let ans = run_map_shortlist(
            &mock.base_url, "gemma3", &transcript, "obscure?", &[],
            &mut |_| {}, None,
        ).await.unwrap();

        assert!(ans.text.to_lowercase().contains("couldn't find"));
        assert!(ans.citations.is_empty());
    }

    #[tokio::test]
    async fn emits_search_phase_per_chunk_then_answer_phase() {
        let transcript = long_transcript();
        let mock = MockOllama::spawn(|body| {
            let prompt = body["prompt"].as_str().unwrap_or("");
            if prompt.contains("Reply with a single JSON object") {
                r#"{"relevant": true, "gist": ""}"#.to_string()
            } else {
                "ok.\n--- citations ---\n".to_string()
            }
        }).await;

        let phases: std::sync::Arc<std::sync::Mutex<Vec<Phase>>> =
            std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        let phases_cb = phases.clone();
        let mut on_phase = move |p: Phase| phases_cb.lock().unwrap().push(p);

        let _ = run_map_shortlist(
            &mock.base_url, "gemma3", &transcript, "any?", &[],
            &mut on_phase, None,
        ).await.unwrap();

        let phases = phases.lock().unwrap();
        let search_count = phases.iter().filter(|p| matches!(p, Phase::Search { .. })).count();
        let answer_count = phases.iter().filter(|p| matches!(p, Phase::Answer { .. })).count();
        assert!(search_count >= 2, "expected one Search phase per chunk, got {search_count}");
        assert!(answer_count >= 1, "expected at least one Answer phase, got {answer_count}");

        // Last Search phase index < total, and Answer comes after the searches.
        let last_search = phases.iter().rposition(|p| matches!(p, Phase::Search { .. })).unwrap();
        let first_answer = phases.iter().position(|p| matches!(p, Phase::Answer { .. })).unwrap();
        assert!(first_answer > last_search);
    }
}
