# ADR 0022: Wiring natural-language search, multi-select, and NAS reconnect validation

## Status

Accepted.

## Context

A further audit of the workspace (comparing every `pub fn`/`pub struct` in each crate against what `apps/desktop/src-tauri` actually calls) turned up four more pieces of real, tested logic with zero callers anywhere outside their own crate's tests:

- `search::parse_natural_language_query` / `explain_text_query` — the desktop shell's `search_assets` command only ever built `AssetSearchQuery::text(query)` from the raw string; the media-type inference and human-readable filter breakdown this function provides were never used.
- `workspace-state::BrowserState` — a complete, tested arrow-key/click/shift-click/ctrl-click selection state machine with drag-payload support, present only as an unused Cargo dependency. The shell had no multi-select at all: every bulk-shaped command (`apply_tag`, `add_to_collection`) already accepted an asset-id array but the frontend only ever sent a single id.
- `storage::project_source_report` + `export_pipeline::render_license_report_csv` — a project's traceable source/license history could be queried and rendered to CSV, but nothing ever called either function outside tests, so there was no way to actually get a compliance report out of the app.
- `storage::validate_media_availability` / `library_sync::plan_reconnect_validation` + `validate_reconnect_paths` — the "Retry Reconnect" button (ADR 0020) only flipped a client-held `reconnect_requested` flag; nothing re-checked which files were actually back, so `availability_state` never left `Missing` after a real reconnect.

## Decision

**Search.** `search_assets` now runs the query through `search::parse_natural_language_query` first, using the parsed `media_type` (when the query names one, e.g. "sound effects for rain") as a real `AssetSearchQuery` filter and the cleaned term list as the FTS text. A new `explain_search_query` command exposes the same parse for a live "what this search means" chip row under the search bar, so the inference isn't silent.

**Multi-select.** `workspace_state::BrowserState`, `BrowserCommand`, `SelectionMode`, and `DragPayload`/`DragTarget` gained `Serialize`/`Deserialize` and are exposed the same stateless way `library_sync::OfflineControlState` already is (ADR 0020): `create_browser_state` builds one from the currently visible asset ids, `apply_browser_command` takes a state and a command and returns the next state. The frontend rebuilds it whenever the visible asset list changes (re-focusing the previously selected row if it's still present) and drives it from row clicks — plain click replaces the selection, Cmd/Ctrl-click toggles, Shift-click extends a range — plus a fixed Cmd/Ctrl+A for select-all, matching standard file-browser conventions rather than being a user-configurable shortcut. `selectedAssetId` (the focused row) still drives the single-asset inspector panels — tags, source/license, embedded metadata, playback — unchanged; bulk-shaped actions (apply tag, add to project) now send every selected id instead of just the focused one, and three new bulk-only actions (favorite, export, trash) appear in the inspector once more than one row is selected.

**License reports.** `export_project_license_report` maps `storage::ProjectSourceReportRow` onto `export_pipeline::LicenseReportRow` (same fields minus `asset_id`) and writes the rendered CSV to a path the user picks via a save dialog. It's a topbar button, enabled only while a project is the active filter.

**NAS reconnect.** `validate_reconnect` does two things in one call now: it runs `storage::validate_media_availability` against the library's real media root (resolving managed assets' relative paths against `media_root`, checking referenced assets' absolute paths as-is) so `availability_state` reflects reality again, and — when the root is online and reconnect validation is required — builds a `PortableManifest` from the live asset list on the spot (there is still no continuously-maintained manifest file; `backup_library` does the same in-memory construction) and runs `plan_reconnect_validation` + `validate_reconnect_paths` against it, reporting exactly which managed paths are still missing. "Retry Reconnect" calls this alongside the existing `apply_offline_control` state flip and shows both the changed-asset count and the missing-path count.

## Consequences

- Four more crates (or crate features) that were dead weight in the dependency graph now have real callers: `search`'s NL parsing, all of `workspace-state`, `validate_media_availability`/`relink_asset`'s sibling `validate_media_availability` (`relink_asset` itself still has no UI — see below), and `library_sync`'s reconnect-validation pair.
- Still deliberately unwired, and why:
  - `library_sync` writer leases (`acquire_lease_file`/`release_lease_file`/`lease_state`) coordinate concurrent writers across *multiple devices* sharing one NAS library. Darkwave has no device-identity concept anywhere in the app yet (no stable per-install id, no pairing UI), so there's no meaningful way to call these without inventing that concept first — a separate feature, not a wiring gap.
  - `storage::relink_asset` (point a missing managed asset at a new absolute path) has no picker UI yet; reconnect validation now tells you *which* paths are missing, which is the prerequisite for a relink flow, but the flow itself is a follow-up.
  - `export_pipeline::render_wav_export`/`PreviewExportQueue` (24-bit WAV re-encoding, queued preview export) would need a format-choice UI that doesn't exist; the current export path is a real, working original-copy export, not a stand-in for this.
  - `maintenance::plan_preview_cache_eviction`/`evict_preview_cache` need an actual on-disk preview cache to scan, which doesn't exist — waveform peaks are computed in memory per session (ADR 0019/0020), never written to disk. Wiring eviction against zero real cache entries would be a permanent no-op, not real functionality.
  - `audio_analysis::intensity_score` needs real decoded-audio measurements (transient density, low-frequency energy) that nothing in the codebase computes; `library_core::provisional_asset` is an optimistic-UI placeholder builder the app has no use for since imports already register synchronously. Both are pre-built for features that don't exist yet, not gaps in features that do.
