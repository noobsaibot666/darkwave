use audio_metadata::DecodedAudioBuffer;
use pitch_detection::detector::mcleod::McLeodDetector;
use pitch_detection::detector::PitchDetector;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AudioMeasurements {
    pub peak_db: f32,
    pub transient_density: f32,
    pub low_frequency_energy: f32,
}

pub fn intensity_score(measurements: AudioMeasurements) -> f32 {
    (measurements.transient_density * 0.5
        + measurements.low_frequency_energy * 0.3
        + measurements.peak_db.abs().recip() * 0.2)
        .clamp(0.0, 1.0)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TempoEstimate {
    pub bpm: f32,
    pub confidence: f32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ActionTag {
    Impact,
    Whoosh,
    Rise,
}

impl ActionTag {
    pub fn as_str(&self) -> &'static str {
        match self {
            ActionTag::Impact => "Impact",
            ActionTag::Whoosh => "Whoosh",
            ActionTag::Rise => "Rise",
        }
    }
}

/// Floor used when a buffer's peak is at or near digital silence, so peak_db
/// never becomes -infinity.
const SILENT_PEAK_DB_FLOOR: f32 = -120.0;

/// RMS below this (roughly -60dBFS) is treated as silence for needs-review
/// purposes. Real audio content essentially never sits this quiet throughout.
const SILENCE_RMS_THRESHOLD: f32 = 0.001;

const ENVELOPE_FRAME_MS: f32 = 20.0;

/// Real, content-based replacement for the size-only "needs review" guess.
/// Augments (does not replace) the fast synchronous size check at import
/// time: this runs after decode and can catch a corrupt-but-large file that
/// size alone never could.
pub fn is_likely_silent_or_corrupt(buffer: &DecodedAudioBuffer) -> bool {
    if buffer.samples.is_empty() || buffer.sample_rate == 0 || buffer.channels == 0 {
        return true;
    }

    rms(&buffer.samples) < SILENCE_RMS_THRESHOLD
}

pub fn measure(buffer: &DecodedAudioBuffer) -> AudioMeasurements {
    let mono = mono_samples(buffer);
    let peak = mono.iter().fold(0.0f32, |max, sample| max.max(sample.abs()));
    let peak_db = if peak > 0.0 {
        20.0 * peak.log10()
    } else {
        SILENT_PEAK_DB_FLOOR
    };

    let envelope = energy_envelope(&mono, buffer.sample_rate, ENVELOPE_FRAME_MS);
    let duration_secs = mono.len() as f32 / buffer.sample_rate.max(1) as f32;
    let transient_density = count_onsets(&envelope) as f32 / duration_secs.max(0.001);

    let low_frequency_energy = low_frequency_energy_ratio(&mono, buffer.sample_rate);

    AudioMeasurements {
        peak_db,
        transient_density,
        low_frequency_energy,
    }
}

const MIN_TEMPO_DURATION_SECS: f32 = 2.0;
const MIN_BPM: u32 = 40;
const MAX_BPM: u32 = 220;

/// Best-effort time-domain tempo estimate via autocorrelation of the energy
/// envelope. Not studio-grade — a simple, dependency-free approximation.
/// Returns `None` for clips too short to meaningfully autocorrelate.
pub fn estimate_tempo(buffer: &DecodedAudioBuffer) -> Option<TempoEstimate> {
    let mono = mono_samples(buffer);
    let duration_secs = mono.len() as f32 / buffer.sample_rate.max(1) as f32;
    if duration_secs < MIN_TEMPO_DURATION_SECS {
        return None;
    }

    let envelope = energy_envelope(&mono, buffer.sample_rate, ENVELOPE_FRAME_MS);
    if envelope.len() < 4 {
        return None;
    }
    let frame_secs = ENVELOPE_FRAME_MS / 1000.0;

    let mean = envelope.iter().sum::<f32>() / envelope.len() as f32;
    let centered: Vec<f32> = envelope.iter().map(|value| value - mean).collect();

    let mut best_bpm = MIN_BPM;
    let mut best_score = f32::MIN;
    let mut scores = Vec::with_capacity((MAX_BPM - MIN_BPM + 1) as usize);

    for bpm in MIN_BPM..=MAX_BPM {
        let lag_secs = 60.0 / bpm as f32;
        let lag_frames = (lag_secs / frame_secs).round() as usize;
        if lag_frames == 0 || lag_frames >= centered.len() {
            continue;
        }

        let mut score = 0.0f32;
        let mut count = 0usize;
        for i in 0..(centered.len() - lag_frames) {
            score += centered[i] * centered[i + lag_frames];
            count += 1;
        }
        if count == 0 {
            continue;
        }
        score /= count as f32;

        scores.push(score);
        if score > best_score {
            best_score = score;
            best_bpm = bpm;
        }
    }

    if scores.is_empty() {
        return None;
    }

    let score_mean = scores.iter().sum::<f32>() / scores.len() as f32;
    let score_max = scores.iter().fold(f32::MIN, |max, value| max.max(*value));
    let spread = (score_max - score_mean).max(1e-6);
    let confidence = ((best_score - score_mean) / spread).clamp(0.0, 1.0);

    Some(TempoEstimate {
        bpm: best_bpm as f32,
        confidence,
    })
}

const IMPACT_MAX_DURATION_SECS: f32 = 2.0;
const IMPACT_MIN_TRANSIENT_DENSITY: f32 = 1.0;
const IMPACT_MIN_PEAK_DB: f32 = -18.0;

const WHOOSH_MIN_DURATION_SECS: f32 = 0.3;
const WHOOSH_MAX_DURATION_SECS: f32 = 3.0;
const WHOOSH_MAX_TRANSIENT_DENSITY: f32 = 1.0;

const RISE_MIN_DURATION_SECS: f32 = 0.5;
const RISE_TREND_RATIO: f32 = 1.5;

/// Rule-based, best-effort action-tag suggestions from real signal shape —
/// not ML classification. These are the first thing that can ever suggest
/// "Rise": the filename/metadata suggestion vocabulary never has.
pub fn suggest_action_tags(
    buffer: &DecodedAudioBuffer,
    measurements: AudioMeasurements,
) -> Vec<ActionTag> {
    let mono = mono_samples(buffer);
    let duration_secs = mono.len() as f32 / buffer.sample_rate.max(1) as f32;
    let mut tags = Vec::new();

    if duration_secs <= IMPACT_MAX_DURATION_SECS
        && measurements.transient_density >= IMPACT_MIN_TRANSIENT_DENSITY
        && measurements.peak_db >= IMPACT_MIN_PEAK_DB
    {
        tags.push(ActionTag::Impact);
    } else if (WHOOSH_MIN_DURATION_SECS..=WHOOSH_MAX_DURATION_SECS).contains(&duration_secs)
        && measurements.transient_density < WHOOSH_MAX_TRANSIENT_DENSITY
    {
        tags.push(ActionTag::Whoosh);
    }

    if duration_secs >= RISE_MIN_DURATION_SECS && trends_upward(&mono, buffer.sample_rate) {
        tags.push(ActionTag::Rise);
    }

    tags
}

const PITCH_WINDOW_SIZE: usize = 2048;
const PITCH_POWER_THRESHOLD: f32 = 5.0;
const PITCH_CLARITY_THRESHOLD: f32 = 0.6;
const MIN_SAMPLES_FOR_PITCH: usize = 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct PitchEstimate {
    /// Nearest note name, e.g. "A4". Not a musical key — McLeod pitch
    /// tracking is a monophonic estimate and will be unreliable on dense
    /// polyphonic music. Most useful on single-source SFX/ambience/drones.
    pub note_name: String,
    pub frequency_hz: f32,
    pub clarity: f32,
}

/// Best-effort dominant-pitch detection over a representative window taken
/// from the middle of the clip (avoids likely silence at the very start/end).
pub fn estimate_pitch(buffer: &DecodedAudioBuffer) -> Option<PitchEstimate> {
    let mono = mono_samples(buffer);
    if mono.len() < MIN_SAMPLES_FOR_PITCH {
        return None;
    }

    let window_size = PITCH_WINDOW_SIZE.min(mono.len());
    let start = (mono.len() - window_size) / 2;
    let window = &mono[start..start + window_size];
    let padding = window_size / 2;

    let mut detector = McLeodDetector::new(window_size, padding);
    let pitch = detector.get_pitch(
        window,
        buffer.sample_rate as usize,
        PITCH_POWER_THRESHOLD,
        PITCH_CLARITY_THRESHOLD,
    )?;

    Some(PitchEstimate {
        note_name: note_name_for_frequency(pitch.frequency),
        frequency_hz: pitch.frequency,
        clarity: pitch.clarity,
    })
}

fn note_name_for_frequency(frequency_hz: f32) -> String {
    const NOTE_NAMES: [&str; 12] = [
        "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
    ];

    if frequency_hz <= 0.0 {
        return "?".to_string();
    }

    let midi = (69.0 + 12.0 * (frequency_hz / 440.0).log2()).round() as i32;
    let note_index = midi.rem_euclid(12) as usize;
    let octave = midi.div_euclid(12) - 1;

    format!("{}{octave}", NOTE_NAMES[note_index])
}

fn trends_upward(mono: &[f32], sample_rate: u32) -> bool {
    let envelope = energy_envelope(mono, sample_rate, ENVELOPE_FRAME_MS);
    if envelope.len() < 6 {
        return false;
    }

    let third = envelope.len() / 3;
    let first_third = &envelope[..third];
    let last_third = &envelope[envelope.len() - third..];

    let first_avg = first_third.iter().sum::<f32>() / first_third.len() as f32;
    let last_avg = last_third.iter().sum::<f32>() / last_third.len() as f32;

    last_avg > first_avg * RISE_TREND_RATIO
}

fn mono_samples(buffer: &DecodedAudioBuffer) -> Vec<f32> {
    let channels = buffer.channels.max(1) as usize;
    if channels == 1 {
        return buffer.samples.clone();
    }

    buffer
        .samples
        .chunks_exact(channels)
        .map(|frame| frame.iter().sum::<f32>() / channels as f32)
        .collect()
}

fn rms(samples: &[f32]) -> f32 {
    if samples.is_empty() {
        return 0.0;
    }
    let sum_squares: f32 = samples.iter().map(|sample| sample * sample).sum();
    (sum_squares / samples.len() as f32).sqrt()
}

fn energy_envelope(mono: &[f32], sample_rate: u32, frame_ms: f32) -> Vec<f32> {
    let frame_size = ((sample_rate.max(1) as f32) * frame_ms / 1000.0).round() as usize;
    let frame_size = frame_size.max(1);

    mono.chunks(frame_size).map(rms).collect()
}

fn count_onsets(envelope: &[f32]) -> usize {
    const ONSET_JUMP_RATIO: f32 = 1.5;
    const ONSET_MIN_ENERGY: f32 = 0.02;

    envelope
        .windows(2)
        .filter(|pair| {
            let (previous, current) = (pair[0], pair[1]);
            current > ONSET_MIN_ENERGY && current > previous * ONSET_JUMP_RATIO
        })
        .count()
}

fn low_frequency_energy_ratio(mono: &[f32], sample_rate: u32) -> f32 {
    if mono.is_empty() || sample_rate == 0 {
        return 0.0;
    }

    const LOW_PASS_CUTOFF_HZ: f32 = 250.0;
    let dt = 1.0 / sample_rate as f32;
    let rc = 1.0 / (2.0 * std::f32::consts::PI * LOW_PASS_CUTOFF_HZ);
    let alpha = dt / (rc + dt);

    let mut filtered = Vec::with_capacity(mono.len());
    let mut previous = 0.0f32;
    for &sample in mono.iter() {
        previous += alpha * (sample - previous);
        filtered.push(previous);
    }

    let total_rms = rms(mono);
    if total_rms <= f32::EPSILON {
        return 0.0;
    }

    (rms(&filtered) / total_rms).clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sine_wave(
        freq: f32,
        duration_secs: f32,
        sample_rate: u32,
        amplitude: f32,
    ) -> DecodedAudioBuffer {
        let sample_count = (duration_secs * sample_rate as f32) as usize;
        let samples = (0..sample_count)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                amplitude * (2.0 * std::f32::consts::PI * freq * t).sin()
            })
            .collect();

        DecodedAudioBuffer {
            sample_rate,
            channels: 1,
            samples,
        }
    }

    #[test]
    fn intensity_score_is_bounded() {
        let score = intensity_score(AudioMeasurements {
            peak_db: -0.1,
            transient_density: 9.0,
            low_frequency_energy: 9.0,
        });

        assert_eq!(score, 1.0);
    }

    #[test]
    fn empty_buffer_is_likely_corrupt() {
        let buffer = DecodedAudioBuffer {
            sample_rate: 44_100,
            channels: 1,
            samples: vec![],
        };

        assert!(is_likely_silent_or_corrupt(&buffer));
    }

    #[test]
    fn near_silent_buffer_is_flagged() {
        let buffer = DecodedAudioBuffer {
            sample_rate: 44_100,
            channels: 1,
            samples: vec![0.0001; 44_100],
        };

        assert!(is_likely_silent_or_corrupt(&buffer));
    }

    #[test]
    fn loud_tone_is_not_flagged_as_corrupt() {
        let buffer = sine_wave(440.0, 1.0, 44_100, 0.8);

        assert!(!is_likely_silent_or_corrupt(&buffer));
    }

    #[test]
    fn measure_reports_peak_near_zero_db_for_full_scale_tone() {
        let buffer = sine_wave(440.0, 0.5, 44_100, 1.0);

        let measurements = measure(&buffer);

        assert!(measurements.peak_db > -1.0);
    }

    #[test]
    fn low_frequency_tone_scores_higher_low_frequency_energy_than_high_tone() {
        let low = sine_wave(80.0, 1.0, 44_100, 0.8);
        let high = sine_wave(8_000.0, 1.0, 44_100, 0.8);

        let low_measurements = measure(&low);
        let high_measurements = measure(&high);

        assert!(low_measurements.low_frequency_energy > high_measurements.low_frequency_energy);
    }

    #[test]
    fn click_train_estimates_plausible_tempo() {
        let sample_rate = 44_100u32;
        let bpm = 120.0f32;
        let duration_secs = 4.0f32;
        let interval_secs = 60.0 / bpm;
        let click_len = (0.01 * sample_rate as f32) as usize;

        let total_samples = (duration_secs * sample_rate as f32) as usize;
        let mut samples = vec![0.0f32; total_samples];
        let interval_samples = (interval_secs * sample_rate as f32) as usize;

        let mut position = 0usize;
        while position + click_len < samples.len() {
            for offset in 0..click_len {
                samples[position + offset] = 0.9;
            }
            position += interval_samples;
        }

        let buffer = DecodedAudioBuffer {
            sample_rate,
            channels: 1,
            samples,
        };

        let estimate = estimate_tempo(&buffer).expect("tempo estimate");

        assert!(
            (estimate.bpm - bpm).abs() <= 4.0
                || (estimate.bpm - bpm * 2.0).abs() <= 4.0
                || (estimate.bpm - bpm / 2.0).abs() <= 4.0,
            "expected a tempo near {bpm} or a harmonic, got {}",
            estimate.bpm
        );
    }

    #[test]
    fn short_clip_has_no_tempo_estimate() {
        let buffer = sine_wave(440.0, 0.2, 44_100, 0.8);

        assert_eq!(estimate_tempo(&buffer), None);
    }

    #[test]
    fn sharp_short_click_suggests_impact() {
        let sample_rate = 44_100u32;
        let mut samples = vec![0.0f32; (0.3 * sample_rate as f32) as usize];
        for sample in samples.iter_mut().skip(1000).take(50) {
            *sample = 0.95;
        }

        let buffer = DecodedAudioBuffer {
            sample_rate,
            channels: 1,
            samples,
        };
        let measurements = measure(&buffer);

        let tags = suggest_action_tags(&buffer, measurements);

        assert!(tags.contains(&ActionTag::Impact));
    }

    #[test]
    fn ramping_amplitude_suggests_rise() {
        let sample_rate = 44_100u32;
        let duration_secs = 1.5f32;
        let sample_count = (duration_secs * sample_rate as f32) as usize;
        let samples: Vec<f32> = (0..sample_count)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                let envelope = t / duration_secs;
                envelope * (2.0 * std::f32::consts::PI * 300.0 * t).sin()
            })
            .collect();

        let buffer = DecodedAudioBuffer {
            sample_rate,
            channels: 1,
            samples,
        };
        let measurements = measure(&buffer);

        let tags = suggest_action_tags(&buffer, measurements);

        assert!(tags.contains(&ActionTag::Rise));
    }

    #[test]
    fn estimates_pitch_of_pure_tone_near_a4() {
        let buffer = sine_wave(440.0, 1.0, 44_100, 0.8);

        let estimate = estimate_pitch(&buffer).expect("pitch estimate");

        assert!((estimate.frequency_hz - 440.0).abs() < 5.0);
        assert_eq!(estimate.note_name, "A4");
    }

    #[test]
    fn short_buffer_has_no_pitch_estimate() {
        let buffer = DecodedAudioBuffer {
            sample_rate: 44_100,
            channels: 1,
            samples: vec![0.1; 100],
        };

        assert_eq!(estimate_pitch(&buffer), None);
    }

    #[test]
    fn note_name_maps_known_frequencies() {
        assert_eq!(note_name_for_frequency(440.0), "A4");
        assert_eq!(note_name_for_frequency(261.63), "C4");
    }

    #[test]
    fn action_tag_names_match_starter_taxonomy() {
        assert_eq!(ActionTag::Impact.as_str(), "Impact");
        assert_eq!(ActionTag::Whoosh.as_str(), "Whoosh");
        assert_eq!(ActionTag::Rise.as_str(), "Rise");
    }
}
