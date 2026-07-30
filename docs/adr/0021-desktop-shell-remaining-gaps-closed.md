# ADR 0021: Closing the desktop shell's remaining wiring gaps

## Status

Accepted.

## Context

ADR 0020 wired the interaction surface but left several items explicitly disabled or absent: tag removal, backup restore, duplicate-group actions beyond listing, and the background job queue (jobs were enqueued at import but nothing ever consumed or completed them, so `pending_job_count` only ever grew). The instruction driving this pass was the same as ADR 0020's: no cosmetic-only controls, and every disabled affordance needs either a real implementation or an honest reason it stays disabled.

## Decision

**Tag removal.** `storage::remove_tag_from_asset` deletes the `asset_tags` row and records a `readd_asset_tags` undo entry (the inverse of the existing `remove_asset_tags` undo entry that `apply_tag_to_assets` records), so removing a tag is undoable/redoable the same way applying one is. Exposed as the `remove_tag` command and a ✗ button on each applied-tag chip.

**Background jobs.** Rather than build the full persistent/prioritized/throttled queue described in the plan doc (§16.3) — which needs retry policy, cancellation, and CPU/battery-aware scheduling to be worth calling a real job system — this pass closes the gap that existed between the job *records* and what already happens in the app:

- `enqueue_job` for `JobKind::Hashing` was removed from `import_file_internal`. The content hash is already a full-file SHA-256 computed synchronously during import; the job existed only in name; nothing was ever going to consume it. Enqueueing it made `pending_job_count` accrue entries that could never mean anything.
- `JobKind::MetadataExtraction` now does real work: `storage` gained `pending_jobs_of_kind`, `complete_job`, and `fail_job`, and the desktop shell's `process_pending_jobs` command drains pending metadata jobs by calling `audio_metadata::extract_embedded_metadata` (WAV `LIST/INFO` chunk parsing — title, genre, comment — already implemented and tested, never called from anywhere) against each asset's resolved playback path, persisting the result via a new `set_embedded_metadata` method and three new nullable columns on `assets` (`embedded_title`, `embedded_genre`, `embedded_comment`). It runs automatically after each import. Non-WAV files complete immediately with empty metadata rather than erroring, matching what `extract_embedded_metadata` already does. The inspector shows an "Embedded Metadata" panel when any field is present.
- `JobKind::WaveformGeneration` now gets marked complete by `storage::complete_pending_jobs_for_asset` when the frontend actually finishes computing real peaks for an asset (the same Web Audio API path from ADR 0020), via a new `mark_waveform_ready` command. This was the actual bug the "stale waveform cache" maintenance count was hiding: the work was happening client-side the whole time, the job record just never learned about it.

This intentionally stops short of a generic background worker loop, priority scheduling, or retry/backoff — those are real, separately-scoped pieces of the plan's job system, not gaps in wiring the existing crates already solved.

**Duplicate-group actions.** `fingerprint::DuplicateReviewAction` defines five actions (`KeepBoth`, `LinkAsVariants`, `MergeMetadata`, `ReplaceLowerQuality`, `MoveDuplicateToTrash`), but only exact-hash duplicate detection exists — there's no acoustic fingerprinting, no variant-linking model in `storage`, and no quality-comparison logic anywhere, so `LinkAsVariants`, `MergeMetadata`, and `ReplaceLowerQuality` have nothing real to call. `MoveDuplicateToTrash` does: the new `trash_duplicate_group` command keeps the first (oldest, by `date_added`) asset in a duplicate group and moves the rest to trash through the existing `move_asset_to_trash`. `KeepBoth` needs no action by definition. The maintenance panel's duplicate-group rows now have a "Keep oldest, trash rest" button; the other three actions stay unbuilt rather than faked.

**Backup restore.** The data-safety concern from ADR 0020 — overwriting `catalog.sqlite` while the app holds it open — is resolved without a close/relaunch flow: `CatalogState` is still `Mutex<Catalog>`, and `restore_library`
1. copies the backup's `catalog.sqlite` to a staging file (`catalog.sqlite.restoring`) next to the live one, entirely off the hot path;
2. takes the mutex and swaps the live `Catalog` for a `Catalog::open(":memory:")` placeholder, which drops the real `Connection` and releases its file handle;
3. takes a best-effort safety copy of the pre-restore catalog (`catalog.sqlite.before-restore`);
4. `rename`s the staged file over the live path — a same-filesystem rename is atomic, so a crash or failure before this point never touches the live file;
5. copies the manifest and reopens `Catalog::open` on the now-restored file, putting the result back into the mutex.

Every branch of the match on (copy result, reopen result) leaves the mutex holding *some* valid `Catalog` — worst case an in-memory empty one rather than a poisoned or missing state — so a failed restore can't leave later commands panicking on a missing catalog. The frontend re-fetches `list_libraries` and resets the active library after a successful restore. This isn't a full crash-safe transactional restore (a concurrent OS-level write to the same file during the staging copy isn't guarded against, and the mutex only serializes access from *this* app instance), but it removes the actual risk that was blocking the button: overwriting a database the running process still had a connection open against.

## Consequences

- The desktop shell's Backup section now has a working Restore path; ADR 0020's disabled-button note about it is superseded by this ADR.
- `background_jobs` counts now reflect real, current state instead of monotonically growing; the maintenance report's stale-waveform-cache number will actually reach zero once every visible asset has been played at least once.
- `LinkAsVariants`, `MergeMetadata`, and `ReplaceLowerQuality` remain unbuilt — they need acoustic fingerprinting and a variant/quality model that don't exist yet, not further wiring of what's already there.
- Native OS drag-and-drop remains out of scope (click-to-apply covers the same functional surface); a full priority/retry/throttled background job system remains out of scope (this pass closes the specific record-vs-reality mismatch, not the plan's entire §16.3).
