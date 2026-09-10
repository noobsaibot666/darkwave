# Instrument-detection model

The Sonic Radar "Instrument Detection" feature (ADR 0031) classifies each
imported sound with a YAMNet ONNX model. The model is **not vendored** —
without it the feature degrades gracefully (empty page, `InstrumentDetection`
jobs stay pending) exactly like the similarity worker does without its
sidecar.

## Files to install

Put both under `apps/desktop/src-tauri/models/` (gitignored). At runtime the
app also checks `<app-data>/models/` first, so a drop-in there needs no
rebuild — but requires an app restart to pick up.

| File | What | Size |
| --- | --- | --- |
| `yamnet.onnx` | YAMNet, **waveform input** variant | ~15 MB |
| `yamnet_class_map.csv` | TF-Hub class map, `index,mid,display_name`, header row | ~14 KB |

## Where to get them

YAMNet ships from TensorFlow Hub as a SavedModel/TFLite. Convert to ONNX
with `tf2onnx`, keeping the **1-D float32 waveform input** (16 kHz mono) and
the per-frame `scores` output — the graph already contains the log-mel
frontend, so nothing else is needed on the Rust side:

```
pip install tensorflow tensorflow-hub tf2onnx
python -m tf2onnx.convert --saved-model <yamnet_saved_model> --output yamnet.onnx --opset 13
```

The class map is `yamnet_class_map.csv` from the same TF-Hub asset
(`https://storage.googleapis.com/audioset/yamnet_class_map.csv`).

Verify the ONNX I/O before shipping:
- exactly one input, rank-1 float32 (`[num_samples]`)
- an output whose name contains `score`, shape `[num_frames, 521]`

The Rust side (`crates/audio-analysis/src/instruments.rs`) matches the input
by position and the scores output by the substring `score` (falling back to
output 0), mean-pools the frames, then folds AudioSet instrument classes
onto a small label set (`INSTRUMENT_LABEL_RULES`). If your export names or
orders outputs differently, adjust `load_instrument_model`.

## Licensing

- **YAMNet weights**: Apache-2.0 (Google).
- **AudioSet class ontology / `yamnet_class_map.csv`**: CC-BY-4.0 — keep the
  attribution with the bundled file.
- **ONNX Runtime** (via the `ort` crate, already linked for Silero VAD):
  MIT.

None of this is GPL, so — unlike the similarity worker — instrument
detection runs in-process, not in an isolated sidecar. Record the model +
class-map provenance in the release notes when first bundled.

## Behaviour without the model

`load_instrument_model` returns `None`; `process_instrument_jobs` claims no
jobs and returns 0; the pending `InstrumentDetection` jobs simply wait. The
"Instrument Detection" page shows: *"No instruments detected yet … the
detection model isn't installed."*
