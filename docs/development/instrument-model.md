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

YAMNet ships from TensorFlow Hub as a `hub.load()`-able SavedModel. It
doesn't expose a `serving_default` signature with a plain waveform input by
default, so wrap it before converting — `hub.load()` returns
`(scores, embeddings, log_mel_spectrogram)` from a plain call, so a thin
`tf.Module` around it with an explicit `input_signature=[TensorSpec([None],
float32)]` and a `{"scores": scores}` return gives `tf2onnx` a concrete
signature to convert, and keeps the **1-D float32 waveform input** (16 kHz
mono) and a `scores`-named output the Rust side matches by substring:

```python
import tensorflow as tf, tensorflow_hub as hub
yamnet = hub.load("https://tfhub.dev/google/yamnet/1")

class YAMNetWaveformModel(tf.Module):
    def __init__(self, yamnet):
        super().__init__()
        self.yamnet = yamnet
    @tf.function(input_signature=[tf.TensorSpec(shape=[None], dtype=tf.float32, name="waveform")])
    def __call__(self, waveform):
        scores, embeddings, spectrogram = self.yamnet(waveform)
        return {"scores": scores}

wrapper = YAMNetWaveformModel(yamnet)
tf.saved_model.save(wrapper, "yamnet_saved_model", signatures={"serving_default": wrapper.__call__.get_concrete_function()})
```

```bash
pip install tensorflow tensorflow-hub tf2onnx onnx onnxruntime
python -m tf2onnx.convert --saved-model yamnet_saved_model --output yamnet.onnx --opset 13 --signature_def serving_default
```

Two toolchain snags hit in practice (as of late 2026) — worth checking
first rather than re-discovering:

- **TensorFlow has no wheel for very new Python versions** (nothing for
  3.14 at time of writing). Use whatever's the latest Python TF actually
  ships for — 3.10 worked — in a throwaway venv.
- **Pin `onnx` down from latest** if `import onnx` fails with
  `ml_dtypes` has no attribute `float4_e2m1fn` — TF 2.16's `ml_dtypes` pin
  is older than current `onnx` expects. `onnx==1.16.1` paired cleanly with
  `tensorflow==2.16.2` here.

The class map is `yamnet_class_map.csv` from the same TF-Hub asset. The
doc's previous URL (`storage.googleapis.com/audioset/yamnet_class_map.csv`)
now 404s — pull it from TensorFlow's own repo instead:
`https://raw.githubusercontent.com/tensorflow/models/master/research/audioset/yamnet/yamnet_class_map.csv`.

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
