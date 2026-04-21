//! ffmpeg-based mix + encode. Produces the final WebM/Opus at a caller-chosen
//! output path. When only system audio was captured, re-encodes to opus; when
//! both are present, runs amix.

use std::path::Path;
use std::process::Command;

pub fn encode_final(
    system_wav: &Path,
    mic_wav: Option<&Path>,
    output_webm: &Path,
) -> Result<(), String> {
    if let Some(parent) = output_webm.parent() {
        std::fs::create_dir_all(parent).map_err(|e| format!("create output dir: {e}"))?;
    }
    let args: Vec<String> = if let Some(mic) = mic_wav {
        vec![
            "-i".into(), system_wav.to_string_lossy().into_owned(),
            "-i".into(), mic.to_string_lossy().into_owned(),
            "-filter_complex".into(),
            "[0:a][1:a]amix=inputs=2:duration=longest:dropout_transition=0,aresample=48000[out]".into(),
            "-map".into(), "[out]".into(),
            "-c:a".into(), "libopus".into(),
            "-b:a".into(), "96k".into(),
            "-y".into(),
            output_webm.to_string_lossy().into_owned(),
        ]
    } else {
        vec![
            "-i".into(), system_wav.to_string_lossy().into_owned(),
            "-c:a".into(), "libopus".into(),
            "-b:a".into(), "96k".into(),
            "-y".into(),
            output_webm.to_string_lossy().into_owned(),
        ]
    };

    let output = Command::new("ffmpeg")
        .args(&args)
        .output()
        .map_err(|e| format!("ffmpeg not found or failed to start: {e}. Install via: brew install ffmpeg"))?;
    if !output.status.success() {
        return Err(format!("ffmpeg error:\n{}", String::from_utf8_lossy(&output.stderr)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::{SampleFormat, WavSpec, WavWriter};
    use tempfile::tempdir;

    fn write_sine(path: &Path, secs: u32, freq: f32) {
        let spec = WavSpec {
            channels: 2,
            sample_rate: 48_000,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };
        let mut w = WavWriter::create(path, spec).unwrap();
        for i in 0..48_000 * secs {
            let t = i as f32 / 48_000.0;
            let s = (2.0 * std::f32::consts::PI * freq * t).sin();
            let v = (s * 16_000.0) as i16;
            w.write_sample(v).unwrap();
            w.write_sample(v).unwrap();
        }
        w.finalize().unwrap();
    }

    #[test]
    fn encode_single_source_produces_nonempty_webm() {
        let dir = tempdir().unwrap();
        let sys = dir.path().join("system.wav");
        write_sine(&sys, 1, 440.0);
        let out = dir.path().join("out.webm");
        encode_final(&sys, None, &out).unwrap();
        let meta = std::fs::metadata(&out).unwrap();
        assert!(meta.len() > 0, "output webm should be non-empty");
    }

    #[test]
    fn encode_two_sources_amix_produces_nonempty_webm() {
        let dir = tempdir().unwrap();
        let sys = dir.path().join("system.wav");
        let mic = dir.path().join("mic.wav");
        write_sine(&sys, 1, 440.0);
        write_sine(&mic, 1, 880.0);
        let out = dir.path().join("out.webm");
        encode_final(&sys, Some(&mic), &out).unwrap();
        let meta = std::fs::metadata(&out).unwrap();
        assert!(meta.len() > 0);
    }
}
