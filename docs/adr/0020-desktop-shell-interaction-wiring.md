# ADR 0020: Desktop shell interaction wiring

## Status

Accepted.

## Context

ADR 0019 wired library creation, folder import, and asset listing/search to the real catalog, but the rest of the interface — the tag grid, favorite star, undo/redo, project drop targets, source/license fields, maintenance summary, settings, shortcuts, accessibility toggles, and the play button itself — was still static mockup content with no backend behind it. The instruction driving this pass was explicit: no cosmetic-only UI elements: every visible control must be backed by a real operation.

## Decision

Extend `storage` with the small set of read paths the shell needed but that didn't exist yet: `list_collections`, `get_source_record`, and `pending_job_count`. Derive `Serialize`/`Deserialize` on the record types that now cross the Tauri IPC boundary (`TagRecord`, `CollectionRecord`, `UsageEventRecord`, `SourceRecordDraft`, `TagOrigin`, `TagApprovalState`, `CollectionType`, `UsageEventType`).

Add real Tauri commands backed directly by existing, already-tested crate logic:

- **Playback**: `asset_playback_path` resolves an asset's absolute file path (joining a managed asset's relative path with its library's media root, or returning a referenced path as-is). The frontend plays it through the platform webview's native `<audio>` element via Tauri's asset protocol, and computes waveform peaks client-side with the Web Audio API — see ADR notes in the playback-engine doc for why this bypasses `audio-engine` for now.
- **Tags**: `list_tags`, `create_tag`, `tags_for_asset`, `suggested_tags_for_asset`, `apply_tag`, `accept_suggested_tag`, `reject_suggested_tag`.
- **Favorites and review**: `set_favorite`, `set_reviewed`.
- **Undo/redo**: `undo_action`, `redo_action`, driven by a client-side stack of the undo IDs that mutating commands (`apply_tag`, `add_to_collection`) already return.
- **Collections and projects**: `list_collections`, `create_project`, `add_to_collection`, `assets_in_collection`. The sidebar's smart filters (Favorites, Unreviewed, Missing Files, Music, Sound Effects, Ambience) are computed client-side over the already-fetched asset list rather than new SQL, since every field they filter on is already present on `AssetRecord`.
- **Source and license**: `get_source_record`, `set_source_record`.
- **Export**: `export_selected_asset` plans and executes an original-copy export through `export-pipeline` and records a usage event.
- **Maintenance**: `maintenance_report` builds a real `MaintenanceReport` from live catalog state — missing media from availability state, license review needed from the absence of a source record, exact duplicates from `fingerprint::exact_duplicate_key` grouping by content hash and file size, and a stale-waveform-cache count from pending `WaveformGeneration` jobs. It does not use likely-duplicate acoustic fingerprint matching, since no real fingerprint extraction from audio exists yet — only exact duplicates are reported.
- **Media root status**: `media_root_status` probes the active library's actual media root instead of a hardcoded sample path.
- **Settings**: `load_app_preferences`/`save_app_preferences` persist to `preferences.json` in the app data directory using the `preferences` crate's existing (previously unused) `load_preferences`/`save_preferences` functions. `AppPreferences` gained `reduced_motion` and `reduced_transparency` fields (both `#[serde(default)]` for forward compatibility with files saved before this change) since accessibility toggles had nowhere to persist.
- **Shortcuts**: the frontend listens for `keydown` globally and dispatches against the real `shortcuts.bindings` list from preferences, so the shortcut list in the inspector reflects what actually fires.

The command palette's five buttons now call real handlers (import, focus search, scroll to the tag/settings sections, export selected) instead of being inert.

A follow-up within the same pass added Trash (`move_asset_to_trash`, `list_trash_items`, `restore_asset_from_trash`, `purge_trash_item` in `storage`, backed by a new `trash_items` table; `list_assets`/`search_assets` now exclude trashed assets) and NAS offline controls (`media_root_status` against the real library path, `apply_offline_control` as a stateless wrapper around `library_sync::OfflineControlState::apply`).

A second follow-up added backup creation: `backup_library` builds a `PortableManifest` from the live asset list (there was no separately-maintained manifest file to source it from), writes it beside the catalog, and copies both into a user-chosen folder through `backup::create_backup`. Restore was deliberately left unwired: applying a `RestorePlan` means overwriting `catalog.sqlite` while the running app still holds it open behind `CatalogState`'s `Mutex`, which is a real data-integrity risk (a corrupted or torn write to a database the app is actively querying) that needs a proper "close catalog, restore, relaunch" flow, not a same-session file copy. The button exists and is disabled with that reasoning shown, rather than either faking it or silently omitting it.

## Consequences

- Every interactive element in the desktop shell either does something real or is explicitly disabled with a tooltip saying it isn't wired yet (restore, duplicate-group actions beyond listing, and native drag-and-drop remain in that state after this pass).
- Per-row waveform bars in the asset browser were replaced with a neutral icon rather than fake random data; only the transport bar, for whatever is currently loaded, renders real computed peaks. Decoding every visible row's audio for a cosmetic mini-waveform was judged not worth the performance cost for an MVP with no background waveform cache yet.
- Tag removal (as opposed to applying a tag or undoing the last apply) has no dedicated command yet — applied tags are shown but not individually removable outside of undo.
- The maintenance report's duplicate detection is exact-hash-only; likely-duplicate detection needs real audio fingerprint extraction, which doesn't exist anywhere in the codebase yet.
