//! Ask-AI-about-transcript feature. See
//! `docs/superpowers/specs/2026-05-09-ask-ai-about-transcript-design.md`.

pub mod cancel;
pub mod citations;
pub mod map_shortlist;
pub mod prompts;
pub mod single_shot;
pub mod strategy;
pub mod types;

use std::sync::Arc;

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::ask::cancel::AskCancelRegistry;
use crate::ask::strategy::{pick_strategy, Strategy};
use crate::ask::types::{AskAnswer, AskTurnIn};
use crate::storage::{self, StageKind, StageStatus, Store};

#[derive(Clone, Serialize)]
#[serde(tag = "phase", rename_all = "lowercase")]
enum AskProgressPayload {
    #[serde(rename_all = "camelCase")]
    Search { chunk: ChunkInfo },
    Answer { chars: usize },
    Done,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct ChunkInfo {
    index: usize,
    total: usize,
}

#[tauri::command]
pub async fn ask_query(
    app: AppHandle,
    store: State<'_, Arc<Store>>,
    registry: State<'_, Arc<AskCancelRegistry>>,
    recording_id: String,
    question: String,
    history: Vec<AskTurnIn>,
) -> Result<AskAnswer, String> {
    if question.trim().is_empty() {
        return Err("question is empty".into());
    }

    // `recordings::get` already returns the recording with stages populated,
    // so one read covers both transcript text and Ollama settings.
    let store = (*store).clone();
    let rec_id = recording_id.clone();
    let (transcript, settings) =
        tokio::task::spawn_blocking(move || -> Result<(String, crate::storage::AppSettings), String> {
            let r = storage::recordings::get(&store, &rec_id)?
                .ok_or_else(|| format!("recording not found: {rec_id}"))?;
            let t = r
                .stages
                .iter()
                .find(|s| s.stage == StageKind::Transcribe)
                .ok_or_else(|| "transcribe stage not found".to_string())?;
            if t.status != StageStatus::Completed {
                return Err("transcribe stage has not completed".into());
            }
            let transcript = t
                .output
                .clone()
                .ok_or_else(|| "transcribe stage has no output".to_string())?;
            Ok((transcript, r.settings))
        })
        .await
        .map_err(|e| format!("task join: {e}"))??;

    let token = registry.token_for_new_query(&recording_id);

    let app_for_search = app.clone();
    let app_for_answer = app.clone();

    let result = match pick_strategy(&transcript) {
        Strategy::SingleShot => {
            let mut on_chars = move |chars: usize| {
                let _ = app_for_answer.emit("ask-progress", AskProgressPayload::Answer { chars });
            };
            crate::ask::single_shot::run_single_shot(
                &settings.ollama_url,
                &settings.model,
                &transcript,
                &question,
                &history,
                &mut on_chars,
                Some(&token),
            )
            .await
        }
        Strategy::MapShortlist => {
            let mut on_phase = move |p: crate::ask::map_shortlist::Phase| {
                let payload = match p {
                    crate::ask::map_shortlist::Phase::Search { index, total } => {
                        AskProgressPayload::Search { chunk: ChunkInfo { index, total } }
                    }
                    crate::ask::map_shortlist::Phase::Answer { chars } => {
                        AskProgressPayload::Answer { chars }
                    }
                };
                let _ = app_for_search.emit("ask-progress", payload);
            };
            crate::ask::map_shortlist::run_map_shortlist(
                &settings.ollama_url,
                &settings.model,
                &transcript,
                &question,
                &history,
                &mut on_phase,
                Some(&token),
            )
            .await
        }
    };

    registry.finish(&recording_id);
    let _ = app.emit("ask-progress", AskProgressPayload::Done);
    result
}

#[tauri::command]
pub async fn cancel_ask(
    registry: State<'_, Arc<AskCancelRegistry>>,
    recording_id: String,
) -> Result<(), String> {
    if let Some(t) = registry.get(&recording_id) {
        t.cancel();
        Ok(())
    } else {
        Err(format!("no in-flight ask for recording {recording_id}"))
    }
}
