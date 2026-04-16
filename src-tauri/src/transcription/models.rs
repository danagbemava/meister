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
}
