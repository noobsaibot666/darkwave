use audio_metadata::DecodedAudioBuffer;
use pitch_detection::detector::mcleod::McLeodDetector;
use pitch_detection::detector::PitchDetector;
use rustfft::{num_complex::Complex, FftPlanner};

mod instruments;
pub use instruments::{
    detect_instruments, load_instrument_model, InstrumentModel, InstrumentPrediction,
};

/// Pitch-class names, index 0 = C. Shared by the pitch-note formatter and
/// the key detector.
const PITCH_CLASS_NAMES: [&str; 12] = [
    "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#", "A", "A#", "B",
];

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

/// Files under this length are assumed to be sound effects rather than
/// music or ambience — matches the intent of import-time's file-size-based
/// heuristic (`SOUND_EFFECT_MAX_BYTES` in `import-pipeline`, ~28s at a
/// typical 16-bit/44.1kHz stereo bitrate), now using real decoded duration
/// instead of a byte-count proxy for it.
const CLASSIFICATION_SHORT_DURATION_MS: i64 = 30_000;

/// Below this, a detected tempo is treated as noise rather than a real
/// beat — autocorrelation on non-rhythmic material (a drone, room tone,
/// wind) can still return *some* bpm value with low confidence.
const CLASSIFICATION_MIN_BPM_CONFIDENCE: f64 = 0.5;

/// Threshold for treating a clip as *primarily* speech for classification
/// purposes. Deliberately higher than the frontend's `VOCAL_RATIO_THRESHOLD`
/// (0.15, used only to flag "has some vocals" for filtering/row icons) —
/// here we're deciding whether the whole track basically *is* a voiceover,
/// which needs a stronger majority-of-the-clip bar than "contains a chorus."
const CLASSIFICATION_VOCAL_RATIO_THRESHOLD: f64 = 0.5;

/// Best-effort media-type classification from the signals `analyze_asset_audio`
/// already computes — duration, detected tempo, and vocal ratio. This is the
/// "first funnel": real acoustic signal, not filename guessing, deciding a
/// file's category before anyone has to review it by hand. Returns `None`
/// when there isn't enough signal to decide (no duration at all), so a
/// caller can leave whatever classification already exists untouched rather
/// than overwriting it with a guess.
///
/// Deliberately few signals, all already computed for every analyzed file at
/// no extra cost — not a new classifier model, consistent with this
/// project's preference for small hand-written heuristics (see ADR 0025)
/// over new ML dependencies: short clips are effects; long clips with a
/// confident beat are music (even if they also have vocals — a sung chorus
/// over a beat is a soundtrack, not a voiceover); long clips without a beat
/// but with substantial vocals are voiceover; everything else long is
/// ambience.
pub fn classify_media_type_from_analysis(
    duration_ms: Option<i64>,
    bpm: Option<f64>,
    bpm_confidence: Option<f64>,
    vocal_ratio: Option<f64>,
) -> Option<&'static str> {
    let duration_ms = duration_ms?;

    if duration_ms <= CLASSIFICATION_SHORT_DURATION_MS {
        return Some("sound_effect");
    }

    let has_confident_beat = bpm.is_some()
        && bpm_confidence.is_some_and(|confidence| confidence >= CLASSIFICATION_MIN_BPM_CONFIDENCE);
    if has_confident_beat {
        return Some("music");
    }

    let has_substantial_vocals =
        vocal_ratio.is_some_and(|ratio| ratio >= CLASSIFICATION_VOCAL_RATIO_THRESHOLD);

    Some(if has_substantial_vocals { "voiceover" } else { "ambience" })
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

// --- Musical key (Krumhansl-Schmuckler) --------------------------------------

const KEY_FFT_SIZE: usize = 8192;
const KEY_HOP_SIZE: usize = 4096;
/// Analyse at most this many frames, taken from the middle of the clip — a
/// track's key is stable enough that ~60s is plenty, and it bounds the cost
/// on a 10-minute stem.
const KEY_MAX_FRAMES: usize = 700;
const KEY_MIN_SAMPLES: usize = KEY_FFT_SIZE * 3;
/// Frequency band folded into the chromagram: below ~A1 is mostly rumble,
/// above ~C7 is mostly harmonics and noise for this purpose.
const KEY_MIN_HZ: f32 = 55.0;
const KEY_MAX_HZ: f32 = 2_093.0;
/// C0 in Hz — the reference for mapping a frequency to a pitch class.
const KEY_REFERENCE_C0_HZ: f32 = 16.351_6;
/// Below this correlation the material isn't tonal enough to call a key
/// (drum loops, atonal SFX, noise beds) — report `None` rather than a guess.
const KEY_MIN_STRENGTH: f32 = 0.6;

/// Krumhansl-Kessler major/minor key profiles (the canonical published
/// weights). Index 0 is the tonic.
const KS_MAJOR_PROFILE: [f32; 12] = [
    6.35, 2.23, 3.48, 2.33, 4.38, 4.09, 2.52, 5.19, 2.39, 3.66, 2.29, 2.88,
];
const KS_MINOR_PROFILE: [f32; 12] = [
    6.33, 2.68, 3.52, 5.38, 2.60, 3.53, 2.54, 4.75, 3.98, 2.69, 3.34, 3.17,
];

#[derive(Clone, Debug, PartialEq)]
pub struct KeyEstimate {
    /// Full key name, e.g. "C major" or "A minor". A real key from a
    /// chromagram/profile correlation over the whole clip — distinct from
    /// `estimate_pitch`, which reports a single monophonic note.
    pub key: String,
    /// Tonic pitch class only, e.g. "C", "A#".
    pub tonic: String,
    pub is_minor: bool,
    /// Pearson correlation of the folded chromagram against the winning key
    /// profile, ~0.0..=1.0. Higher = more strongly tonal.
    pub strength: f32,
}

/// Best-effort musical-key detection via the Krumhansl-Schmuckler method:
/// fold the clip into a 12-bin pitch-class chromagram, then correlate it
/// against all 24 major/minor key profiles and take the best fit. Returns
/// `None` for clips too short to analyse or material too atonal to call
/// (correlation below `KEY_MIN_STRENGTH`). Works on polyphonic music,
/// unlike `estimate_pitch`.
pub fn estimate_key(buffer: &DecodedAudioBuffer) -> Option<KeyEstimate> {
    let mono = mono_samples(buffer);
    let sample_rate = buffer.sample_rate.max(1) as f32;
    if mono.len() < KEY_MIN_SAMPLES || sample_rate <= 0.0 {
        return None;
    }

    // Centre a bounded analysis window on the middle of the clip.
    let max_span = KEY_MAX_FRAMES * KEY_HOP_SIZE + KEY_FFT_SIZE;
    let (window_start, window_end) = if mono.len() > max_span {
        let start = (mono.len() - max_span) / 2;
        (start, start + max_span)
    } else {
        (0, mono.len())
    };
    let region = &mono[window_start..window_end];

    let hann: Vec<f32> = (0..KEY_FFT_SIZE)
        .map(|n| {
            0.5 - 0.5 * (2.0 * std::f32::consts::PI * n as f32 / (KEY_FFT_SIZE as f32 - 1.0)).cos()
        })
        .collect();

    // Precompute the pitch class each usable FFT bin maps to.
    let nyquist_bin = KEY_FFT_SIZE / 2;
    let bin_pitch_class: Vec<Option<usize>> = (0..nyquist_bin)
        .map(|bin| {
            let freq = bin as f32 * sample_rate / KEY_FFT_SIZE as f32;
            if !(KEY_MIN_HZ..=KEY_MAX_HZ).contains(&freq) {
                return None;
            }
            let pc = (12.0 * (freq / KEY_REFERENCE_C0_HZ).log2()).round() as i64;
            Some(pc.rem_euclid(12) as usize)
        })
        .collect();

    let fft = FftPlanner::<f32>::new().plan_fft_forward(KEY_FFT_SIZE);
    let mut spectrum = vec![Complex::new(0.0_f32, 0.0); KEY_FFT_SIZE];
    let mut chroma = [0.0_f32; 12];
    let mut frames = 0usize;

    let mut offset = 0usize;
    while offset + KEY_FFT_SIZE <= region.len() && frames < KEY_MAX_FRAMES {
        for (i, slot) in spectrum.iter_mut().enumerate() {
            *slot = Complex::new(region[offset + i] * hann[i], 0.0);
        }
        fft.process(&mut spectrum);

        for (bin, pitch_class) in bin_pitch_class.iter().enumerate() {
            if let Some(pc) = pitch_class {
                chroma[*pc] += spectrum[bin].norm();
            }
        }

        offset += KEY_HOP_SIZE;
        frames += 1;
    }

    if frames == 0 || chroma.iter().sum::<f32>() <= f32::EPSILON {
        return None;
    }

    let (tonic, is_minor, strength) = best_key_for_chroma(&chroma)?;
    if strength < KEY_MIN_STRENGTH {
        return None;
    }

    let tonic_name = PITCH_CLASS_NAMES[tonic].to_string();
    Some(KeyEstimate {
        key: format!("{} {}", tonic_name, if is_minor { "minor" } else { "major" }),
        tonic: tonic_name,
        is_minor,
        strength,
    })
}

/// Correlates `chroma` against every rotation of both KS profiles and
/// returns `(tonic pitch class, is_minor, correlation)` for the best fit.
fn best_key_for_chroma(chroma: &[f32; 12]) -> Option<(usize, bool, f32)> {
    let mut best: Option<(usize, bool, f32)> = None;

    for tonic in 0..12 {
        for (is_minor, profile) in [(false, &KS_MAJOR_PROFILE), (true, &KS_MINOR_PROFILE)] {
            let rotated: [f32; 12] = std::array::from_fn(|i| profile[(i + 12 - tonic) % 12]);
            let correlation = pearson_correlation(chroma, &rotated);
            let improves = match best {
                Some((_, _, current)) => correlation > current,
                None => true,
            };
            if improves {
                best = Some((tonic, is_minor, correlation));
            }
        }
    }

    best.map(|(tonic, is_minor, correlation)| (tonic, is_minor, correlation.clamp(0.0, 1.0)))
}

fn pearson_correlation(a: &[f32; 12], b: &[f32; 12]) -> f32 {
    let n = 12.0_f32;
    let mean_a = a.iter().sum::<f32>() / n;
    let mean_b = b.iter().sum::<f32>() / n;

    let mut covariance = 0.0_f32;
    let mut variance_a = 0.0_f32;
    let mut variance_b = 0.0_f32;
    for i in 0..12 {
        let da = a[i] - mean_a;
        let db = b[i] - mean_b;
        covariance += da * db;
        variance_a += da * da;
        variance_b += db * db;
    }

    let denominator = (variance_a * variance_b).sqrt();
    if denominator <= f32::EPSILON {
        0.0
    } else {
        covariance / denominator
    }
}

const VAD_SAMPLE_RATE: u32 = 16_000;
const VAD_CHUNK_SIZE: usize = 512;
const VAD_SPEECH_PROBABILITY_THRESHOLD: f32 = 0.5;
const MIN_SECONDS_FOR_VOCAL_DETECTION: f32 = 1.0;

/// Fraction of the clip Silero VAD classifies as speech (0.0-1.0), or `None`
/// for clips too short to meaningfully sample, or if the model fails to
/// load. Silero only accepts 8kHz or 16kHz mono input, so this downmixes and
/// resamples a copy of the buffer first — the original is untouched.
pub fn detect_vocal_ratio(buffer: &DecodedAudioBuffer) -> Option<f32> {
    let mono = mono_samples(buffer);
    let duration_secs = mono.len() as f32 / buffer.sample_rate.max(1) as f32;
    if duration_secs < MIN_SECONDS_FOR_VOCAL_DETECTION {
        return None;
    }

    let resampled = resample_linear(&mono, buffer.sample_rate.max(1), VAD_SAMPLE_RATE);
    if resampled.len() < VAD_CHUNK_SIZE {
        return None;
    }

    let mut vad = voice_activity_detector::VoiceActivityDetector::builder()
        .sample_rate(VAD_SAMPLE_RATE)
        .chunk_size(VAD_CHUNK_SIZE)
        .build()
        .ok()?;

    let mut speech_chunks = 0usize;
    let mut total_chunks = 0usize;
    for chunk in resampled.chunks(VAD_CHUNK_SIZE) {
        // Drop a trailing partial chunk rather than padding it with silence,
        // which would bias it toward "non-speech".
        if chunk.len() < VAD_CHUNK_SIZE {
            break;
        }
        let probability = vad.predict(chunk.to_vec());
        total_chunks += 1;
        if probability > VAD_SPEECH_PROBABILITY_THRESHOLD {
            speech_chunks += 1;
        }
    }

    if total_chunks == 0 {
        return None;
    }

    Some(speech_chunks as f32 / total_chunks as f32)
}

/// Simple linear-interpolation resampler. Not audiophile-grade, but that's
/// not the goal — it just needs to hand the VAD model a representative
/// 16kHz signal, not produce audio for playback.
pub(crate) fn resample_linear(input: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || input.is_empty() {
        return input.to_vec();
    }

    let ratio = from_rate as f64 / to_rate as f64;
    let output_len = (input.len() as f64 / ratio).floor() as usize;
    (0..output_len)
        .map(|i| {
            let position = i as f64 * ratio;
            let index = position.floor() as usize;
            let frac = (position - index as f64) as f32;
            let a = input[index.min(input.len() - 1)];
            let b = input[(index + 1).min(input.len() - 1)];
            a + (b - a) * frac
        })
        .collect()
}

fn note_name_for_frequency(frequency_hz: f32) -> String {
    if frequency_hz <= 0.0 {
        return "?".to_string();
    }

    let midi = (69.0 + 12.0 * (frequency_hz / 440.0).log2()).round() as i32;
    let note_index = midi.rem_euclid(12) as usize;
    let octave = midi.div_euclid(12) - 1;

    format!("{}{octave}", PITCH_CLASS_NAMES[note_index])
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

pub(crate) fn mono_samples(buffer: &DecodedAudioBuffer) -> Vec<f32> {
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

    #[test]
    fn classification_treats_short_clips_as_sound_effects_regardless_of_tempo() {
        assert_eq!(
            classify_media_type_from_analysis(Some(5_000), Some(120.0), Some(0.9), None),
            Some("sound_effect")
        );
        assert_eq!(
            classify_media_type_from_analysis(Some(30_000), None, None, Some(0.9)),
            Some("sound_effect")
        );
    }

    #[test]
    fn classification_treats_long_confident_tempo_as_music_even_with_vocals() {
        assert_eq!(
            classify_media_type_from_analysis(Some(180_000), Some(96.0), Some(0.8), None),
            Some("music")
        );
        // A sung chorus over a real beat is a soundtrack, not a voiceover —
        // the beat wins even when vocals are dominant (bug-log-v2: long
        // vocal tracks were wrongly ending up as sound effects/ambience
        // instead of soundtracks).
        assert_eq!(
            classify_media_type_from_analysis(Some(180_000), Some(96.0), Some(0.8), Some(0.9)),
            Some("music")
        );
    }

    #[test]
    fn classification_treats_long_clips_with_substantial_vocals_and_no_beat_as_voiceover() {
        assert_eq!(
            classify_media_type_from_analysis(Some(180_000), None, None, Some(0.6)),
            Some("voiceover")
        );
    }

    #[test]
    fn classification_treats_long_clips_without_a_confident_beat_or_vocals_as_ambience() {
        assert_eq!(
            classify_media_type_from_analysis(Some(180_000), None, None, None),
            Some("ambience")
        );
        // A spurious low-confidence tempo on non-rhythmic material
        // shouldn't be enough to call it "music".
        assert_eq!(
            classify_media_type_from_analysis(Some(180_000), Some(70.0), Some(0.2), None),
            Some("ambience")
        );
        // Some incidental speech in the background isn't enough to call the
        // whole clip a voiceover.
        assert_eq!(
            classify_media_type_from_analysis(Some(180_000), None, None, Some(0.2)),
            Some("ambience")
        );
    }

    #[test]
    fn classification_declines_to_guess_without_a_duration() {
        assert_eq!(
            classify_media_type_from_analysis(None, Some(120.0), Some(0.9), Some(0.9)),
            None
        );
    }

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
    fn resample_linear_preserves_a_constant_signal() {
        let input = vec![0.5f32; 44_100];

        let resampled = resample_linear(&input, 44_100, 16_000);

        assert!((resampled.len() as i64 - 16_000).abs() <= 1);
        assert!(resampled.iter().all(|sample| (sample - 0.5).abs() < 1e-6));
    }

    #[test]
    fn resample_linear_is_a_no_op_when_rates_match() {
        let input = vec![0.1, 0.2, 0.3];

        assert_eq!(resample_linear(&input, 16_000, 16_000), input);
    }

    #[test]
    fn too_short_clip_has_no_vocal_ratio() {
        let buffer = sine_wave(220.0, 0.3, 44_100, 0.8);

        assert_eq!(detect_vocal_ratio(&buffer), None);
    }

    #[test]
    fn pure_tone_scores_low_vocal_ratio() {
        // Silero VAD is trained on real speech; a steady sine tone is a
        // reasonable negative control even without a real speech sample to
        // hand — it should not be mistaken for a voice.
        let buffer = sine_wave(220.0, 3.0, 44_100, 0.6);

        let ratio = detect_vocal_ratio(&buffer).expect("long enough for a vocal ratio");

        assert!(ratio < 0.2, "expected a low vocal ratio for a pure tone, got {ratio}");
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

    /// Sum of sustained sine partials — a crude but tonally-unambiguous
    /// stand-in for a chord progression in the given key.
    fn chord_buffer(fundamentals_hz: &[f32], duration_secs: f32, sample_rate: u32) -> DecodedAudioBuffer {
        let sample_count = (duration_secs * sample_rate as f32) as usize;
        let samples = (0..sample_count)
            .map(|i| {
                let t = i as f32 / sample_rate as f32;
                fundamentals_hz
                    .iter()
                    .map(|freq| 0.2 * (2.0 * std::f32::consts::PI * freq * t).sin())
                    .sum()
            })
            .collect();

        DecodedAudioBuffer { sample_rate, channels: 1, samples }
    }

    #[test]
    fn estimates_c_major_from_a_c_major_triad() {
        // C4 E4 G4 + C5 E5 G5 — all diatonic to C major, tonic strongly present.
        let buffer = chord_buffer(
            &[261.63, 329.63, 392.00, 523.25, 659.25, 783.99],
            6.0,
            44_100,
        );

        let key = estimate_key(&buffer).expect("key estimate");
        assert_eq!(key.key, "C major");
        assert_eq!(key.tonic, "C");
        assert!(!key.is_minor);
        assert!(key.strength >= KEY_MIN_STRENGTH);
    }

    #[test]
    fn estimates_a_minor_from_an_a_minor_triad() {
        // A3 C4 E4 + A4 C5 E5 — diatonic to A minor.
        let buffer = chord_buffer(
            &[220.00, 261.63, 329.63, 440.00, 523.25, 659.25],
            6.0,
            44_100,
        );

        let key = estimate_key(&buffer).expect("key estimate");
        assert_eq!(key.tonic, "A");
        assert!(key.is_minor);
        assert_eq!(key.key, "A minor");
    }

    #[test]
    fn short_buffer_has_no_key_estimate() {
        let buffer = sine_wave(440.0, 0.2, 44_100, 0.8);
        assert_eq!(estimate_key(&buffer), None);
    }

    #[test]
    fn white_noise_has_no_confident_key() {
        // Deterministic pseudo-noise — flat pitch-class content, so no
        // profile correlates well enough to call a key.
        let sample_rate = 44_100;
        let mut state = 0x2545_F491_4F6C_DD1Du64;
        let samples = (0..sample_rate * 4)
            .map(|_| {
                state ^= state << 13;
                state ^= state >> 7;
                state ^= state << 17;
                (state as f32 / u64::MAX as f32) * 2.0 - 1.0
            })
            .collect();

        let buffer = DecodedAudioBuffer { sample_rate, channels: 1, samples };
        assert_eq!(estimate_key(&buffer), None);
    }

    #[test]
    fn action_tag_names_match_starter_taxonomy() {
        assert_eq!(ActionTag::Impact.as_str(), "Impact");
        assert_eq!(ActionTag::Whoosh.as_str(), "Whoosh");
        assert_eq!(ActionTag::Rise.as_str(), "Rise");
    }
}
