# ADR 0023: Desktop shell usability pass and smarter import

## Status

Accepted.

## Context

A round of direct user feedback on the running app identified concrete usability problems (oversized type, misplaced controls, buttons clipping their own labels, dead buttons) and real functional gaps in import behavior (no subfolder recursion, no size-based triage, no way to pick up files dropped into the media root outside the app). This ADR covers that whole pass together since most of the changes are small and interrelated.

## Decision

**Typography and layout.** Base font size dropped from the browser default (16px) to 13px, with headings and secondary text scaled down proportionally. Every button class gained `white-space: nowrap` plus, where a button's content can exceed its box (tag chips, drop-target buttons, nav items), `overflow: hidden; text-overflow: ellipsis` instead of wrapping to a second line.

**Undo/Redo moved off the toolbar.** They're gone from the topbar entirely. A native Tauri menu (`Edit` submenu, built in `run()`'s `.setup()`) now carries Undo (`CmdOrCtrl+Z`) and Redo (`CmdOrCtrl+Shift+Z`) as real OS-level menu items with accelerators, alongside standard Cut/Copy/Paste/Select All so text fields keep normal behavior. Menu clicks emit `menu-undo`/`menu-redo` events (`on_menu_event` → `app.emit`), which the frontend listens for via `@tauri-apps/api/event` and routes to the existing `handleUndo`/`handleRedo` logic — no duplicate implementation. New Project moved from the sidebar into the topbar in the space Undo/Redo vacated.

**One Import Folder control.** The topbar's primary Import button is now the only import trigger; the duplicate buttons in the onboarding strip and command strip are gone. The media root path is no longer shown in the main UI — it moved into the new Settings modal (see below), alongside library name and online/offline status.

**Collapsible inspector.** Every inspector section is now a `CollapsibleSection` (a small local component: header with title + fold/unfold chevron button, collapsible body) instead of a flat stack of `<section>`s. Shortcuts and Release Readiness default collapsed since they're reference material, not day-to-day controls. Settings and Accessibility were pulled out of the inspector entirely and now live in a modal opened from a gear icon in the topbar (the old gear icon scrolled to a section; it now actually opens something). Browser density and preview cache limit are editable there and actually take effect: `browser_density` now drives real row-height CSS (`.browser[data-density="Compact|Expanded"]`), closing a gap where that preference was previously stored but never applied to anything.

**Panel collapse and search filter.** New sidebar/inspector hide toggles (`PanelLeftClose`/`PanelRightClose` icons) collapse either panel via a CSS class swap on `.shell` (`grid-template-columns` adjusts, the collapsed `<aside>` gets `display: none`). The search bar's Filter button, previously hardcoded `disabled`, now opens a dropdown with the same smart-filter list as the sidebar — real function, and useful specifically because the sidebar can now be hidden.

**Arrow-key selection bug.** `ArrowUp`/`ArrowDown` were already bound to `PreviousAsset`/`NextAsset` and did change `selectedAssetId` and start playback, but the row highlight is driven by `BrowserState.selected_indices`, which those handlers never updated — so arrow navigation moved playback without visibly selecting a row. `playRelative` now also dispatches `FocusRow`/`SelectFocused` against the current `BrowserState`, the same sequence a row click uses.

**Click-to-seek.** The transport waveform is now a real scrubber: it's `role="slider"` with a click handler that maps the click's x-position to a fraction of `duration` and sets `audio.currentTime` directly. Previously it only displayed peaks with no interaction.

**Sidebar taxonomy.** "Music" is relabeled "Soundtracks" in the UI (the underlying `media_type` value is still `"music"` — this is a display rename, not a schema change). Sound Effects gained an expandable sub-list of action-facet tags (Impact, Whoosh, Rise, …) from the starter taxonomy; selecting one filters by that tag through a new `assets_for_tag` command (a thin wrapper around `storage::search_assets` with `AssetSearchQuery::with_tag`, since the existing search command only ever exposed free-text queries over IPC, not a direct tag filter).

**Import intelligence.** Three changes to `import-pipeline`:
- Import now recurses into subfolders (`collect_audio_files` in the desktop shell walks the directory tree, skipping dotfiles/dot-directories) instead of only reading the top level of the chosen folder — real libraries are nested vendor/pack trees, not flat.
- The extension whitelist gating import was `audio_metadata::supported_mvp_format` (the same list that gates *playback* decode support). It's now `is_recognized_audio_extension`, a deliberately broader list (adds wma, caf, wv, ape, amr, oga, opus on top of the MVP set) — a file can be cataloged, tagged, and organized before this app can natively play it, matching the plan's existing distinction between import-eligible and decode-supported.
- `refine_media_type_by_size` runs after the existing filename/embedded-metadata classification: files ≤8KB are routed to a new `needs_review` category regardless of what the filename suggested (that size is far too small to be real audio — it's the same NAS-stub/placeholder pattern observed during earlier live testing — see ADR 0022's testing notes), and files ≤5MB that filename/metadata analysis left as `"other"` default to `sound_effect`, since a real music track is essentially never that small. `needs_review` is a new sidebar smart filter, not a silent drop — the files are still imported, just flagged.

**Refresh and auto-scan.** `refresh_library` re-walks a library's media root with the same recursive collector and imports anything not already cataloged; since `register_asset` already dedupes on content hash + size, re-running it is naturally idempotent — already-known files come back unchanged rather than duplicating. A topbar Refresh button triggers it on demand, and a `useEffect` on `activeLibraryId` triggers it silently once whenever a library loads or is switched to, covering "I dropped a file into the NAS folder outside the app."

## Consequences

- The Undo/Redo keyboard shortcuts are now provided by the OS menu system rather than a JS `keydown` handler, which is more correct (works even when the accelerator is checked against a native menu, consistent with platform conventions) but means they're untestable via the existing pure-frontend test surface (there isn't one yet — this app has no frontend test suite).
- `needs_review` and the size thresholds (8KB / 5MB) are heuristics, not certainty — a legitimately tiny sound effect a few KB in size will still land in `sound_effect` territory fine (the 8KB floor is far below any real audio), but the thresholds are not configurable yet if a library's norms differ.
- Link/merge/replace duplicate actions, real audio-engine playback, and the full background-job priority/retry system remain out of scope, as documented in ADRs 0020–0022.
