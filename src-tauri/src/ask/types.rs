//! Wire types shared between Rust and TypeScript for the Ask feature.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AskCitation {
    pub quote: String,
    pub start: Option<usize>,
    pub end: Option<usize>,
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct AskAnswer {
    pub text: String,
    pub citations: Vec<AskCitation>,
}

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct AskTurnIn {
    pub role: String,
    pub content: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ask_citation_serializes_camel_case() {
        let c = AskCitation { quote: "hi".into(), start: Some(3), end: Some(5) };
        let json = serde_json::to_value(&c).unwrap();
        assert_eq!(json, serde_json::json!({"quote": "hi", "start": 3, "end": 5}));
    }

    #[test]
    fn ask_citation_serializes_null_offsets() {
        let c = AskCitation { quote: "x".into(), start: None, end: None };
        let json = serde_json::to_value(&c).unwrap();
        assert_eq!(json, serde_json::json!({"quote": "x", "start": null, "end": null}));
    }

    #[test]
    fn ask_answer_round_trips_in_camel_case() {
        let a = AskAnswer {
            text: "the answer".into(),
            citations: vec![AskCitation { quote: "q".into(), start: Some(0), end: Some(1) }],
        };
        let json = serde_json::to_value(&a).unwrap();
        assert_eq!(
            json,
            serde_json::json!({"text": "the answer", "citations": [{"quote":"q","start":0,"end":1}]}),
        );
    }

    #[test]
    fn ask_turn_in_deserializes_role_and_content() {
        let v: AskTurnIn = serde_json::from_value(serde_json::json!({"role": "user", "content": "hi"})).unwrap();
        assert_eq!(v, AskTurnIn { role: "user".into(), content: "hi".into() });
    }
}
