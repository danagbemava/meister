//! Test-only helpers shared across modules.
//!
//! Lifted out of `commands.rs` so sibling modules (e.g. `ask`) can reuse the
//! same in-process fake Ollama for their integration tests.

#![cfg(test)]

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(crate) struct MockOllama {
    pub base_url: String,
    pub calls: Arc<Mutex<Vec<serde_json::Value>>>,
    pub overlaps: Arc<AtomicUsize>,
    pub in_flight: Arc<AtomicUsize>,
    shutdown: Option<tokio::sync::oneshot::Sender<()>>,
    handle: Option<tokio::task::JoinHandle<()>>,
}

impl MockOllama {
    pub async fn spawn<F>(handler: F) -> Self
    where
        F: Fn(serde_json::Value) -> String + Send + Sync + 'static,
    {
        use axum::{
            body::Body, extract::State, http::StatusCode, response::Response,
            routing::post, Router,
        };

        let calls: Arc<Mutex<Vec<serde_json::Value>>> = Arc::new(Mutex::new(Vec::new()));
        let overlaps = Arc::new(AtomicUsize::new(0));
        let in_flight = Arc::new(AtomicUsize::new(0));
        let handler = Arc::new(handler);

        #[derive(Clone)]
        struct AppState {
            calls: Arc<Mutex<Vec<serde_json::Value>>>,
            overlaps: Arc<AtomicUsize>,
            in_flight: Arc<AtomicUsize>,
            handler: Arc<dyn Fn(serde_json::Value) -> String + Send + Sync>,
        }

        async fn generate(
            State(s): State<AppState>,
            axum::Json(body): axum::Json<serde_json::Value>,
        ) -> Response {
            s.calls.lock().unwrap().push(body.clone());
            let cur = s.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            if cur > 1 {
                s.overlaps.fetch_add(1, Ordering::SeqCst);
            }

            let text = (s.handler)(body);
            // Give tests time to observe overlap.
            tokio::time::sleep(Duration::from_millis(60)).await;

            // Stream the text as NDJSON with one "response" line per word + one "done".
            let mut lines = String::new();
            for w in text.split_inclusive(' ') {
                let line = serde_json::json!({ "response": w }).to_string();
                lines.push_str(&line);
                lines.push('\n');
            }
            lines.push_str(&serde_json::json!({ "done": true }).to_string());
            lines.push('\n');

            s.in_flight.fetch_sub(1, Ordering::SeqCst);

            Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "application/x-ndjson")
                .body(Body::from(lines))
                .unwrap()
        }

        let state = AppState {
            calls: calls.clone(),
            overlaps: overlaps.clone(),
            in_flight: in_flight.clone(),
            handler,
        };

        let app = Router::new()
            .route("/api/generate", post(generate))
            .with_state(state);

        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let base_url = format!("http://{addr}");

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let handle = tokio::spawn(async move {
            axum::serve(listener, app)
                .with_graceful_shutdown(async { let _ = rx.await; })
                .await
                .ok();
        });

        Self { base_url, calls, overlaps, in_flight, shutdown: Some(tx), handle: Some(handle) }
    }
}

impl Drop for MockOllama {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
    }
}
