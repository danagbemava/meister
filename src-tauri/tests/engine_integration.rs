//! Integration test for the whisper.cpp engine. Requires the bundled `base`
//! model to exist under `resources/models/` (created by `cargo build`'s
//! fetch-base-model.sh hook).

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use meister_lib::transcription::engine;
use meister_lib::transcription::models::ModelSize;

fn base_model_path() -> Option<PathBuf> {
    let p = PathBuf::from("resources/models");
    let bin = p.join("ggml-base.bin");
    let enc = p.join("ggml-base-encoder.mlmodelc");
    if bin.is_file() && enc.is_dir() { Some(p) } else { None }
}

#[test]
fn transcribes_fixture_with_base_model() {
    let Some(models_dir) = base_model_path() else {
        eprintln!("skipping: bundled base model not present (run `cargo build` first)");
        return;
    };

    // Tell whisper.cpp's Metal backend where to find ggml-metal.metal.
    // build.rs copies it into resources/ from whisper-rs-sys's source.
    std::env::set_var("GGML_METAL_PATH_RESOURCES", "resources");

    let progress = Arc::new(Mutex::new(Vec::<i32>::new()));
    let p = progress.clone();

    let wav = std::path::Path::new("tests/fixtures/hello.wav");
    let result = engine::transcribe_wav_with_callback(
        &models_dir,
        ModelSize::Base,
        wav,
        Some("en"),
        move |pct| { p.lock().unwrap().push(pct); },
    );

    let text = result.expect("transcription should succeed");
    let lower = text.to_lowercase();
    assert!(lower.contains("hello") || lower.contains("test"),
        "expected 'hello' or 'test' in transcript, got: {text:?}");

    let progress = progress.lock().unwrap();
    assert!(!progress.is_empty(), "progress callback should have fired at least once");
    assert!(progress.iter().any(|&p| p > 0), "progress should reach >0%");
}
