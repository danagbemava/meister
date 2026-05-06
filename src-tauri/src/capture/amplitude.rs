//! Per-frame RMS amplitude computation + per-session emitter that pushes
//! samples to the JS side at ~30 fps.

/// Compute RMS over an interleaved or mono float frame, clamped to [0.0, 1.0].
pub fn compute_rms(data: &[f32]) -> f32 {
    if data.is_empty() {
        return 0.0;
    }
    let sum: f32 = data.iter().map(|&s| s * s).sum();
    let rms = (sum / data.len() as f32).sqrt();
    rms.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rms_silence_is_zero() {
        let data = vec![0.0_f32; 1024];
        assert_eq!(compute_rms(&data), 0.0);
    }

    #[test]
    fn rms_unit_sine_is_one_over_sqrt_two() {
        let n = 1024;
        let data: Vec<f32> = (0..n)
            .map(|i| (2.0 * std::f32::consts::PI * i as f32 / 16.0).sin())
            .collect();
        let r = compute_rms(&data);
        assert!((r - 1.0 / 2.0_f32.sqrt()).abs() < 1e-3, "got {r}");
    }

    #[test]
    fn rms_clamps_above_one() {
        let data = vec![2.0_f32; 8];
        assert_eq!(compute_rms(&data), 1.0);
    }

    #[test]
    fn rms_empty_is_zero() {
        let data: Vec<f32> = Vec::new();
        assert_eq!(compute_rms(&data), 0.0);
    }
}
