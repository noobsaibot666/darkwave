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

### Categories are virtual: tags, never moved files

Every asset in a typical library is **Referenced**. It lives in the user's
own folder (`/Volumes/DATA/01_ADDS/_SoundFX/`), and NLE projects may link
to it. Moving files into real subfolders would break those links and
reach into storage Darkwave doesn't own. A category is a tag with facet
`category` (`storage::CATEGORY_FACET`), and the real folder structure
appears only where Darkwave writes files: `export_asset_to_project` now
routes into a subfolder named after the asset's category
(`03_SFX/Doors/…`) before falling back to the old "first tag" rule.

- **One category per asset.** `Catalog::set_asset_categories` replaces
  rather than stacks, so the export subfolder is unambiguous. The
  generic Apply Tag grid hides category tags so it can't break this.
  Categories are set via Organize or the inspector's Category field.
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
