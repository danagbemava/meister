//! Path-A/B selection for the Ask pipeline.
//!
//! Single source of truth lives in `commands::{THRESHOLD, approx_tokens}` —
//! the format pipeline already uses it. We share it so the boundary doesn't
//! drift between features.

use crate::commands::{approx_tokens_pub as approx_tokens, THRESHOLD_PUB as THRESHOLD};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Strategy {
    SingleShot,
    MapShortlist,
}

pub fn pick_strategy(transcript: &str) -> Strategy {
    if approx_tokens(transcript) < THRESHOLD {
        Strategy::SingleShot
    } else {
        Strategy::MapShortlist
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_transcript_picks_single_shot() {
        let t: String = "word ".repeat(100);
        assert_eq!(pick_strategy(&t), Strategy::SingleShot);
    }

    #[test]
    fn long_transcript_picks_map_shortlist() {
        // THRESHOLD = 4000 tokens × ~4 chars/token = ~16k chars. 30k chars
        // is comfortably above that.
        let t: String = "x".repeat(30_000);
        assert_eq!(pick_strategy(&t), Strategy::MapShortlist);
    }

    #[test]
    fn exactly_at_threshold_picks_map_shortlist() {
        // approx_tokens uses chars()/4, so 4000 tokens == 16_000 chars exactly.
        let t: String = "a".repeat(16_000);
        assert_eq!(pick_strategy(&t), Strategy::MapShortlist);
    }

    #[test]
    fn one_below_threshold_picks_single_shot() {
        let t: String = "a".repeat(15_999);
        assert_eq!(pick_strategy(&t), Strategy::SingleShot);
    }
}
