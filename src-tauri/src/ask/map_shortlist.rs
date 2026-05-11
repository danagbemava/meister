//! Path B: keyword-prefilter + map (relevance) + reduce. The map pass runs
//! over a top-K shortlist of chunks identified by `keyword_shortlist`, with
//! bounded concurrency (`buffer_unordered`). The reduce pass is unchanged.

use std::sync::{Arc, Mutex};

use futures_util::{stream, StreamExt};

use crate::ask::citations::match_quotes;
use crate::ask::keyword_shortlist::keyword_shortlist;
use crate::ask::prompts::{build_answer_prompt, build_relevance_prompt, parse_answer, parse_relevance};
use crate::ask::types::{AskAnswer, AskTurnIn};
use crate::cancellation::CancelToken;
use crate::commands::{ollama_stream, preprocess_pub as preprocess, split_into_chunks_pub as split_into_chunks};

const SHORTLIST_K: usize = 5;
const MAP_CONCURRENCY: usize = 3;

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

    // 1. Keyword prefilter. Fall back to all chunks if nothing overlaps.
    let mut shortlist = keyword_shortlist(&chunks, question, SHORTLIST_K);
    if shortlist.is_empty() {
        shortlist = chunks.iter().cloned().enumerate().collect();
    }
    let llm_total = shortlist.len();

    // 2. Bounded-parallel map. `on_phase` lives on the caller's stack; route
    //    its invocations through a Mutex so the unordered futures can call it.
    let on_phase_arc: Arc<Mutex<&mut (dyn FnMut(Phase) + Send)>> =
        Arc::new(Mutex::new(on_phase));
    let completed = Arc::new(std::sync::atomic::AtomicUsize::new(0));

    let map_results: Vec<Result<Option<(usize, String)>, String>> = stream::iter(
        shortlist.into_iter().enumerate().map(|(slot, (original_idx, chunk))| {
            let url = ollama_url.to_string();
            let model = model.to_string();
            let question = question.to_string();
            let on_phase_arc = on_phase_arc.clone();
            let completed = completed.clone();
            let cancel = cancel.cloned();
            async move {
                let prompt = build_relevance_prompt(&chunk, slot, llm_total, &question);
                let mut sink = |_: &str| {};
                let raw = ollama_stream(&url, &model, prompt, 4096, &mut sink, cancel.as_ref()).await?;
                let verdict = parse_relevance(&raw);
                let done = completed.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
                // Emit Search progress AFTER each call resolves (running count
                // visible to the user).
                if let Ok(mut cb) = on_phase_arc.lock() {
                    cb(Phase::Search { index: done.saturating_sub(1), total: llm_total });
                }
                if verdict.relevant {
                    Ok(Some((original_idx, chunk)))
                } else {
                    Ok::<Option<(usize, String)>, String>(None)
                }
            }
        }),
    )
    .buffer_unordered(MAP_CONCURRENCY)
    .collect()
    .await;

    // Surface the first error (if any).
    let mut relevant: Vec<(usize, String)> = Vec::new();
    for r in map_results {
        match r {
            Ok(Some(pair)) => relevant.push(pair),
            Ok(None) => {}
            Err(e) => return Err(e),
        }
    }
    // Reorder by original chunk index so the reduce prompt is monotonic.
    relevant.sort_by_key(|(idx, _)| *idx);

    if relevant.is_empty() {
        return Ok(AskAnswer {
            text: "I couldn't find anything about that in the transcript.".to_string(),
            citations: Vec::new(),
        });
    }

    // 3. Reduce pass.
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
        if let Ok(mut cb) = on_phase_arc.lock() {
            cb(Phase::Answer { chars: total_chars });
        }
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
        // Designed to split into >5 chunks so tests that rely on capping at
        // SHORTLIST_K=5 are meaningful. CHUNK_TARGET = 3000 tokens × 4
        // chars/token = 12k chars per chunk. We need >5 chunks = >60k chars.
        // sentence = 36 chars; 2000 × 36 = 72k chars ≈ 18k tokens → ~6 chunks.
        let sentence = "This is a long sentence with words. ";
        sentence.repeat(2000)
    }

    #[tokio::test]
    async fn keyword_prefilter_caps_relevance_calls_at_k_plus_one() {
        // Long transcript splits into many chunks. Question has one keyword
        // ("words") which matches every chunk equally. The prefilter must
        // still cap relevance calls at SHORTLIST_K (5).
        let transcript = long_transcript();
        let mock = MockOllama::spawn(|body| {
            let prompt = body["prompt"].as_str().unwrap_or("");
            if prompt.contains("Reply with a single JSON object") {
                r#"{"relevant": true, "gist": ""}"#.to_string()
            } else {
                "Reduced answer.\n--- citations ---\n".to_string()
            }
        }).await;

        let _ = run_map_shortlist(
            &mock.base_url, "gemma3", &transcript, "what words?", &[],
            &mut |_| {}, None,
        ).await.unwrap();

        let calls = mock.calls.lock().unwrap();
        let relevance_calls = calls
            .iter()
            .filter(|c| c["prompt"].as_str().unwrap_or("").contains("Reply with a single JSON object"))
            .count();
        let reduce_calls = calls.len() - relevance_calls;
        assert_eq!(relevance_calls, 5, "expected K=5 relevance calls, got {}", relevance_calls);
        assert_eq!(reduce_calls, 1, "expected exactly one reduce call");
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
            &mock.base_url, "gemma3", &transcript, "words?", &[],
            &mut |_| {}, None,
        ).await.unwrap();

        assert!(ans.text.to_lowercase().contains("couldn't find"));
        assert!(ans.citations.is_empty());
    }

    #[tokio::test]
    async fn fallback_when_question_has_no_keyword_overlap() {
        // Question is only stopwords — keyword shortlist returns empty.
        // The fallback path runs the relevance pass over ALL chunks.
        let transcript = long_transcript();
        let mock = MockOllama::spawn(|body| {
            let prompt = body["prompt"].as_str().unwrap_or("");
            if prompt.contains("Reply with a single JSON object") {
                r#"{"relevant": false, "gist": ""}"#.to_string()
            } else {
                "ignored".to_string()
            }
        }).await;

        let _ = run_map_shortlist(
            &mock.base_url, "gemma3", &transcript, "what is the?", &[],
            &mut |_| {}, None,
        ).await.unwrap();

        let calls = mock.calls.lock().unwrap();
        let relevance_calls = calls
            .iter()
            .filter(|c| c["prompt"].as_str().unwrap_or("").contains("Reply with a single JSON object"))
            .count();
        // The fallback runs over all chunks. Long transcript produces N chunks,
        // and N must be > SHORTLIST_K to prove fallback (not prefilter) is acting.
        assert!(relevance_calls > 5, "fallback path should issue relevance calls > K=5; got {relevance_calls}");
    }

    #[tokio::test]
    async fn emits_search_phase_then_answer_phase() {
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
            &mock.base_url, "gemma3", &transcript, "words?", &[],
            &mut on_phase, None,
        ).await.unwrap();

        let phases = phases.lock().unwrap();
        let search_count = phases.iter().filter(|p| matches!(p, Phase::Search { .. })).count();
        let answer_count = phases.iter().filter(|p| matches!(p, Phase::Answer { .. })).count();
        assert_eq!(search_count, 5, "expected SHORTLIST_K Search events");
        assert!(answer_count >= 1, "expected at least one Answer event");

        // Answer comes after the last Search.
        let last_search = phases.iter().rposition(|p| matches!(p, Phase::Search { .. })).unwrap();
        let first_answer = phases.iter().position(|p| matches!(p, Phase::Answer { .. })).unwrap();
        assert!(first_answer > last_search);
    }
}
