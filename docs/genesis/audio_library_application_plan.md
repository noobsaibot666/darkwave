# Audio Library Application — Product & Development Plan

## Implementation Status (updated 2026-07-30)

This plan is the original product/development spec. It is not rewritten as implementation progresses — instead, Section 19 (MVP Scope) and Section 20 (Delivery Milestones) below are annotated in place so this document stays the single source of truth for both intent and status. Everything else in the document (product definition, principles, taxonomy, design system, data model, etc.) remains the target spec, not a record of what exists yet.

Legend: **[Done]** built and wired end-to-end · **[Partial]** real, tested logic exists but isn't fully wired or the deliverable is incomplete · **[Not started]** no implementation yet.

Product name: the app is currently branded **Darkwave** in the codebase, not the placeholder "Resonant" from Section 1.

High-level state: the desktop shell (Tauri + Rust + React) has a working end-to-end vertical slice — create a library, import a folder, browse/search/play/tag/organize/export, back up and restore, work offline and reconnect a NAS-backed media root. Every interactive control either does something real or is explicitly disabled with a reason; there are no cosmetic-only UI elements. What's missing is concentrated in three areas the plan explicitly treats as later-stage: the smart-analysis pipeline beyond filename/embedded-metadata rules (no audio measurements, classification, or fingerprinting/embeddings), the full background job system (persistent queue exists, but no priority/pause/retry/throttling), and release readiness (signing, notarization, auto-update, licensing review are all still "Planned", not built). Detailed status is in Sections 19 and 20. Reasoning for every deliberate deferral is captured in `docs/adr/0019` through `docs/adr/0022`.

One caveat worth surfacing explicitly: the in-app "Release Readiness" panel (Section 20, Milestone 0/7) sources macOS audit, Windows audit, accessibility audit, performance profile, crash recovery, and onboarding docs from `ReleaseReadinessConfig::code_gates_passed()` — a hardcoded default that marks all of them `Passed`. None of those audits have actually been performed; the name means "the code exists and builds," not "the audit ran." Only codec packaging, codec license review, update system, and signing/notarization are driven by real optional config and correctly show `Planned`. Treat the "Passed" gates as unverified, not as evidence.

## 1. Product Definition

### Working title
**Resonant** — a fast, intelligent audio asset library for video editors and sound-driven creatives.

The name is temporary. Do not hard-code it deeply into the application architecture.

### Product statement
A desktop application that captures, analyzes, catalogs, previews, searches, organizes, and exports music, sound effects, ambience, dialogue fragments, loops, and other audio assets.

The application reduces repeated downloading and searching by turning every imported sound into a reusable, searchable personal library.

### Core promise
> Drop audio in once. Find it instantly forever.

### Primary user
A video editor who:

- Downloads large numbers of music tracks and short sound effects.
- Works with immersive, layered sound design.
- Receives files from many websites and in inconsistent formats.
- Does not want to manually rename and tag every asset.
- Needs extremely fast keyboard-led auditioning.
- Stores media on local disks or a NAS such as TrueNAS.
- Needs to drag assets directly into editing software.

### Product category
This is not simply “Eagle for sound.” It should be positioned as:

**A listening-first audio asset workspace with automatic organization.**

The distinction matters. Image asset managers are visually browsed. Audio assets must be understood through rapid playback, waveform shape, duration, sonic similarity, metadata, and contextual tags.

---

## 2. The Problem Being Solved

The current workflow creates a permanent loop:

1. Search online for music or sound effects.
2. Download many candidate files.
3. Use a small percentage in a project.
4. Leave unused files in Downloads.
5. Lose context, source, license, and organization.
6. Delete them later.
7. Search for and download the same or similar sounds again.

This wastes time in four places:

- Repeated discovery.
- Repeated downloading.
- Manual auditioning.
- Manual organization.

The application must solve all four. A product that only stores and tags files solves only the last one.

---

## 3. Product Challenge and Corrections

## 3.1 Do not build a generic folder manager

Folders alone reproduce the existing problem. A sound can simultaneously be:

- Impact
- Metallic
- Cinematic
- Short
- Dark
- Trailer
- High intensity

A rigid folder hierarchy forces the user to choose one location. The product should instead use:

- Collections for broad intentional grouping.
- Tags for multiple classifications.
- Smart collections for rule-based grouping.
- Projects for temporary editorial use.
- Similarity relationships for sonic discovery.

Folders may be displayed as a familiar organizational layer, but they must not be the underlying mental model.

## 3.2 Do not depend on filenames as the intelligence layer

Filenames are useful signals but often incomplete, promotional, duplicated, or meaningless. Automatic classification should combine:

- Filename tokens.
- Embedded metadata.
- Parent folder names.
- Duration.
- Loudness.
- Tempo.
- Spectral and transient characteristics.
- Audio embedding or classifier output.
- Source URL or source domain when available.
- User corrections over time.

Filename analysis should be the first signal, not the final answer.

## 3.3 Do not put the live database on TrueNAS

The media library can live on TrueNAS, but the active application database should remain local on each computer.

Recommended architecture:

- **TrueNAS:** original audio files, optional managed library structure, backups, portable library manifest exports.
- **Local device:** SQLite catalog, search index, thumbnails/waveform peaks, playback cache, preferences, current session state.
- **Synchronization layer:** reconcile local catalog state with a portable library manifest stored beside the media.

Reasons:

- Network shares add latency.
- File locking varies across SMB implementations and operating systems.
- SQLite WAL mode should not be used as a multi-host database over a network filesystem.
- Playback must remain responsive during temporary NAS interruptions.

The application must never assume the NAS is always online.

## 3.4 “Native macOS” and “Windows” require a design-system compromise

A single SwiftUI application cannot serve Windows. The correct goal is:

> Apple-quality interaction and visual refinement, implemented in a cross-platform desktop architecture with platform-aware behavior.

Recommended stack:

- Tauri 2 desktop shell.
- Rust core for indexing, file operations, analysis, audio processing, and database access.
- React + TypeScript interface.
- CSS design tokens and platform-specific material treatments.
- Native menu bar, keyboard shortcuts, file dialogs, drag-and-drop, notifications, and window behavior through Tauri.

The macOS build should use Apple-inspired translucent materials and spacing. The Windows build should preserve the same product identity while adapting window controls, context menus, typography, and accessibility behavior to Windows conventions.

Do not copy Apple proprietary visuals exactly. Build an original design system inspired by clarity, depth, restraint, and responsive materials.

## 3.5 AI tagging must be reviewable, reversible, and local-first

Fully automatic tagging will make errors. The product should expose confidence and make correction effortless.

Each machine-generated tag should have:

- Confidence score.
- Source: filename, metadata, acoustic model, user rule, or user correction.
- Approval state: suggested, accepted, rejected.

The user should not be forced into a long import wizard. Assets should appear immediately, then improve progressively as background analysis completes.

## 3.6 Do not make “download from websites” the first release

Direct integrations with commercial music and SFX services create:

- Authentication complexity.
- API dependence.
- Terms-of-service concerns.
- Licensing confusion.
- Site-specific breakage.

The MVP should capture files after download through:

- Watched folders.
- Drag and drop.
- Share extension or system action where practical.
- Clipboard/source URL capture.
- Optional browser extension in a later phase.

The product can later integrate approved provider APIs, but it should not begin as a downloader or scraper.

---

## 4. Product Principles

1. **Listening comes before organizing.**
2. **Import must feel instant.**
3. **The application organizes first; the user corrects exceptions.**
4. **Keyboard use must be as complete as mouse use.**
5. **Every sound remains traceable to source and license.**
6. **NAS disconnection must never break the catalog.**
7. **No destructive file operation without a recoverable path.**
8. **One main workspace, not a maze of pages.**
9. **The interface must remain calm with 100,000+ assets.**
10. **Audio playback should respond faster than the user can think.**

---

## 5. Core Product Structure

The application uses one primary workspace with adaptive panels.

### Main window regions

#### A. Left sidebar — Library context

Contains:

- All Sounds
- Inbox
- Recently Added
- Recently Played
- Favorites
- Unreviewed
- Missing Files
- Duplicates
- Music
- Sound Effects
- Ambience
- Voice / Dialogue
- User Collections
- Smart Collections
- Projects
- Sources
- Trash

The sidebar is collapsible and resizable.

#### B. Central browser — Listening feed

A dense, row-based browser inspired by professional music libraries rather than image thumbnails.

Each sound row includes:

- Play state.
- Compact waveform.
- Filename or cleaned display name.
- Primary category.
- Duration.
- BPM where relevant.
- Key where relevant.
- Intensity indicator.
- Favorite control.
- Availability state: local, NAS, cached, missing.
- License/source badge.
- AI-review indicator when needed.

Rows must support compact, comfortable, and expanded density.

#### C. Right inspector — Classification and details

The inspector contains contextual sections:

- Suggested tags.
- Accepted tags.
- Category.
- Mood.
- Material/source.
- Action.
- Intensity.
- Texture.
- Tempo/key for music.
- Source and license.
- File metadata.
- Notes.
- Related/similar sounds.

Tags are presented as large drop targets. The user can drag selected sounds onto a tag, collection, project, or classification target.

#### D. Bottom transport — Persistent audition controls

Contains:

- Large waveform overview.
- Play/pause.
- Previous/next.
- Scrub position.
- In/out preview region.
- Loop.
- Playback speed.
- Volume.
- Mono/stereo control.
- Output device.
- Auto-play next toggle.
- “Reveal,” “Copy,” and “Send to editor” actions.

The transport remains visible while navigating the library.

#### E. Top command area

Contains:

- Global search.
- Filter button.
- Sort.
- View density.
- Import.
- Command palette.
- Background task status.

---

## 6. Primary Workflows

## 6.1 First launch

1. User chooses **Create Library** or **Open Library**.
2. User chooses media storage location:
   - Local disk.
   - External disk.
   - NAS folder.
3. Application explains that local catalog/cache files remain on the computer for speed.
4. User optionally selects watched folders such as Downloads.
5. Application creates the library structure and opens the main workspace.

Do not force account creation in the first release.

## 6.2 Import workflow

Supported entry points:

- Drag files or folders into the application.
- File picker.
- Watched folder.
- “Import from Downloads.”
- Paste a file path.
- System share/open-with action where supported.

Import behavior:

1. Register files immediately.
2. Display rows with provisional metadata.
3. Begin waveform generation and analysis in the background.
4. Detect exact duplicates through content hash.
5. Detect likely duplicates through audio fingerprint.
6. Extract embedded metadata.
7. Parse filenames and folder names.
8. Generate category and tag suggestions.
9. Add new files to Inbox or Unreviewed.
10. Preserve source URL and license information whenever available.

The interface must stay usable while analysis runs.

## 6.3 Rapid audition workflow

Default shortcuts:

- `Space`: play/pause selected sound.
- `Up/Down`: previous/next sound.
- `Left/Right`: seek backward/forward.
- `Shift + Left/Right`: larger seek interval.
- `Enter`: open inspector focus.
- `F`: favorite.
- `L`: toggle loop.
- `I`: mark in point.
- `O`: mark out point.
- `1–9`: apply user-configurable quick tags.
- `Cmd/Ctrl + K`: command palette.
- `Cmd/Ctrl + F`: search.
- `Cmd/Ctrl + C`: copy selected asset.
- `Cmd/Ctrl + Shift + C`: copy file path.
- `Cmd/Ctrl + E`: reveal/export/send action.

Selection change should optionally auto-play after a configurable delay of approximately 80–150 ms.

## 6.4 Organization workflow

The user can:

- Select one or many sounds.
- Drag them onto a tag, collection, project, category, or quick target.
- Accept or reject AI suggestions with one click.
- Apply a quick tag by number key.
- Create a new tag by typing once.
- Merge duplicate or synonymous tags.
- Undo every organizational operation.

The application should learn from accepted corrections within the current library.

Example:

- AI repeatedly suggests “hit.”
- User changes similar assets to “impact.”
- Application asks whether “hit” should map to “impact” in future imports.

## 6.5 Editing workflow

The user should be able to:

- Drag an original file directly into DaVinci Resolve, Premiere Pro, Final Cut Pro, After Effects, Finder, Explorer, or another application.
- Copy the file to a chosen project media folder.
- Create an editorial copy while preserving the library original.
- Convert the editorial copy to a chosen standard such as WAV 48 kHz/24-bit.
- Export only a selected in/out range.
- Reveal the original file.
- Copy its path.

The first release should prioritize universal drag-and-drop and copy/export. Direct NLE integrations can follow later.

## 6.6 End-of-project workflow

Projects in this application are lightweight bins, not editing timelines.

At the end of a project, the user can:

- View every auditioned sound.
- View every exported or dragged sound.
- Mark used sounds.
- Preserve unused shortlist assets.
- Archive the project bin without deleting library items.
- Export a cue/source/license report.

This directly addresses the user’s current loss of project context.

---

## 7. Information Architecture and Taxonomy

The product should ship with a controlled starter taxonomy, not a blank tagging system.

## 7.1 Top-level media types

- Music
- Sound Effect
- Ambience
- Foley
- Voice / Dialogue
- Loop
- Stinger / Ident
- Transition
- Production Audio
- Other

## 7.2 Sound-effect facets

### Action

- Impact
- Hit
- Whoosh
- Rise
- Fall
- Sweep
- Movement
- Drop
- Break
- Crush
- Slam
- Click
- Beep
- Alarm
- Transition

### Source or object

- Metal
- Glass
- Wood
- Paper
- Fabric
- Plastic
- Stone
- Water
- Fire
- Electricity
- Vehicle
- Machine
- Human
- Animal
- Nature
- Interface

### Character

- Clean
- Dirty
- Organic
- Synthetic
- Cinematic
- Realistic
- Abstract
- Vintage
- Futuristic
- Distorted
- Glitchy
- Textural

### Energy

- Subtle
- Low
- Medium
- High
- Extreme

### Frequency impression

- Sub
- Bass-heavy
- Mid-focused
- Bright
- Full-range

## 7.3 Music facets

- Genre
- Mood
- Energy
- Tempo
- BPM
- Key
- Instrumentation
- Vocal / Instrumental
- Structure
- Era
- Use case

## 7.4 Tag governance

The system must support:

- Preferred term.
- Synonyms.
- Hidden aliases.
- Parent/child relationship.
- User-created custom terms.
- Merge tags.
- Rename tags without touching filenames.
- Blocked tags that AI should no longer suggest.

---

## 8. Smart Analysis Pipeline

Analysis should be progressive and modular.

## 8.1 Stage 1 — Immediate metadata

Target completion: nearly instant.

- File path.
- Extension.
- File size.
- Creation/modification dates.
- Duration.
- Sample rate.
- Bit depth.
- Channel count.
- Embedded title, artist, album, genre, comments.

## 8.2 Stage 2 — Filename intelligence

- Tokenize filename.
- Remove pack numbers, vendor prefixes, duplicate counters, and irrelevant punctuation.
- Detect known vocabulary.
- Detect BPM and key notation.
- Detect source pack and vendor.
- Create a cleaned display name without renaming the original file.

## 8.3 Stage 3 — Audio measurements

- Integrated loudness.
- Peak level.
- Dynamic range estimate.
- Tempo/BPM confidence.
- Musical key confidence.
- Transient density.
- Spectral centroid/brightness.
- Low-frequency energy.
- Stereo width.
- Silence at head and tail.
- Loop likelihood.

## 8.4 Stage 4 — Audio classification

- Broad media type.
- SFX action category.
- Source/material classification.
- Mood and intensity.
- Instrumentation and vocal presence for music.

All results remain suggestions until confidence crosses a configurable threshold or the user accepts them.

## 8.5 Stage 5 — Similarity and fingerprinting

Generate:

- Exact content hash.
- Perceptual audio fingerprint.
- Semantic embedding.

Use cases:

- Exact duplicates.
- Alternate formats of the same sound.
- Shortened or normalized versions.
- “Find sounds like this.”
- Cluster similar assets in the Inbox.

## 8.6 AI architecture

Use a provider abstraction:

- Local inference where practical.
- Optional cloud enrichment later.
- No mandatory cloud account for core organization.
- Clear consent before uploading any audio.
- User-visible model and privacy settings.

Initial MVP can combine filename rules, metadata, audio measurements, and a modest local classifier. Do not make the MVP dependent on a large generative model.

---

## 9. Library and Storage Architecture

## 9.1 Library modes

### Managed library

The application copies files into a controlled media structure.

Recommended structure:

```text
Resonant Library/
├── Media/
│   ├── 00/
│   ├── 01/
│   └── ...
├── Manifests/
├── Exports/
└── Backups/
```

Files should use stable asset IDs internally. User-facing display names remain independent.

### Referenced library

The application catalogs files in their existing locations without moving them.

Use case:

- Existing sound libraries.
- Commercial sample packs.
- Read-only archives.

### Hybrid library

Some files are managed, others referenced.

This should become the recommended mode.

## 9.2 Local application data

Stored per machine:

```text
App Data/
├── catalog.sqlite
├── search-index/
├── waveform-cache/
├── preview-cache/
├── model-cache/
├── thumbnails/
├── logs/
└── preferences.json
```

## 9.3 Portable library manifest

Stored in the shared library:

- Stable library UUID.
- Asset IDs.
- Relative media paths.
- User metadata.
- Collections.
- Tags.
- Source/license records.
- Change log or versioned snapshots.

Do not use the portable manifest as the live query database. It is the synchronization and recovery format.

Suggested format:

- Versioned SQLite snapshot in rollback-journal-safe export mode, or
- Structured JSON/JSONL records with atomic manifest replacement.

For the first release, support one active writer at a time for a shared library. Detect another writer through a lease file and show a clear read-only mode.

## 9.4 Offline and NAS behavior

When NAS is unavailable:

- Catalog remains searchable.
- Cached waveforms remain visible.
- Cached low-bitrate previews may play when available.
- Original-dependent actions are disabled with a clear status.
- User can queue exports for later.
- Reconnection triggers validation, not a complete rescan.

## 9.5 File integrity

Every asset receives:

- Stable UUID.
- Content hash.
- Original filename.
- Current path.
- File size.
- Last-seen timestamp.
- Availability state.

The application must distinguish:

- Moved.
- Renamed.
- Missing.
- Modified.
- Duplicate.
- Replaced.

---

## 10. Playback and Waveform Requirements

## 10.1 Playback goals

- Selected sound begins audibly in under 100 ms when local or cached.
- NAS audio should begin as fast as the network permits, with optional local preview caching.
- Moving to the next sound must cancel the previous decode immediately.
- Playback should not block UI rendering or indexing.
- Gapless repeated auditioning where format support permits.

## 10.2 Recommended audio engine

Rust audio service using:

- Rodio/CPAL for playback.
- Symphonia decoding for common formats.
- A carefully reviewed FFmpeg sidecar or licensed build for formats outside the native decoder set and for conversion.

Important: FFmpeg distribution and enabled codec configuration must be reviewed for licensing before commercial release.

## 10.3 Supported formats for MVP

Required:

- WAV
- AIFF
- MP3
- FLAC
- AAC/M4A
- OGG Vorbis

Optional after validation:

- Opus
- CAF
- WMA
- ALAC
- Broadcast WAV metadata

Unsupported files should remain visible with a diagnostic and conversion option.

## 10.4 Waveform system

Waveforms must be generated once and cached as multi-resolution peak data.

Do not decode the full file every time a row is displayed.

Store multiple levels:

- Tiny row waveform.
- Medium inspector waveform.
- Full transport waveform.

Waveform animation:

- Smooth playhead at display refresh rate.
- Subtle amplitude-reactive glow or material response.
- Reduced-motion alternative.
- No decorative animation that delays selection or playback.

Recommended rendering:

- Canvas or WebGL for dense lists.
- A custom waveform component fed by precomputed peaks.
- Avoid rendering thousands of individual SVG points in long lists.

## 10.5 Preview cache

Optional local preview cache for NAS assets:

- Low- or medium-bitrate compressed previews.
- Configurable cache size.
- LRU cleanup.
- Never replaces originals.
- User can pin assets or collections for offline auditioning.

---

## 11. Search and Discovery

## 11.1 Search types

### Text search

Search:

- Display name.
- Original filename.
- Tags.
- Collections.
- Notes.
- Source/vendor.
- License.
- Embedded metadata.

### Natural language search

Examples:

- “short dark metallic impacts”
- “slow emotional piano without vocals”
- “subtle room tone under 30 seconds”
- “bright digital transition with no bass”

Natural language search should translate into visible filters so the result is explainable.

### Similarity search

- Find sounds similar to selected.
- Exclude exact pack variants.
- Adjust similarity emphasis between timbre, rhythm, mood, and semantics in a later release.

## 11.2 Filter system

Filters include:

- Type.
- Category.
- Tags.
- Duration range.
- BPM range.
- Key.
- Energy.
- Loudness.
- Source.
- License status.
- Date added.
- Last used.
- Times used.
- Favorites.
- Offline availability.
- Reviewed/unreviewed.

Filters appear as removable chips and can be saved as Smart Collections.

## 11.3 Search performance

- Local full-text search index.
- Indexed numeric fields.
- Paginated or virtualized results.
- Query cancellation while typing.
- Target visible update under 50 ms for common local queries.

---

## 12. Source and License Tracking

This must be a first-class feature, not an afterthought.

Each asset can store:

- Provider/source name.
- Source URL.
- Download date.
- Account or subscription context.
- License type.
- License document or receipt attachment.
- Project restrictions.
- Attribution requirement.
- Expiration or subscription status note.
- Custom notes.

The application should warn, not block, when exporting an asset with missing or uncertain license information.

A project report can include:

- Asset title.
- Original filename.
- Source.
- License.
- Usage status.
- Export date.

The product must never imply that storing a file grants permission to use it.

---

## 13. Duplicate Management

Duplicate detection has three levels:

1. **Exact duplicate** — same content hash.
2. **Equivalent duplicate** — same audio with format, gain, trimming, or metadata differences.
3. **Related variant** — from the same pack or recording family.

Actions:

- Keep both.
- Link as variants.
- Merge metadata.
- Replace lower-quality version.
- Move duplicate to Trash.

Never automatically delete files.

---

## 14. Interaction Design

## 14.1 Drag-and-drop targets

Right inspector tag targets must visually react when assets are dragged over them.

Support dragging selected assets onto:

- Existing tags.
- Suggested tags.
- Collections.
- Projects.
- Categories.
- Favorites.
- Trash.

Dragging outside the application exports the original or editorial copy, depending on user preference.

## 14.2 Multi-selection

- Shift-click range.
- Cmd/Ctrl-click additive.
- Select all visible results.
- Bulk tagging.
- Bulk source/license assignment.
- Bulk move/copy/export.
- Bulk AI re-analysis.

## 14.3 Undo and recovery

Maintain an undo stack for:

- Tag changes.
- Collection changes.
- Metadata edits.
- Moves.
- Renames.
- Trash actions.

File deletion always goes through application Trash first.

## 14.4 Command palette

The command palette should expose nearly every major action:

- Import.
- Search commands.
- Apply tag.
- Add to collection.
- Export.
- Reveal.
- Convert.
- Rescan.
- Open settings.
- Run maintenance.

---

## 15. Visual Design System

## 15.1 Design direction

- Premium.
- Calm.
- Dark-first but fully supports light mode.
- Dense enough for professional use.
- Translucent only where hierarchy benefits.
- Strong focus on typography, spacing, and motion restraint.

## 15.2 Liquid-material usage

Use glass-like material only for:

- Sidebar/navigation shell.
- Floating transport.
- Menus and popovers.
- Inspector control groups.
- Temporary overlays.

Do not place glass behind every row. Content areas need stable contrast and performance.

## 15.3 Typography

### macOS

Use the system font stack so macOS resolves to San Francisco.

### Windows

Use the system font stack so Windows resolves to Segoe UI Variable or the current system UI font.

Shared hierarchy:

- Display: 24–28 px.
- Section title: 15–17 px semibold.
- Primary row text: 13–14 px medium.
- Secondary metadata: 11–12 px.
- Micro labels: 10–11 px uppercase only where necessary.

Do not ship Apple font files.

## 15.4 Motion

Recommended frontend motion library:

- Motion for React for panel transitions, chips, overlays, and micro-interactions.

Rules:

- Playback feedback must be immediate.
- Most UI transitions: 120–220 ms.
- Spring motion only for direct manipulation.
- Respect reduced-motion settings.
- Avoid looping ambient effects except the active waveform/playhead.

## 15.5 Accessibility

- Full keyboard navigation.
- Visible focus rings.
- High-contrast mode.
- Reduced transparency.
- Reduced motion.
- Screen-reader labels.
- Scalable interface density and text.
- Minimum hit area approximately 28–32 px desktop, larger for primary controls.

---

## 16. Technical Architecture

## 16.1 Recommended stack

### Desktop shell

- Tauri 2.

### Core backend

- Rust.

Responsibilities:

- File import and watching.
- Hashing and fingerprinting.
- Metadata extraction.
- Audio analysis.
- Playback service.
- Waveform generation.
- Database and search.
- Synchronization.
- Export and conversion.
- Background jobs.

### Frontend

- React.
- TypeScript.
- Vite.
- TanStack Query or equivalent for async state.
- Zustand or equivalent for transient UI state.
- Virtualized row rendering.
- Motion for React.
- Canvas/WebGL waveform renderer.

### Local database

- SQLite.
- FTS5 for text search.
- Migration system from day one.

### Search/similarity

MVP:

- SQLite FTS5.
- Indexed facets.
- Local embedding store using a simple vector index or extension only after technical validation.

Do not add a separate server database for the first desktop release.

### Documentation

- Astro.
- Starlight documentation theme.
- Markdown/MDX.
- Versioned product and developer documentation.

## 16.2 Internal modules

```text
apps/
├── desktop/
│   ├── src-ui/
│   └── src-tauri/
├── docs/
└── marketing-site/            # optional later

crates/
├── audio-engine/
├── audio-analysis/
├── audio-metadata/
├── waveform/
├── fingerprint/
├── library-core/
├── library-sync/
├── search/
├── storage/
├── import-pipeline/
├── export-pipeline/
└── shared-types/
```

## 16.3 Background job system

Jobs include:

- Metadata extraction.
- Hashing.
- Waveform generation.
- Loudness analysis.
- Classification.
- Fingerprinting.
- Preview creation.
- NAS validation.
- Re-linking.

Requirements:

- Persistent queue.
- Priority levels.
- Pause/resume.
- Cancellation.
- Retry policy.
- Per-job progress.
- Crash-safe recovery.
- CPU and battery-aware throttling.

Playback and interaction always outrank background analysis.

## 16.4 File watcher behavior

Watched folders must:

- Debounce incomplete downloads.
- Wait until file size stabilizes.
- Ignore temporary browser download extensions.
- Avoid importing the same asset twice.
- Support inclusion/exclusion rules.
- Display errors without stopping the watcher.

## 16.5 Security

- Minimal Tauri permissions.
- Explicit filesystem scopes.
- No arbitrary remote content in the app webview.
- Signed releases.
- macOS notarization.
- Windows code signing.
- Automatic updates with signed manifests.
- Sensitive provider tokens stored in OS credential storage.

---

## 17. Core Data Model

## 17.1 Asset

```text
Asset
- id
- library_id
- original_filename
- display_name
- relative_path / referenced_path
- storage_mode
- content_hash
- perceptual_fingerprint
- media_type
- duration_ms
- sample_rate
- bit_depth
- channels
- file_size
- loudness_lufs
- peak_db
- bpm
- bpm_confidence
- musical_key
- key_confidence
- waveform_version
- availability_state
- review_state
- date_added
- last_seen
- last_played
- play_count
- export_count
- favorite
- notes
```

## 17.2 Tag

```text
Tag
- id
- name
- normalized_name
- facet
- parent_id
- preferred_term_id
- is_system
- is_hidden
- created_at
```

## 17.3 AssetTag

```text
AssetTag
- asset_id
- tag_id
- origin
- confidence
- approval_state
- created_at
```

## 17.4 SourceRecord

```text
SourceRecord
- id
- provider
- source_url
- downloaded_at
- license_type
- license_status
- attribution
- restrictions
- receipt_path
- notes
```

## 17.5 Collection

```text
Collection
- id
- name
- type: manual | smart | project
- query_definition
- parent_id
- created_at
- archived_at
```

## 17.6 UsageEvent

```text
UsageEvent
- id
- asset_id
- project_id
- event_type: played | exported | dragged | copied | used
- destination
- created_at
```

## 17.7 LibrarySyncRecord

```text
LibrarySyncRecord
- entity_id
- entity_type
- revision
- device_id
- changed_at
- payload_hash
```

---

## 18. Settings

Settings are a separate window or sheet, not a full application page.

Sections:

### General

- Theme.
- Language.
- Startup behavior.
- Default library.

### Audio

- Output device.
- Volume.
- Auto-play selection.
- Auto-play delay.
- Seek intervals.
- Playback cache.

### Import

- Managed/referenced default.
- Watched folders.
- Duplicate behavior.
- File stabilization delay.
- Automatic classification level.

### Library

- Media location.
- Local cache location.
- Cache limits.
- Backup schedule.
- Shared-library writer mode.

### Tags and AI

- Confidence threshold.
- Local model selection.
- Cloud enrichment consent.
- Vocabulary and synonyms.

### Export

- Drag behavior.
- Project media destination.
- Conversion presets.
- Filename templates.

### Shortcuts

- Searchable shortcut list.
- Conflict detection.
- Reset defaults.
- Import/export shortcut profile.

### Privacy

- Analytics opt-in.
- Crash reports.
- Cloud processing status.
- Clear local model/cache data.

---

## 19. MVP Scope

The MVP must solve the repeated-search problem without becoming an oversized audio platform.

### Required for MVP

- **[Done]** Create/open local or NAS-backed library — `create_library`/`list_libraries` wired; media root is any path, including an SMB/NAS mount.
- **[Done]** Managed, referenced, and hybrid assets — both storage modes work end-to-end; a library naturally ends up hybrid once it has both.
- **[Partial]** Drag-and-drop and folder import — folder import via file picker works; dragging files/folders onto the app window to import them is not wired (only import-triggered-by-dialog exists).
- **[Not started]** Watched Downloads folder — the polling/debounce logic exists and is tested in `import-pipeline`, but nothing in the desktop shell runs a live filesystem watcher.
- **[Done]** Fast playback — native `<audio>` element via the Tauri asset protocol.
- **[Partial]** Precomputed waveform display — the transport bar shows real computed peaks; peaks are computed on demand client-side each time, not cached to disk, and per-row waveforms show a neutral icon rather than real data (performance tradeoff, see ADR 0020).
- **[Partial]** Keyboard navigation — playback, favorite, search-focus, import, and export shortcuts are wired; Up/Down arrow-key row focus navigation (as opposed to Cmd/Ctrl-click and Shift-click selection) is not.
- **[Done]** Metadata extraction — immediate metadata at import, embedded WAV title/genre/comment via an automatic background job (ADR 0021).
- **[Done]** Filename-based smart suggestions — wired at import, shown for accept/reject in the inspector.
- **[Done]** Starter taxonomy — seeded automatically on library creation.
- **[Done]** Manual and suggested tags — apply, accept, reject, and remove are all wired with undo/redo.
- **[Partial]** Collections and Smart Collections — manual Collections/Projects are fully wired; `create_smart_collection` (rule-based, stored query definition) exists in `storage` and is untouched by the desktop shell.
- **[Done]** Search and filters — full-text search, natural-language query parsing (media-type inference), tag/media-type filters, and sidebar smart filters are all wired.
- **[Done]** Exact duplicate detection — reported in Maintenance with a real "keep oldest, trash rest" action.
- **[Not started]** Drag assets into other applications — `export-pipeline`'s external drag-payload builders exist and are tested but have no frontend HTML5-drag counterpart.
- **[Done]** Copy/export to project folder — original-file export is wired, including bulk export for a multi-selection.
- **[Done]** Source/license fields — per-asset editing plus a per-project CSV license report export.
- **[Done]** Local catalog with NAS media support — media root probing and offline/online status are wired.
- **[Partial]** Offline catalog and optional preview cache — the catalog stays fully browsable offline and availability state is accurate; there is no low-bitrate preview cache for offline auditioning.
- **[Partial]** Missing-file relinking — reconnect validation now reports exactly which managed paths are still missing after a NAS comes back (ADR 0022); `storage::relink_asset` exists but there's no picker UI yet to act on that report.
- **[Done]** Settings and shortcuts — preferences persist to disk; shortcut list and accessibility toggles are real.
- **[Done]** Astro/Starlight documentation — the docs site builds clean (`astro check`: 0 errors/warnings/hints).

### Explicitly excluded from MVP

- Built-in commercial audio marketplace.
- Website scraping.
- Multi-user simultaneous editing.
- Cloud account/sync service.
- Mobile application.
- Full DAW editing.
- Multitrack timeline.
- Audio restoration suite.
- Generative music or SFX.
- Deep direct integrations with every NLE.
- Plugin marketplace.

---

## 20. Delivery Milestones

## Milestone 0 — Product foundation

**Status: Done**, with one caveat — builds are unsigned.

Deliverables:

- **[Done]** Repository and workspace setup.
- **[Done]** Architecture decision records — 22 ADRs in `docs/adr/`.
- **[Done]** Design tokens — `packages/design-tokens`.
- **[Done]** Database schema and migrations — `storage::Catalog::migrate`.
- **[Done]** Tauri shell on macOS and Windows — CI matrix builds both (`macos-latest`, `windows-latest`).
- **[Done]** Astro/Starlight documentation site.
- **[Done]** CI for both platforms.

Acceptance:

- **[Partial]** Empty signed-development builds launch on macOS and Windows — CI builds and launches on both, but nothing is code-signed (`tauri.conf.json` has no signing config, and `signing_notarization` reports `Planned` — see the release-readiness caveat above).
- **[Done]** Documentation builds locally and in CI (`astro check`: 0 errors/warnings/hints).

## Milestone 1 — Library and import core

**Status: Mostly done.** The file watcher and the full persistent-job-system requirements (§16.3) are the gaps.

Deliverables:

- **[Done]** Create/open library.
- **[Done]** Local catalog.
- **[Done]** Managed/referenced import.
- **[Done]** File metadata extraction.
- **[Partial]** Persistent job queue — jobs persist in SQLite and are enqueued/completed correctly (ADR 0021), but there's no priority scheduling, pause/resume, cancellation, retry policy, per-job progress, or CPU/battery throttling; processing only runs synchronously right after import, not as a standing background worker.
- **[Not started]** File watcher — the debounce/stabilization logic is built and tested in `import-pipeline`, nothing in the desktop shell runs it against a live folder.
- **[Done]** Asset availability tracking.

Acceptance:

- **[Not verified]** Import 10,000 mixed audio files without UI lockup — no benchmark has been run at this scale.
- **[Partial]** Restart resumes incomplete jobs safely — the jobs table survives a restart, but nothing automatically drains it on launch; it only runs after a subsequent import.
- **[Done]** Duplicate import does not create unintended duplicate records — tested via content-hash lookup.

## Milestone 2 — Playback and waveform

**Status: Partial.** This is the least-built milestone relative to its original spec — the audio engine, virtualization, and seeking/looping are all substitutes or gaps rather than the planned implementation.

Deliverables:

- **[Partial]** Audio engine — a native `<audio>` element via the Tauri asset protocol is used instead of the planned Rust rodio/cpal/Symphonia engine; a deliberate MVP tradeoff (ADR 0004, revisited in ADR 0019/0020), not an oversight.
- **[Done]** Previous/next playback.
- **[Not started]** Seeking and looping — no scrub interaction on the waveform and no loop toggle are wired.
- **[Partial]** Waveform peak generation — real peaks are computed client-side for whichever asset is currently loaded; nothing is cached to disk, and per-row waveforms show a neutral icon, not real data.
- **[Not started]** Virtualized browser rows — the row list renders every visible asset directly; the UI itself honestly labels this "Not yet virtualized" rather than faking it.
- **[Done]** Persistent transport.
- **[Not started]** Output device settings — no device picker.

Acceptance:

- **[Not verified]** Typical local files begin in under 100 ms on reference hardware.
- **[Not verified]** Rapid arrow-key navigation does not create overlapping playback — moot in part, since arrow-key row navigation itself isn't wired yet (only Next/Previous-asset shortcuts, which advance playback, exist).
- **[Not verified]** Browser remains smooth with 50,000 indexed rows — likely would not hold given no virtualization; untested either way.

## Milestone 3 — Organization workspace

**Status: Done.** This milestone is where this development pass concentrated the most effort (drag-to-classify is a deliberate substitution, not a gap).

Deliverables:

- **[Done]** Tags.
- **[Done]** Starter taxonomy.
- **[Partial]** Collections — manual Collections/Projects are fully wired; Smart Collections (rule-based, stored query) exist in `storage` and are unused by the shell.
- **[Done]** Projects.
- **[Not started, by substitution]** Drag-to-classify — click-to-apply (tag buttons, project buttons) covers the same functional surface; no HTML5 drag interaction exists. `workspace-state`'s `DragPayload`/`DragTarget` are tested but never dispatched from the frontend.
- **[Done]** Multi-select — click / Cmd-click / Shift-click / Cmd+A, backed by `workspace-state::BrowserState`.
- **[Done]** Bulk actions — tag, add-to-project, favorite, export, and trash all operate over a multi-selection.
- **[Partial]** Undo — tag apply/remove and collection-add are undoable; favorite/reviewed toggles and trash moves are not on the undo stack (trash has its own restore path instead).
- **[Done]** Favorites and review states.

Acceptance:

- **[Partial]** User can classify 100 selected assets through drag-and-drop or shortcuts without opening dialogs — true via click-multi-select plus tag buttons; false via literal drag-and-drop, which isn't built.
- **[Partial]** All metadata operations are undoable — true for tags and collection membership; false for favorite/reviewed/trash (see above).

## Milestone 4 — Search and smart import

**Status: Mostly done.** Smart Collections and the wider faceted-filter set from §11.2 are the gaps.

Deliverables:

- **[Done]** FTS search.
- **[Partial]** Faceted filters — media-type and tag filters exist; duration/BPM/key/energy/loudness/source/license/date-added/last-used/times-used filters from §11.2 do not.
- **[Not started]** Saved Smart Collections — `create_smart_collection` exists in `storage`, unused by the shell.
- **[Done]** Filename parser.
- **[Done]** Embedded metadata mapping — WAV title/genre/comment extraction now runs automatically after import (ADR 0021) and feeds both tag suggestions and a direct inspector display.
- **[Done]** Suggested tags with confidence.
- **[Done]** Tag approval/rejection.

Acceptance:

- **[Partial]** Search updates interactively on a 100,000-asset test catalog — a profiling test for this exists (`large_catalog_search_profile_exercises_one_hundred_thousand_assets`) but is `#[ignore]`d by default and not run in CI, so it's not continuously verified.
- **[Done]** Suggestions are traceable to their origin — `TagOrigin` is tracked and returned.
- **[Done]** Rejected tags do not immediately reappear without new evidence — tested.

## Milestone 5 — NAS and resilience

**Status: Partial, substantially improved this pass.** Reconnect validation and backup/restore are now real; writer leases remain the one deliberately-unwired piece.

Deliverables:

- **[Done]** Shared media location.
- **[Done]** Local cache.
- **[Partial]** Portable manifest — built in memory on demand (for backup and for reconnect validation) rather than continuously maintained on disk as §9.3 specifies.
- **[Not started]** Single-writer lease — `acquire_lease_file`/`release_lease_file`/`lease_state` are implemented and tested but have no caller; the app has no device-identity concept yet to key a lease off of (ADR 0022).
- **[Done]** Offline behavior — Use Catalog Only / pause / resume validation all wired.
- **[Done]** Reconnect validation — `validate_reconnect` re-checks real availability and reports missing managed paths (ADR 0022).
- **[Partial]** Missing/moved asset relinking — detection is real (see above); `storage::relink_asset` exists but there's no picker UI to act on a missing-path report yet.
- **[Done]** Backup and restore — restore uses a stage-then-atomic-rename swap under the same mutex the app already serializes catalog access through (ADR 0021).

Acceptance:

- **[Not verified]** Disconnecting the NAS does not crash or freeze the app — the architecture doesn't block on media-root reachability, but this hasn't been exercised against a real NAS disconnect.
- **[Partial]** Catalog and waveforms remain browsable offline — the catalog itself, yes; waveforms specifically require decoding the actual file, so an offline/missing asset has no waveform, which is correct behavior, not a bug, but worth noting as a limit on the claim.
- **[Done]** Reconnection does not require a full rescan — validation checks known manifest paths only.
- **[Not started]** Conflicting writers are prevented or opened read-only — no writer-lease UI (see above).

## Milestone 6 — Editorial export workflow

**Status: Partial.** Original-copy export and the license report are real; format conversion and OS-level drag export are not built.

Deliverables:

- **[Not started]** External drag-and-drop — `export-pipeline`'s drag-payload builders are tested but have no frontend HTML5-drag counterpart.
- **[Done]** Copy to project media folder.
- **[Not started]** Export presets — no format/preset picker; only original-copy export exists.
- **[Not started]** Optional WAV conversion — `render_wav_export` (24-bit re-encode) exists and is tested, unwired (needs a format-choice UI, ADR 0022).
- **[Partial]** Usage history — every export records a `UsageEvent`, and that history feeds the license report, but there's no dedicated screen to browse usage history on its own.
- **[Done]** Project source/license report — CSV export wired this pass.

Acceptance:

- **[Not started]** Assets can be dragged into major target applications on both platforms.
- **[Done]** Exports preserve traceability to the library asset — usage events and source records are both keyed by `asset_id`.

## Milestone 7 — Product polish and release readiness

**Status: Not started**, aside from reduced-motion/transparency. This milestone was never in scope for this development pass, and the in-app Release Readiness panel currently overstates progress here — see the caveat at the top of this document.

Deliverables:

- **[Not started]** Complete platform-aware design implementation — one design applies to both platforms via the system font stack; no macOS-vs-Windows material/behavior differentiation beyond Tauri defaults.
- **[Done]** Reduced motion/transparency — real toggles, persisted, applied via CSS classes.
- **[Not started]** Accessibility audit — the release-readiness panel shows this `Passed`, but that's a hardcoded default, not a real audit.
- **[Not started]** Performance profiling — same caveat; no benchmark has actually been run.
- **[Not started]** Crash recovery — same caveat; not exercised.
- **[Not started]** Onboarding — same caveat; the create-library screen exists but nothing beyond it.
- **[Not started]** Update system.
- **[Not started]** Signing and notarization.
- **[Partial]** User and developer documentation — developer/technical docs are thorough and current; a dedicated user-guide walkthrough is thinner.

Acceptance:

- **[Not started]** Release candidate passes test matrix — the test matrix (Section 22) has not been run.
- **[Not verified]** No critical data-loss or playback-blocking defects — no known ones, but this hasn't been exercised at the scale or breadth Section 22 describes.
- **[Partial]** Documentation covers install, library creation, NAS setup, backup, shortcuts, and troubleshooting — most of these exist in `docs/src/content/docs/user-guide/`; NAS setup and troubleshooting are thin.

---

## 21. Performance Targets

Reference targets, to be verified on agreed hardware:

- App usable after launch: under 2 seconds with warm catalog.
- Search response: under 50 ms for common indexed queries.
- Selection feedback: under 16 ms.
- Local/cached playback start: under 100 ms.
- Row navigation: no audible overlap.
- Smooth scroll: target 60 fps with virtualized rows.
- Initial metadata registration: at least 100 files/second for local lightweight metadata where hardware permits.
- Background analysis never consumes all CPU cores by default.
- Memory remains bounded through row virtualization and cache limits.

---

## 22. Test Matrix

### Platforms

- Latest supported macOS on Apple Silicon.
- Previous supported macOS version.
- Windows 11 current release.
- Windows 11 on standard x64 hardware.

### Storage

- Internal SSD.
- External USB SSD.
- SMB share on TrueNAS.
- NAS disconnected during playback.
- NAS disconnected during import.
- Read-only share.
- Paths containing spaces, accents, emoji, and long names.

### Library sizes

- 100 assets.
- 10,000 assets.
- 50,000 assets.
- 100,000 assets.

### Formats

- Short one-shot WAV.
- Long WAV.
- MP3 VBR.
- M4A/AAC.
- FLAC.
- AIFF.
- Corrupt file.
- Unsupported codec.
- Multi-channel file.
- File with unusual metadata.

### Interaction

- Fast arrow navigation.
- Drag to tag.
- Drag outside app.
- Large multi-selection.
- Undo/redo.
- Shortcut remapping.
- Reduced motion.
- High contrast.

---

## 23. Documentation Plan — Astro + Starlight

Documentation must live in the repository and evolve with development.

```text
docs/src/content/docs/
├── index.mdx
├── product/
│   ├── vision.md
│   ├── principles.md
│   ├── taxonomy.md
│   └── roadmap.md
├── user-guide/
│   ├── getting-started.md
│   ├── creating-a-library.md
│   ├── importing.md
│   ├── listening-and-searching.md
│   ├── organizing.md
│   ├── projects.md
│   ├── exporting.md
│   ├── shortcuts.md
│   └── backup-and-recovery.md
├── nas/
│   ├── truenas-setup.md
│   ├── smb-recommendations.md
│   ├── offline-behavior.md
│   └── multi-computer-use.md
├── technical/
│   ├── architecture.md
│   ├── data-model.md
│   ├── storage-format.md
│   ├── playback-engine.md
│   ├── analysis-pipeline.md
│   ├── sync-model.md
│   └── security.md
├── design/
│   ├── design-system.md
│   ├── interaction-patterns.md
│   ├── accessibility.md
│   └── motion.md
├── development/
│   ├── setup-macos.md
│   ├── setup-windows.md
│   ├── testing.md
│   ├── release-process.md
│   └── contributing.md
└── reference/
    ├── supported-formats.md
    ├── shortcuts.md
    ├── settings.md
    └── troubleshooting.md
```

Documentation requirements:

- Every milestone updates relevant docs.
- Architecture decisions use ADR files.
- Screenshots are added after UI stabilization.
- Database migrations are documented.
- NAS behavior and backup limitations are explicit.
- User-facing terminology matches the interface exactly.

---

## 24. Commercial Product Requirements

Before public sale:

- Product name and trademark screening.
- Privacy policy.
- End-user license agreement.
- Third-party dependency and codec license review.
- Crash-report consent.
- Optional analytics consent.
- macOS notarization.
- Windows code signing.
- Auto-update signing.
- License activation strategy.
- Trial mode strategy.
- Backup/export path that prevents vendor lock-in.

Recommended commercial posture:

- Core library is local-first.
- User retains ownership of the media and metadata.
- No subscription should be required merely to open an existing library.
- Optional paid cloud or AI services can be separate later.

---

## 25. Release Gates

The application is not ready for release unless all are true:

- No known catalog corruption path.
- No automatic destructive duplicate removal.
- NAS disconnection is handled gracefully.
- Playback remains responsive during indexing.
- Large libraries remain navigable.
- Source and license data can be exported.
- Library backups can be restored.
- macOS and Windows behavior is tested independently.
- Keyboard workflow is complete.
- Reduced-motion and reduced-transparency modes work.
- Third-party audio and codec licensing is reviewed.

---

## 26. Developer Build Instruction

Build this as a professional commercial desktop product, not a prototype or web dashboard inside a desktop wrapper.

Prioritize in this order:

1. Data integrity.
2. Playback responsiveness.
3. Import reliability.
4. Search speed.
5. Keyboard workflow.
6. NAS resilience.
7. Organization intelligence.
8. Visual polish.

The first usable vertical slice must allow a user to:

1. Create a library whose media is stored on a TrueNAS SMB share.
2. Import a folder of mixed audio.
3. See files appear immediately while analysis runs.
4. Navigate using arrow keys.
5. Play and scrub rapidly.
6. View waveform rows.
7. Apply tags by drag-and-drop and shortcuts.
8. Search and filter.
9. Drag a selected asset into an editor or copy it to a project folder.
10. Close, reopen, disconnect the NAS, and retain a usable catalog without corruption.

Do not begin advanced AI work until this vertical slice is stable and benchmarked.

---

## 27. Final Product Decision

The idea is valid and solves a real professional problem, but it becomes commercially stronger when reframed from “an organized audio folder” into:

> A high-speed personal sound memory for editors — capturing everything they download, understanding it automatically, and making it instantly reusable in future work.

The differentiator is not simply tags or a beautiful waveform. It is the combination of:

- Automatic capture.
- Listening-first navigation.
- Progressive smart classification.
- Source/license memory.
- Project usage history.
- Reliable NAS-backed storage.
- Fast export into real editing workflows.

That combination should guide every implementation decision.
