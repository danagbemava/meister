use std::path::PathBuf;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSize {
    Tiny,
    Base,
    Small,
    Medium,
    Large,
}

impl ModelSize {
    pub fn from_str(s: &str) -> Option<ModelSize> {
        match s {
            "tiny" => Some(Self::Tiny),
            "base" => Some(Self::Base),
            "small" => Some(Self::Small),
            "medium" => Some(Self::Medium),
            "large" => Some(Self::Large),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Tiny => "tiny",
            Self::Base => "base",
            Self::Small => "small",
            Self::Medium => "medium",
            Self::Large => "large",
        }
    }

    /// Approximate total download size (.bin + CoreML encoder), in bytes.
    /// Used as the user-facing "this download is N GB" estimate. Real on-disk
    /// sizes are read from the filesystem after download completes.
    pub fn approx_bundle_bytes(&self) -> u64 {
        match self {
            Self::Tiny => 78 * 1024 * 1024 + 17 * 1024 * 1024,
            Self::Base => 148 * 1024 * 1024 + 75 * 1024 * 1024,
            Self::Small => 488 * 1024 * 1024 + 230 * 1024 * 1024,
            Self::Medium => 1_500 * 1024 * 1024 + 700 * 1024 * 1024,
            Self::Large => 3_100 * 1024 * 1024 + 1_500 * 1024 * 1024,
        }
    }
}

pub fn bin_filename(size: ModelSize) -> String {
    format!("ggml-{}.bin", size.as_str())
}

pub fn encoder_dirname(size: ModelSize) -> String {
    format!("ggml-{}-encoder.mlmodelc", size.as_str())
}

pub fn model_paths(models_dir: &std::path::Path, size: ModelSize) -> (PathBuf, PathBuf) {
    (
        models_dir.join(bin_filename(size)),
        models_dir.join(encoder_dirname(size)),
    )
}

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub size: String,
    pub downloaded: bool,
    pub size_bytes: u64,
}

const ALL_SIZES: [ModelSize; 5] = [
    ModelSize::Tiny,
    ModelSize::Base,
    ModelSize::Small,
    ModelSize::Medium,
    ModelSize::Large,
];

pub fn list_available_models(models_dir: &std::path::Path) -> Vec<ModelInfo> {
    ALL_SIZES.iter().map(|&size| {
        let (bin, enc) = model_paths(models_dir, size);
        let downloaded = bin.is_file() && enc.is_dir();
        let size_bytes = if downloaded {
            std::fs::metadata(&bin).map(|m| m.len()).unwrap_or(0)
                + dir_size(&enc).unwrap_or(0)
        } else {
            size.approx_bundle_bytes()
        };
        ModelInfo { size: size.as_str().to_string(), downloaded, size_bytes }
    }).collect()
}

fn dir_size(path: &std::path::Path) -> std::io::Result<u64> {
    let mut total = 0u64;
    for entry in std::fs::read_dir(path)? {
        let entry = entry?;
        let meta = entry.metadata()?;
        total += if meta.is_dir() { dir_size(&entry.path()).unwrap_or(0) } else { meta.len() };
    }
    Ok(total)
}

/// Copy the bundled base model from `<resource_dir>/models/` into
/// `<app_data_dir>/models/` if not already present. Idempotent.
pub fn ensure_bundled_base_copied(
    resource_dir: &std::path::Path,
    app_data_dir: &std::path::Path,
) -> Result<(), String> {
    let target_dir = app_data_dir.join("models");
    std::fs::create_dir_all(&target_dir).map_err(|e| format!("create models dir: {e}"))?;

    let (target_bin, target_enc) = model_paths(&target_dir, ModelSize::Base);
    if target_bin.is_file() && target_enc.is_dir() {
        return Ok(());
    }

    let bundled_dir = resource_dir.join("models");
    let (src_bin, src_enc) = model_paths(&bundled_dir, ModelSize::Base);

    if !src_bin.is_file() {
        return Err(format!("bundled base model not found at {}", src_bin.display()));
    }

    if !target_bin.exists() {
        std::fs::copy(&src_bin, &target_bin)
            .map_err(|e| format!("copy bin: {e}"))?;
    }
    if !target_enc.exists() {
        copy_dir_recursive(&src_enc, &target_enc)
            .map_err(|e| format!("copy encoder: {e}"))?;
    }
    Ok(())
}

fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let target = dst.join(entry.file_name());
        if path.is_dir() {
            copy_dir_recursive(&path, &target)?;
        } else {
            std::fs::copy(&path, &target)?;
        }
    }
    Ok(())
}

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::collections::HashMap;

#[derive(Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn cancel(&self) { self.0.store(true, Ordering::SeqCst); }
    pub fn is_cancelled(&self) -> bool { self.0.load(Ordering::SeqCst) }
}

pub fn new_cancel_token() -> CancelToken { CancelToken::default() }

#[derive(Default)]
pub struct CancelRegistry {
    inner: Mutex<HashMap<String, CancelToken>>,
}

impl CancelRegistry {
    pub fn token_for(&self, size: &str) -> CancelToken {
        let mut g = self.inner.lock().unwrap();
        g.entry(size.to_string()).or_default().clone()
    }

    pub fn reset(&self, size: &str) {
        let mut g = self.inner.lock().unwrap();
        g.insert(size.to_string(), CancelToken::default());
    }
}

const HF_BASE: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

/// Download a model's .bin and .mlmodelc.zip (streamed) into `models_dir`.
/// `progress` is invoked with (bytes_downloaded, bytes_total_estimate).
/// `cancel` is checked between chunks; if set, the function returns Err("cancelled").
pub async fn download_model(
    models_dir: &std::path::Path,
    size: ModelSize,
    cancel: CancelToken,
    progress: impl Fn(u64, u64) + Send + 'static,
) -> Result<(), String> {
    use futures_util::StreamExt;
    use std::io::Write;

    let tmp_dir = models_dir.join(".tmp").join(size.as_str());
    let _ = std::fs::remove_dir_all(&tmp_dir); // Clean up any leftovers
    std::fs::create_dir_all(&tmp_dir).map_err(|e| format!("create tmp dir: {e}"))?;

    let bin_url = format!("{HF_BASE}/{}", bin_filename(size));
    let enc_url = format!("{HF_BASE}/ggml-{}-encoder.mlmodelc.zip", size.as_str());

    let staged_bin = tmp_dir.join(bin_filename(size));
    let staged_zip = tmp_dir.join(format!("{}-encoder.mlmodelc.zip", size.as_str()));

    let total_estimate = size.approx_bundle_bytes();
    let mut downloaded: u64 = 0;

    // Stream both files, sharing one running counter, into the staging dir.
    for (url, target) in [(&bin_url, &staged_bin), (&enc_url, &staged_zip)] {
        let resp = reqwest::Client::new().get(url).send().await
            .map_err(|e| format!("GET {url}: {e}"))?
            .error_for_status()
            .map_err(|e| format!("HTTP error from {url}: {e}"))?;

        let mut file = std::fs::File::create(target)
            .map_err(|e| format!("create {}: {e}", target.display()))?;
        let mut stream = resp.bytes_stream();

        while let Some(chunk) = stream.next().await {
            if cancel.is_cancelled() {
                let _ = std::fs::remove_dir_all(&tmp_dir);
                return Err("cancelled".to_string());
            }
            let bytes = chunk.map_err(|e| format!("download chunk: {e}"))?;
            file.write_all(&bytes).map_err(|e| format!("write: {e}"))?;
            downloaded += bytes.len() as u64;
            progress(downloaded, total_estimate);
        }
    }

    // Unzip encoder in place
    let zip_file = std::fs::File::open(&staged_zip).map_err(|e| format!("open zip: {e}"))?;
    let mut archive = zip::ZipArchive::new(zip_file).map_err(|e| format!("read zip: {e}"))?;
    archive.extract(&tmp_dir).map_err(|e| format!("unzip: {e}"))?;
    std::fs::remove_file(&staged_zip).ok();

    // Atomic rename into final location
    let (final_bin, final_enc) = model_paths(models_dir, size);
    let staged_enc = tmp_dir.join(encoder_dirname(size));
    if !staged_enc.is_dir() {
        return Err(format!("expected encoder dir after unzip: {}", staged_enc.display()));
    }
    if final_bin.exists() { std::fs::remove_file(&final_bin).ok(); }
    if final_enc.exists() { std::fs::remove_dir_all(&final_enc).ok(); }
    std::fs::rename(&staged_bin, &final_bin).map_err(|e| format!("install bin: {e}"))?;
    std::fs::rename(&staged_enc, &final_enc).map_err(|e| format!("install encoder: {e}"))?;

    // Best-effort cleanup of the tmp staging dir
    let _ = std::fs::remove_dir_all(&tmp_dir);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_sizes() {
        assert_eq!(ModelSize::from_str("base"), Some(ModelSize::Base));
        assert_eq!(ModelSize::from_str("large"), Some(ModelSize::Large));
        assert_eq!(ModelSize::from_str("xl"), None);
    }

    #[test]
    fn round_trips_str_form() {
        for s in ["tiny", "base", "small", "medium", "large"] {
            let m = ModelSize::from_str(s).unwrap();
            assert_eq!(m.as_str(), s);
        }
    }

    #[test]
    fn paths_are_under_models_dir() {
        let dir = std::path::Path::new("/tmp/x");
        let (bin, enc) = model_paths(dir, ModelSize::Base);
        assert_eq!(bin, std::path::PathBuf::from("/tmp/x/ggml-base.bin"));
        assert_eq!(enc, std::path::PathBuf::from("/tmp/x/ggml-base-encoder.mlmodelc"));
    }

    #[test]
    fn bundle_bytes_are_increasing() {
        let sizes = [ModelSize::Tiny, ModelSize::Base, ModelSize::Small, ModelSize::Medium, ModelSize::Large];
        for w in sizes.windows(2) {
            assert!(w[0].approx_bundle_bytes() < w[1].approx_bundle_bytes());
        }
    }

    #[test]
    fn lists_no_models_in_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let infos = list_available_models(tmp.path());
        assert_eq!(infos.len(), 5);
        assert!(infos.iter().all(|i| !i.downloaded));
    }

    #[test]
    fn detects_base_when_both_files_present() {
        let tmp = tempfile::tempdir().unwrap();
        let (bin, enc) = model_paths(tmp.path(), ModelSize::Base);
        std::fs::write(&bin, b"fake-bin-bytes").unwrap();
        std::fs::create_dir_all(&enc).unwrap();
        std::fs::write(enc.join("model.mlmodel"), b"fake").unwrap();

        let infos = list_available_models(tmp.path());
        let base = infos.iter().find(|i| i.size == "base").unwrap();
        assert!(base.downloaded);
        assert!(base.size_bytes > 0);
    }

    #[test]
    fn missing_encoder_means_not_downloaded() {
        let tmp = tempfile::tempdir().unwrap();
        let (bin, _enc) = model_paths(tmp.path(), ModelSize::Base);
        std::fs::write(&bin, b"only-bin").unwrap();
        let infos = list_available_models(tmp.path());
        let base = infos.iter().find(|i| i.size == "base").unwrap();
        assert!(!base.downloaded, "model should be incomplete without encoder");
    }

    #[test]
    fn ensure_bundled_copies_when_missing() {
        let resource = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();

        // Fake "bundled" base model under resource_dir/models/
        let bundled_models = resource.path().join("models");
        std::fs::create_dir_all(&bundled_models).unwrap();
        let (bin, enc) = model_paths(&bundled_models, ModelSize::Base);
        std::fs::write(&bin, b"bundled-bin").unwrap();
        std::fs::create_dir_all(&enc).unwrap();
        std::fs::write(enc.join("file"), b"x").unwrap();

        ensure_bundled_base_copied(resource.path(), app_data.path()).unwrap();

        let (target_bin, target_enc) = model_paths(&app_data.path().join("models"), ModelSize::Base);
        assert!(target_bin.exists());
        assert!(target_enc.is_dir());
        assert_eq!(std::fs::read(&target_bin).unwrap(), b"bundled-bin");
    }

    #[test]
    fn ensure_bundled_skips_when_already_present() {
        let resource = tempfile::tempdir().unwrap();
        let app_data = tempfile::tempdir().unwrap();
        let bundled = resource.path().join("models");
        std::fs::create_dir_all(&bundled).unwrap();

        // Pre-existing model in app_data
        let target_dir = app_data.path().join("models");
        std::fs::create_dir_all(&target_dir).unwrap();
        let (target_bin, target_enc) = model_paths(&target_dir, ModelSize::Base);
        std::fs::write(&target_bin, b"existing").unwrap();
        std::fs::create_dir_all(&target_enc).unwrap();
        std::fs::write(target_enc.join("file"), b"x").unwrap();

        // Bundled model has different content; ensure we don't overwrite
        let (bin, enc) = model_paths(&bundled, ModelSize::Base);
        std::fs::write(&bin, b"different").unwrap();
        std::fs::create_dir_all(&enc).unwrap();
        std::fs::write(enc.join("file"), b"y").unwrap();

        ensure_bundled_base_copied(resource.path(), app_data.path()).unwrap();
        assert_eq!(std::fs::read(&target_bin).unwrap(), b"existing");
    }

    #[test]
    fn cancel_token_starts_unset() {
        let tok = new_cancel_token();
        assert!(!tok.is_cancelled());
    }

    #[test]
    fn cancel_token_flips_when_cancelled() {
        let tok = new_cancel_token();
        let tok2 = tok.clone();
        tok2.cancel();
        assert!(tok.is_cancelled());
    }

    #[test]
    fn registry_returns_same_token_per_size() {
        let r = CancelRegistry::default();
        let a = r.token_for("base");
        let b = r.token_for("base");
        b.cancel();
        assert!(a.is_cancelled(), "tokens for the same size should share state");
    }

    #[test]
    fn registry_separates_sizes() {
        let r = CancelRegistry::default();
        let a = r.token_for("base");
        let b = r.token_for("large");
        a.cancel();
        assert!(!b.is_cancelled());
    }
}
