---
title: Importing
description: Current import behavior for the first library core milestone.
---

Milestone 1 supports the first backend import path.

When a supported audio file is imported, Darkwave:

- Reads immediate file metadata such as extension and file size.
- Reads WAV embedded title, genre, and comment fields when present.
- Registers the asset in the local catalog.
- Records whether the asset is managed or referenced.
- Copies managed imports into the library media root under `Media/00`.
- Sets an initial media type when filename signals clearly imply music, ambience, or sound effects.
- Tracks the asset as locally available.
- Avoids creating a second asset row when the same content hash and file size already exist in the same library.
- Adds pending filename-based tag suggestions for review.
- Creates pending background jobs for metadata extraction, hashing, and waveform generation.
- Discovers watched-folder candidates only after supported audio files have stabilized.
- Maintains watched-folder size snapshots across polls so new files are imported only after a later stable scan.
- Ingests platform filesystem notifications and emits import candidates only after repeat stable events.
- Decodes WAV PCM files for cached-preview playback preparation.
- Supports a packaged-decoder provider interface for compressed MVP formats once release decoder artifacts are installed.
- Reports codec capability for imported extensions: native WAV PCM, packaged-decoder formats, or unsupported with conversion available.

Watched folders ignore incomplete browser download files such as `.crdownload`, `.download`, `.part`, and `.tmp`. A watched file is considered ready only after its file size has stabilized.

Referenced imports remain catalog-only and keep pointing at their original file path. Arbitrary-format decoder artifacts and licensing review remain release-build requirements.
