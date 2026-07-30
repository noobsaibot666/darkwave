use std::io::Write;
use std::process::Command;

#[test]
fn analyzes_a_real_wav_file_into_a_feature_vector() {
    let path = write_wav_fixture();

    let output = Command::new(env!("CARGO_BIN_EXE_similarity-worker"))
        .arg(&path)
        .output()
        .expect("run similarity-worker");

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid json");

    let analysis = parsed
        .get("analysis")
        .and_then(|value| value.as_array())
        .expect("analysis array present");
    assert!(!analysis.is_empty());

    let _ = std::fs::remove_file(path);
}

#[test]
fn reports_a_json_error_for_a_missing_file() {
    let output = Command::new(env!("CARGO_BIN_EXE_similarity-worker"))
        .arg("/nonexistent/path/does-not-exist.wav")
        .output()
        .expect("run similarity-worker");

    assert!(!output.status.success());

    let stdout = String::from_utf8(output.stdout).expect("utf8 stdout");
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).expect("valid json");
    assert!(parsed.get("error").is_some());
}

/// Hand-built 16-bit mono WAV: a couple seconds of a 440Hz tone, enough for
/// bliss-rs to produce a real analysis vector.
fn write_wav_fixture() -> std::path::PathBuf {
    let sample_rate = 44_100u32;
    let duration_secs = 2.0f32;
    let sample_count = (sample_rate as f32 * duration_secs) as usize;

    let samples: Vec<i16> = (0..sample_count)
        .map(|i| {
            let t = i as f32 / sample_rate as f32;
            (16_000.0 * (2.0 * std::f32::consts::PI * 440.0 * t).sin()) as i16
        })
        .collect();

    let data_size = (samples.len() * 2) as u32;
    let mut wav = Vec::new();
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_size).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16u32.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&1u16.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&(sample_rate * 2).to_le_bytes());
    wav.extend_from_slice(&2u16.to_le_bytes());
    wav.extend_from_slice(&16u16.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_size.to_le_bytes());
    for sample in samples {
        wav.extend_from_slice(&sample.to_le_bytes());
    }

    let mut path = std::env::temp_dir();
    path.push(format!("similarity-worker-fixture-{}.wav", std::process::id()));
    let mut file = std::fs::File::create(&path).expect("create fixture");
    file.write_all(&wav).expect("write fixture");
    path
}
