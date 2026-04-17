//! Deterministic content hash for templates, used to detect "template edited
//! since last format" on a recording.

use sha2::{Digest, Sha256};

pub fn template_hash(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_input_same_hash() {
        assert_eq!(template_hash("hi"), template_hash("hi"));
    }

    #[test]
    fn different_input_different_hash() {
        assert_ne!(template_hash("hi"), template_hash("bye"));
    }

    #[test]
    fn known_value() {
        assert_eq!(
            template_hash(""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
