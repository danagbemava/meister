use std::path::Path;

pub fn read_wav_as_f32_mono(path: &Path) -> Result<Vec<f32>, String> {
    let mut reader = hound::WavReader::open(path)
        .map_err(|e| format!("open wav {}: {e}", path.display()))?;
    let spec = reader.spec();
    if spec.channels != 1 {
        return Err(format!("expected mono WAV, got {} channels", spec.channels));
    }
    if spec.sample_rate != 16_000 {
        return Err(format!("expected 16 kHz WAV, got {} Hz", spec.sample_rate));
    }
    if spec.bits_per_sample != 16 {
        return Err(format!("expected 16-bit WAV, got {} bits", spec.bits_per_sample));
    }
    let samples: Result<Vec<i16>, _> = reader.samples::<i16>().collect();
    let samples = samples.map_err(|e| format!("read samples: {e}"))?;
    Ok(samples.into_iter().map(|s| s as f32 / 32768.0).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_fixture_wav() {
        let path = std::path::Path::new("tests/fixtures/hello.wav");
        let samples = read_wav_as_f32_mono(path).expect("fixture should be readable");
        assert!(!samples.is_empty(), "should produce samples");
        // 16 kHz mono, ~1–3 seconds of speech → 16k–48k samples
        assert!(samples.len() > 8_000, "too few samples: {}", samples.len());
        assert!(samples.iter().all(|s| s.abs() <= 1.0), "samples should be in [-1, 1]");
    }

    #[test]
    fn rejects_missing_file() {
        let err = read_wav_as_f32_mono(std::path::Path::new("/nonexistent.wav")).unwrap_err();
        assert!(err.contains("open wav"));
    }
}
