# ADR 0036: Organizing sound effects into categories, and cleaning up junk files

## Status

Accepted.

## Context

Sound-effect libraries arrive as one flat folder of files whose *names*
already say what they are (`door-closing.mp3`, `Multimedia 770`,
`footsteps-2-soft.wav`), buried under vendor naming noise: Freesound's
`12345__username__` prefix, Epidemic Sound's `ES_… - SFX Producer`, and
take numbers. The sidebar's "Sound Effects › By category" list existed,
but it only showed the three seeded `action` tags (Impact/Whoosh/Rise), so
it organized nothing.

Separately, the Genesis library had 11 "sounds" that were macOS
AppleDouble sidecars (`._Shaft Drop 2.wav`). Finder writes these on
NAS/exFAT drives. Folder import already skipped dotfiles, but drag-drop
and the file picker hand them over explicitly, and they then failed
every analysis job.

## Decision

### Categories are tags first; folders on disk follow them

*Revised 2026-11-13.* The first version kept categories inside Darkwave
only, because every asset in a typical library is **Referenced** (it lives
in the user's own folder, e.g. `/Volumes/DATA/01_ADDS/_SoundFX/`, which
edit projects may link to). In practice the user's model of "organized"
is real folders, so Organize can now also **move the files** (on by
default in the review). The category itself is still a tag with facet
`category` (`storage::CATEGORY_FACET`). That tag is what the sidebar,
the category chips, the row badges and `export_asset_to_project`'s
`03_SFX/<Category>/` routing read. The folder on disk is a mirror of it.

Moving rules (`organize_assets` in `src-tauri/src/lib.rs`):

- The category folder is created **next to the file**. If the file is
  already inside a category folder (a re-sort), it's created next to that
  folder instead, so files move sideways and never nest
  (`category_folder_base`).
- **Never overwrite.** A taken name gets ` (2)`, ` (3)`…
  (`unique_destination`). `rename` is used when possible, otherwise
  copy → verify size → delete. A file's `._` sidecar moves with it.
- A file that isn't reachable (NAS offline) keeps its new category, is
  not moved, and is reported.
- The catalog path is updated with `Catalog::set_asset_location`, which,
  unlike `relink_asset`, keeps the waveform cache (it's the same file).
- **One Undo reverts both halves.** The run records an
  `organize_file_moves` undo entry that nests the catalog's
  `set_asset_categories` entry. `undo_action` moves each file back (only
  if it's still where the run left it and its old spot is free), removes
  category folders it created once they're empty, and then undoes the
  categories. Redo replays the run.
- Known cost: an edit project that already used a moved file sees it
  offline until relinked. The review modal says so next to the option.

Tracks that already have a category are not re-guessed (that would
override a choice the user made), but the review still lists them under
their current category, so a later run with "move files" puts them in
their folder. The inspector's Category field moves the file too; clearing
a category leaves the file where it is.

- **One category per asset**, enforced in storage on every path:
  `set_asset_categories`, plus `apply_tag_to_assets` and
  `set_tag_approval`, both of which turn a category tag into a move
  rather than stacking it. `suggest_tag_for_asset` won't suggest a
  category to a track that already has one. Category names are resolved
  *before* any asset is touched, because promoting a legacy tag partway
  through a batch once left stale "Impact" suggestions next to real
  categories (seen on a real library). A repair in `migrate()` clears
  those.
- **One undo per batch.** A 300-file Organize records a single
  `set_asset_categories` undo entry (JSON payload with each asset's
  previous category rows).
- **Namespaced key, plain name.** `tags.normalized_name` is globally
  unique, and libraries already have `Ambience` (media type) and
  `Metal`/`Glass` (source) tags. A new category tag is keyed
  `category:<name>` but keeps its plain display name, so neither tag is
  renamed or re-faceted. A same-named tag with no facet or the legacy
  `action` facet is promoted in place, since it already was a category
  in all but name.

### The proposal is reviewed before anything is written

`propose_sfx_organization` is read-only. It runs
`search::sfx_categories::propose_sfx_categories`, and the user then
renames, merges (two groups with the same name merge), unticks, or moves
files before `apply_sfx_organization`. Assets that already have a
category are left out unless the user opts in, so re-running after an
import only sorts the new files and never overrides a choice made by hand.

The engine is pure and deterministic (no ML). Signals, strongest first:
existing user categories, then numbered series kept together, then a
weighted UCS-style keyword vocabulary, then a shared first word. In the
vocabulary, what a sound *is* outweighs generic catch-alls and materials
(`heavy steel door` → Doors, `metro ambiance` → Vehicles). Each member
carries the word that decided it, so the review UI can explain every
placement. On the real Genesis library, all 37 sound effects were placed
in 16 categories, with 0 left uncategorized.

### Junk cleanup: what is deleted from disk is decided per kind

`import_pipeline::junk_file_kind` (AppleDouble `._*`, `.DS_Store`,
`Thumbs.db`, …) is now checked in `prepare_import`, the chokepoint every
import path shares, so junk can't be imported again. Settings › Clean Up
Junk Files (`scan_junk_files`, then a reviewed `clean_up_junk_files`,
which re-classifies every id server-side) treats each kind differently:

| Kind | Library | File on disk |
|---|---|---|
| AppleDouble / system file | removed (tombstoned) | deleted if the user leaves the option on (holds no audio) |
| Empty (0 bytes) | removed | **never deleted**: may be a cloud-sync placeholder whose deletion propagates |
| Unreadable (decode failed) | moved to Trash, restorable | never touched here; it has a real audio name |

"Unreadable" only counts `DecodeFailed`/`InvalidWav` job errors on a
format this build can decode, so an offline NAS file (I/O error after
`defer_unavailable_job`'s retries) or a deliberately unsupported codec
(AAC/M4A, ADR 0028) is never flagged as junk.
