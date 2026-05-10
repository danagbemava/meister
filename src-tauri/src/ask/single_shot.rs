//! Path A: single-shot Ollama call over the full transcript.

use crate::ask::citations::match_quotes;
use crate::ask::prompts::{build_answer_prompt, parse_answer};
use crate::ask::types::{AskAnswer, AskTurnIn};
use crate::cancellation::CancelToken;
use crate::commands::ollama_stream;

/// Run path A. Returns the final `AskAnswer` with citations resolved against
/// the original transcript.
///
/// `on_chars` is called as tokens stream in, with the running character count
/// of the answer (including any tail that will later be split off as the
/// citation block — splitting only happens at the end).
pub async fn run_single_shot(
    ollama_url: &str,
    model: &str,
    transcript: &str,
    question: &str,
    history: &[AskTurnIn],
    on_chars: &mut (dyn FnMut(usize) + Send),
    cancel: Option<&CancelToken>,
) -> Result<AskAnswer, String> {
    let prompt = build_answer_prompt(transcript, question, history);
    let mut total_chars: usize = 0;
    let mut on_token = |piece: &str| {
        total_chars += piece.chars().count();
        on_chars(total_chars);
    };
    let raw = ollama_stream(ollama_url, model, prompt, 8192, &mut on_token, cancel).await?;
    let (text, quotes) = parse_answer(&raw);
    let citations = match_quotes(transcript, &quotes);
    Ok(AskAnswer { text, citations })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::MockOllama;

    #[tokio::test]
    async fn single_shot_returns_answer_and_resolves_citations() {
        let transcript = "we delayed the Q3 budget review until November.";
        let mock = MockOllama::spawn(|_body| {
            "We delayed Q3 to November.\n\n--- citations ---\n\"we delayed the Q3 budget review until November\"\n".to_string()
        }).await;

        let mut chars: usize = 0;
        let ans = run_single_shot(
            &mock.base_url, "gemma3", transcript, "when was Q3 reviewed?",
            &[], &mut |c| chars = c, None,
        ).await.unwrap();

        assert!(ans.text.contains("November"));
        assert_eq!(ans.citations.len(), 1);
        assert!(ans.citations[0].start.is_some(), "citation should resolve to transcript offset");
        assert!(chars > 0, "on_chars should have been called as tokens streamed");

        let calls = mock.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        let prompt = calls[0]["prompt"].as_str().unwrap();
        assert!(prompt.contains("when was Q3 reviewed?"));
        assert!(prompt.contains("we delayed the Q3 budget review"));
    }

    #[tokio::test]
    async fn single_shot_marks_unmatched_citation_with_none_offsets() {
        let transcript = "agenda topic: marketing.";
        let mock = MockOllama::spawn(|_| {
            "Marketing was discussed.\n--- citations ---\n\"some text not in transcript\"\n".to_string()
        }).await;

        let ans = run_single_shot(
            &mock.base_url, "gemma3", transcript, "anything?", &[],
            &mut |_| {}, None,
        ).await.unwrap();

        assert_eq!(ans.citations.len(), 1);
        assert!(ans.citations[0].start.is_none());
        assert!(ans.citations[0].end.is_none());
    }

    #[tokio::test]
    async fn single_shot_handles_response_with_no_citation_block() {
        let mock = MockOllama::spawn(|_| "just an answer.".to_string()).await;
        let ans = run_single_shot(
            &mock.base_url, "gemma3", "irrelevant transcript", "q?", &[],
            &mut |_| {}, None,
        ).await.unwrap();
        assert_eq!(ans.text, "just an answer.");
        assert!(ans.citations.is_empty());
    }

    #[tokio::test]
    async fn single_shot_passes_history_into_prompt() {
        let mock = MockOllama::spawn(|_| "follow up answer.".to_string()).await;
        let history = vec![
            AskTurnIn { role: "user".into(), content: "prior question".into() },
            AskTurnIn { role: "assistant".into(), content: "prior answer".into() },
        ];
        let _ = run_single_shot(
            &mock.base_url, "gemma3", "t", "next?", &history,
            &mut |_| {}, None,
        ).await.unwrap();
        let calls = mock.calls.lock().unwrap();
        let prompt = calls[0]["prompt"].as_str().unwrap();
        assert!(prompt.contains("Previous conversation"));
        assert!(prompt.contains("prior question"));
        assert!(prompt.contains("prior answer"));
    }
}
