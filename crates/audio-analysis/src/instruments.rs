//! Instrument detection over a YAMNet (Google AudioSet) ONNX model.
//!
//! YAMNet takes a 16 kHz mono waveform and returns per-frame scores over its
//! 521 AudioSet classes; the model's own graph does the log-mel frontend, so
//! all this module does is resample, run inference, mean-pool the frames,
//! and fold the instrument-relevant classes onto a small clean label set for
//! the library filter.
//!
//! The model file is not vendored — [`load_instrument_model`] returns `None`
//! when it (or its class map) is absent, and [`detect_instruments`] then
//! yields nothing, exactly like the similarity worker degrades when its
//! sidecar is missing. See `docs/development/instrument-model.md`.

use std::path::Path;

use audio_metadata::DecodedAudioBuffer;
use ort::session::Session;
use ort::value::Tensor;

use crate::{mono_samples, resample_linear};

const YAMNET_SAMPLE_RATE: u32 = 16_000;
/// One 0.96 s YAMNet frame is 15 360 samples at 16 kHz; below roughly a
/// second there's nothing for it to score.
const MIN_SAMPLES_FOR_DETECTION: usize = YAMNET_SAMPLE_RATE as usize;
/// Cap the waveform fed to the model. Instrumentation is stable enough
/// across a track that ~90 s taken from the middle is representative, and it
/// bounds inference cost on a 10-minute stem.
const MAX_SAMPLES_FOR_DETECTION: usize = YAMNET_SAMPLE_RATE as usize * 90;
/// Mean score a class must clear across the clip to count as "present".
/// Deliberately conservative — a spurious instrument on a filter chip is
/// worse than a missed one.
const PRESENCE_THRESHOLD: f32 = 0.15;
/// Never tag an asset with more than this many instruments; keeps the
/// per-asset chip row and the library facet legible.
const MAX_INSTRUMENTS_PER_ASSET: usize = 8;

#[derive(Clone, Debug, PartialEq)]
pub struct InstrumentPrediction {
    /// A cleaned display label (see `INSTRUMENT_LABEL_RULES`), e.g. "Guitar".
    pub instrument: String,
    /// Highest mean class score that folded into this label, ~0.0..=1.0.
    pub confidence: f32,
}

/// A loaded YAMNet session plus its class-index → display-name map.
pub struct InstrumentModel {
    session: Session,
    class_names: Vec<String>,
    input_name: String,
    scores_output_index: usize,
}

impl std::fmt::Debug for InstrumentModel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InstrumentModel")
            .field("classes", &self.class_names.len())
            .field("input_name", &self.input_name)
            .finish()
    }
}

/// Loads the YAMNet ONNX model at `model_path` and the AudioSet class map at
/// `class_map_path` (the standard TF-Hub `yamnet_class_map.csv`:
/// `index,mid,display_name`). Returns `None` if either file is missing or
/// unreadable, or the model won't open — the caller treats that as "no
/// instrument detection available", not an error.
pub fn load_instrument_model(model_path: &Path, class_map_path: &Path) -> Option<InstrumentModel> {
    if !model_path.is_file() || !class_map_path.is_file() {
        return None;
    }

    let class_names = parse_class_map(&std::fs::read_to_string(class_map_path).ok()?);
    if class_names.is_empty() {
        return None;
    }

    // Capped rather than ORT's default (one thread per logical core): this
    // session already only ever runs one inference at a time (the caller
    // serialises on a mutex — see the module doc on process_instrument_jobs),
    // so letting ONE call claim every core just means it fights the OTHER
    // background job kinds (waveform generation, audio analysis) that are
    // typically churning through the same large backlog concurrently, on
    // their own native threads, for the same CPU. Observed on a 10-core
    // machine: with this left at ORT's default, instrument detection alone
    // spun up threads across every core while waveform/analysis were also
    // mid-backlog, which is what made the whole app (including the UI
    // thread) feel starved rather than just "busy". 2 threads keeps this
    // model's own latency reasonable without trying to own the machine.
    let session = Session::builder()
        .ok()?
        .with_intra_threads(2)
        .ok()?
        .commit_from_file(model_path)
        .ok()?;

    let input_name = session.inputs.first()?.name.clone();
    // TF-Hub YAMNet exports order outputs as (scores, embeddings,
    // log_mel_spectrogram); match by name and fall back to the first.
    let scores_output_index = session
        .outputs
        .iter()
        .position(|output| output.name.to_ascii_lowercase().contains("score"))
        .unwrap_or(0);

    // YAMNet has 521 AudioSet classes; a wildly different count means the
    // class map and the model don't belong together, which would map raw
    // scores to the wrong instrument names. Log it rather than fail — a
    // near miss (an export with a padding class, say) still works.
    if !(500..=540).contains(&class_names.len()) {
        eprintln!(
            "instrument-detection: class map has {} entries (expected ~521 for YAMNet) — \
             labels may be misaligned; check docs/development/instrument-model.md",
            class_names.len()
        );
    }

    Some(InstrumentModel {
        session,
        class_names,
        input_name,
        scores_output_index,
    })
}

/// Runs instrument detection on `buffer`. Empty vec when the clip is too
/// short, inference fails, or nothing clears `PRESENCE_THRESHOLD`.
/// `&mut` because `ort::Session::run` needs it.
pub fn detect_instruments(
    model: &mut InstrumentModel,
    buffer: &DecodedAudioBuffer,
) -> Vec<InstrumentPrediction> {
    // Trim to (about) the middle window this function ends up using *before*
    // mono-mixing and resampling, not after. Both of those scale with the
    // whole file's length, not the ~90s MAX_SAMPLES_FOR_DETECTION actually
    // keeps — mono-mixing and resampling a full multi-minute track only to
    // throw away everything outside the last window was wasted CPU
    // proportional to (duration / 90s), and on a library with many
    // multi-minute tracks that was the largest single cost in this
    // function, well past decode and past the model's own inference time.
    let windowed = trim_to_detection_window(buffer);
    let mono = mono_samples(&windowed);
    let resampled = resample_linear(&mono, windowed.sample_rate.max(1), YAMNET_SAMPLE_RATE);
    if resampled.len() < MIN_SAMPLES_FOR_DETECTION {
        return Vec::new();
    }

    let waveform: &[f32] = if resampled.len() > MAX_SAMPLES_FOR_DETECTION {
        let start = (resampled.len() - MAX_SAMPLES_FOR_DETECTION) / 2;
        &resampled[start..start + MAX_SAMPLES_FOR_DETECTION]
    } else {
        &resampled
    };

    let Some(class_means) = run_inference(model, waveform) else {
        return Vec::new();
    };
    aggregate_predictions(&model.class_names, &class_means)
}

/// Slices `buffer` down to (approximately) the middle
/// `MAX_SAMPLES_FOR_DETECTION` post-resample window, measured in the
/// buffer's *own* sample rate and frame-aligned across all channels —
/// cheap to slightly over-estimate (resampling still trims exactly to
/// MAX_SAMPLES_FOR_DETECTION afterward), so this only needs to be in the
/// right ballpark, not exact. A short clip (nothing to trim) is returned
/// untouched, borrowing nothing extra.
fn trim_to_detection_window(buffer: &DecodedAudioBuffer) -> DecodedAudioBuffer {
    let channels = buffer.channels.max(1) as usize;
    let total_frames = buffer.samples.len() / channels;
    let window_frames =
        ((MAX_SAMPLES_FOR_DETECTION as u64 * buffer.sample_rate.max(1) as u64) / YAMNET_SAMPLE_RATE as u64) as usize;

    if window_frames == 0 || total_frames <= window_frames {
        return DecodedAudioBuffer {
            sample_rate: buffer.sample_rate,
            channels: buffer.channels,
            samples: buffer.samples.clone(),
        };
    }

    let start_frame = (total_frames - window_frames) / 2;
    let start = start_frame * channels;
    let end = ((start_frame + window_frames) * channels).min(buffer.samples.len());
    DecodedAudioBuffer {
        sample_rate: buffer.sample_rate,
        channels: buffer.channels,
        samples: buffer.samples[start..end].to_vec(),
    }
}

/// Feeds the waveform to the model and returns the per-class mean score
/// across all frames, or `None` on any inference failure.
fn run_inference(model: &mut InstrumentModel, waveform: &[f32]) -> Option<Vec<f32>> {
    let input = Tensor::from_array((vec![waveform.len()], waveform.to_vec())).ok()?;
    let outputs = model
        .session
        .run(ort::inputs![model.input_name.as_str() => input])
        .ok()?;

    let (shape, data) = outputs[model.scores_output_index]
        .try_extract_tensor::<f32>()
        .ok()?;

    let num_classes = *shape.last()? as usize;
    if num_classes == 0 || data.len() < num_classes {
        return None;
    }
    let frames = data.len() / num_classes;

    let mut means = vec![0.0_f32; num_classes];
    for frame in 0..frames {
        let base = frame * num_classes;
        for (class, mean) in means.iter_mut().enumerate() {
            *mean += data[base + class];
        }
    }
    for mean in &mut means {
        *mean /= frames as f32;
    }
    Some(means)
}

/// Folds raw class scores onto the clean instrument label set, keeps the
/// strongest score per label, drops anything under threshold, and returns
/// the top `MAX_INSTRUMENTS_PER_ASSET` by confidence.
fn aggregate_predictions(class_names: &[String], class_means: &[f32]) -> Vec<InstrumentPrediction> {
    let mut best: Vec<(&'static str, f32)> = Vec::new();

    for (index, &score) in class_means.iter().enumerate() {
        if score < PRESENCE_THRESHOLD {
            continue;
        }
        let Some(raw_name) = class_names.get(index) else {
            continue;
        };
        let Some(label) = label_for_class(raw_name) else {
            continue;
        };
        match best.iter_mut().find(|(existing, _)| *existing == label) {
            Some((_, current)) if *current >= score => {}
            Some(entry) => entry.1 = score,
            None => best.push((label, score)),
        }
    }

    best.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    best.truncate(MAX_INSTRUMENTS_PER_ASSET);
    best.into_iter()
        .map(|(instrument, confidence)| InstrumentPrediction {
            instrument: instrument.to_string(),
            confidence,
        })
        .collect()
}

/// AudioSet display-name (lowercased) substring → clean instrument label.
/// Ordered most-specific first so `.find` picks e.g. "Bass" for "Bass
/// guitar" before the generic "Guitar" rule. Several raw classes fold into
/// one label on purpose.
const INSTRUMENT_LABEL_RULES: &[(&str, &str)] = &[
    ("electric piano", "Electric Piano"),
    ("piano", "Piano"),
    ("organ", "Organ"),
    ("synthesizer", "Synth"),
    ("sampler", "Synth"),
    ("mellotron", "Synth"),
    ("bass guitar", "Bass"),
    ("double bass", "Upright Bass"),
    ("electric guitar", "Electric Guitar"),
    ("steel guitar", "Guitar"),
    ("slide guitar", "Guitar"),
    ("acoustic guitar", "Acoustic Guitar"),
    ("tapping (guitar technique)", "Guitar"),
    ("guitar", "Guitar"),
    ("banjo", "Banjo"),
    ("sitar", "Sitar"),
    ("mandolin", "Mandolin"),
    ("ukulele", "Ukulele"),
    ("harp", "Harp"),
    ("string section", "Strings"),
    ("pizzicato", "Strings"),
    ("orchestra", "Orchestra"),
    ("violin", "Strings"),
    ("fiddle", "Strings"),
    ("cello", "Strings"),
    ("drum kit", "Drums"),
    ("drum machine", "Drum Machine"),
    ("snare drum", "Drums"),
    ("bass drum", "Drums"),
    ("hi-hat", "Drums"),
    ("rimshot", "Drums"),
    ("drum roll", "Drums"),
    ("cymbal", "Cymbals"),
    ("tambourine", "Percussion"),
    ("tabla", "Percussion"),
    ("timpani", "Timpani"),
    ("mallet percussion", "Mallets"),
    ("marimba", "Mallets"),
    ("xylophone", "Mallets"),
    ("glockenspiel", "Mallets"),
    ("vibraphone", "Mallets"),
    ("steelpan", "Mallets"),
    ("drum", "Drums"),
    ("percussion", "Percussion"),
    ("trumpet", "Trumpet"),
    ("trombone", "Trombone"),
    ("french horn", "Horn"),
    ("cornet", "Trumpet"),
    ("brass instrument", "Brass"),
    ("saxophone", "Saxophone"),
    ("clarinet", "Clarinet"),
    ("flute", "Flute"),
    ("oboe", "Woodwinds"),
    ("bassoon", "Woodwinds"),
    ("harmonica", "Harmonica"),
    ("accordion", "Accordion"),
    ("bagpipes", "Bagpipes"),
    ("didgeridoo", "Didgeridoo"),
    ("theremin", "Theremin"),
    ("bell", "Bells"),
    ("wind chime", "Bells"),
    ("chime", "Bells"),
    ("choir", "Choir"),
    ("singing", "Vocals"),
];

fn label_for_class(raw_name: &str) -> Option<&'static str> {
    let lower = raw_name.to_ascii_lowercase();
    INSTRUMENT_LABEL_RULES
        .iter()
        .find(|(needle, _)| lower.contains(needle))
        .map(|(_, label)| *label)
}

/// Parses a TF-Hub `yamnet_class_map.csv` (`index,mid,display_name`, with a
/// header row). Only the display name matters here; it may be quoted and
/// contain commas.
fn parse_class_map(csv: &str) -> Vec<String> {
    csv.lines()
        .skip(1)
        .filter_map(|line| {
            let line = line.trim();
            if line.is_empty() {
                return None;
            }
            // Skip `index,` then `mid,` (mid never contains a comma), the
            // rest is the display name.
            let after_index = line.split_once(',')?.1;
            let display = after_index.split_once(',')?.1.trim();
            Some(display.trim_matches('"').to_string())
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn label_rules_fold_related_classes_and_ignore_non_instruments() {
        assert_eq!(label_for_class("Electric guitar"), Some("Electric Guitar"));
        assert_eq!(label_for_class("Slide guitar"), Some("Guitar"));
        assert_eq!(label_for_class("Bass guitar"), Some("Bass"));
        assert_eq!(label_for_class("Snare drum"), Some("Drums"));
        assert_eq!(label_for_class("Violin, fiddle"), Some("Strings"));
        assert_eq!(label_for_class("String section"), Some("Strings"));
        assert_eq!(label_for_class("Speech"), None);
        assert_eq!(label_for_class("Dog"), None);
    }

    #[test]
    fn aggregate_folds_by_label_thresholds_sorts_and_caps() {
        let class_names: Vec<String> = vec![
            "Acoustic guitar".into(), // 0
            "Electric guitar".into(), // 1 -> different label, stronger
            "Speech".into(),          // 2 -> ignored
            "Piano".into(),           // 3 -> below threshold
            "Snare drum".into(),      // 4
            "Bass drum".into(),       // 5 -> same label as 4, weaker
        ];
        let means = [0.40_f32, 0.80, 0.99, 0.05, 0.60, 0.30];

        let predictions = aggregate_predictions(&class_names, &means);

        assert_eq!(
            predictions,
            vec![
                InstrumentPrediction { instrument: "Electric Guitar".into(), confidence: 0.80 },
                InstrumentPrediction { instrument: "Drums".into(), confidence: 0.60 },
                InstrumentPrediction { instrument: "Acoustic Guitar".into(), confidence: 0.40 },
            ]
        );
    }

    #[test]
    fn parse_class_map_handles_header_quotes_and_commas() {
        let csv = "index,mid,display_name\n\
                   0,/m/09x0r,Speech\n\
                   137,/m/04szw,\"Musical instrument\"\n\
                   139,/m/0342h,Guitar\n\
                   244,/m/07y_7,\"Violin, fiddle\"\n";

        let names = parse_class_map(csv);

        assert_eq!(names, vec!["Speech", "Musical instrument", "Guitar", "Violin, fiddle"]);
    }

    #[test]
    fn trim_to_detection_window_leaves_a_short_clip_untouched() {
        let buffer = DecodedAudioBuffer {
            sample_rate: 44_100,
            channels: 2,
            samples: vec![0.0; 44_100 * 2 * 5], // 5s, well under the ~90s window
        };

        let trimmed = trim_to_detection_window(&buffer);

        assert_eq!(trimmed.samples.len(), buffer.samples.len());
    }

    #[test]
    fn trim_to_detection_window_shrinks_a_long_clip_to_about_90_seconds() {
        // The bug this guards against: mono-mixing and resampling a whole
        // multi-minute track before detect_instruments trims to its ~90s
        // window scaled CPU cost with the entire file, not the window it
        // actually uses — the dominant cost for a long library track.
        let sample_rate = 44_100u32;
        let channels = 2u16;
        let total_seconds = 200;
        let buffer = DecodedAudioBuffer {
            sample_rate,
            channels,
            samples: vec![0.0; sample_rate as usize * channels as usize * total_seconds],
        };

        let trimmed = trim_to_detection_window(&buffer);
        let trimmed_seconds = (trimmed.samples.len() / channels as usize) as f64 / sample_rate as f64;

        assert!(
            trimmed_seconds < total_seconds as f64 / 2.0,
            "expected real shrinkage, got {trimmed_seconds}s of {total_seconds}s"
        );
        assert!((trimmed_seconds - 90.0).abs() < 1.0, "expected ~90s, got {trimmed_seconds}s");
        assert_eq!(trimmed.samples.len() % channels as usize, 0, "must stay frame-aligned");
    }

    #[test]
    fn missing_model_files_yield_no_model() {
        assert!(load_instrument_model(
            Path::new("/nonexistent/yamnet.onnx"),
            Path::new("/nonexistent/class_map.csv"),
        )
        .is_none());
    }
}
