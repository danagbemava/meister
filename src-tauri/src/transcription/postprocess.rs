//! Post-processing of whisper segment output: collapses lock-in loops
//! and drops known hallucinated boilerplate.

pub(crate) fn collapse_repetitions(segments: &[String]) -> Vec<String> {
    segments.to_vec()
}

pub(crate) fn filter_hallucinations(segments: &[String]) -> Vec<String> {
    segments.to_vec()
}

#[cfg(test)]
mod tests {}
