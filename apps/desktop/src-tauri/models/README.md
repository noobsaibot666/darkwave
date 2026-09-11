# Bundled models

Drop the instrument-detection model here:

- `yamnet.onnx` — YAMNet (Google AudioSet), ONNX, waveform input
- `yamnet_class_map.csv` — the standard TF-Hub class map (`index,mid,display_name`)

Both are gitignored (the `.onnx` is ~15 MB). Without them the Instrument
Detection page shows an empty state and `InstrumentDetection` jobs stay
pending — nothing breaks. See `docs/development/instrument-model.md` for
where to get the files, the exact I/O contract, and licensing.

This `README.md` is committed only so the bundler's `models/*` resource
glob always matches something.
