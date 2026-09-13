use chrono::Utc;
use rusqlite::{params, params_from_iter, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use shared_types::{AvailabilityState, StorageMode};
use std::path::Path;
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
pub enum StorageError {
    #[error("path must be absolute")]
    RelativePath,
    #[error("sqlite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("asset not found")]
    AssetNotFound,
    #[error("collection not found")]
    CollectionNotFound,
    #[error("collection is not a smart collection")]
    NotASmartCollection,
    #[error("smart collection query is invalid: {0}")]
    InvalidSmartCollectionQuery(String),
    #[error("invalid input: {0}")]
    InvalidInput(String),
}

pub fn is_network_tolerant_catalog_path(path: &str) -> Result<bool, StorageError> {
    if !path.starts_with('/') && !path.contains(":\\") {
        return Err(StorageError::RelativePath);
    }

    Ok(!path.contains("/Volumes/") && !path.starts_with("\\\\"))
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct LibraryRecord {
    pub id: Uuid,
    pub name: String,
    pub media_root: String,
    /// macOS App Store build only: a base64-encoded NSURL security-scoped
    /// bookmark for `media_root`, so a sandboxed process can regain access
    /// to a user-picked folder (including an SMB/NAS mount) on a later
    /// launch without a fresh folder-picker prompt. `None` on every other
    /// build — the direct-sale build is unsandboxed and never needs one.
    pub media_root_bookmark: Option<String>,
    /// Folder the app scans for new sounds to auto-import (on launch and on
    /// manual refresh — see `scan_import_folder`), distinct from
    /// `media_root` where the organized library actually lives. `None`
    /// until the user sets one, e.g. in the first-run wizard.
    pub import_root: Option<String>,
    /// Named subfolders under `media_root` a file dropped onto the app
    /// window can be filed into (e.g. `["Soundtrack", "SFX"]`) — see
    /// `set_library_import_subfolders`. `None` or empty means there's only
    /// one destination (`media_root` itself), so a drop never needs to ask
    /// "which folder"; 2+ names means it does, unless the user's chosen to
    /// remember an answer for this session.
    pub import_subfolders: Vec<String>,
}

/// A folder the user has pointed Darkwave at, beyond the single implicit
/// `media_root`/`import_root` every library already had — see `add_folder`.
/// Both `role` and `kind` are plain strings (matching how `media_type` is
/// already stored on `AssetRecord`), not Rust enums: known `role` values are
/// `"music"`, `"sound_effect"`, `"voiceover"`, `"foley"`, `"ambience"`,
/// `"documents"`, `"needs_review"`, or `None` for "no specific type"; known
/// `kind` values are `"media_root"`, `"watched"`, and `"documents"`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FolderRecord {
    pub id: Uuid,
    pub library_id: Uuid,
    pub path: String,
    pub role: Option<String>,
    pub kind: String,
    pub created_at: String,
}

/// A file discovered inside a `kind = "documents"` folder — see
/// `record_folder_document`. Inventory only: no analysis, no `AssetRecord`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct FolderDocumentRecord {
    pub id: Uuid,
    pub library_id: Uuid,
    pub folder_id: Uuid,
    pub path: String,
    pub filename: String,
    pub discovered_at: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AssetPath {
    Managed(String),
    Referenced(String),
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NewAssetRecord {
    pub library_id: Uuid,
    pub original_filename: String,
    pub display_name: String,
    pub path: AssetPath,
    pub storage_mode: StorageMode,
    pub content_hash: Option<String>,
    pub media_type: String,
    pub file_size: u64,
    pub availability_state: AvailabilityState,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssetRecord {
    pub id: Uuid,
    pub library_id: Uuid,
    pub original_filename: String,
    pub display_name: String,
    pub path: AssetPath,
    pub storage_mode: StorageMode,
    pub content_hash: Option<String>,
    pub media_type: String,
    pub file_size: u64,
    pub availability_state: AvailabilityState,
    pub review_state: ReviewState,
    pub favorite: bool,
    pub embedded_title: Option<String>,
    pub embedded_genre: Option<String>,
    pub embedded_comment: Option<String>,
    pub duration_ms: Option<i64>,
    pub sample_rate: Option<i64>,
    pub bit_depth: Option<i64>,
    pub channels: Option<i64>,
    pub loudness_lufs: Option<f64>,
    pub peak_db: Option<f64>,
    pub bpm: Option<f64>,
    pub bpm_confidence: Option<f64>,
    /// Best-effort detected pitch note name (e.g. "A4"), not a musical key —
    /// a monophonic estimate, best on single-source SFX/ambience/drones. See
    /// docs/adr/0025-real-audio-analysis.md. For the polyphonic musical key
    /// (e.g. "A minor") use `detected_key`.
    pub musical_key: Option<String>,
    pub key_confidence: Option<f64>,
    /// Fraction of the clip Silero VAD classifies as speech (ADR 0027).
    /// `None` until the analysis job runs, or if it couldn't run at all.
    pub vocal_ratio: Option<f64>,
    /// Krumhansl-Schmuckler musical key, e.g. "C major" / "A minor" (ADR
    /// 0030). `None` until analysis runs, or if the material is too atonal
    /// to call.
    pub detected_key: Option<String>,
    /// Chromagram/profile correlation behind `detected_key`, ~0.0..=1.0.
    pub key_strength: Option<f64>,
    /// Shared by every member of a detected stem group (the full mix plus
    /// its separated-instrument siblings — see `import_pipeline::stems`).
    /// `None` for an asset that isn't part of one.
    pub stem_group_id: Option<Uuid>,
    /// This member's part within its stem group, e.g. "Full Mix", "Drums",
    /// "Melody". `None` when `stem_group_id` is `None`.
    pub stem_label: Option<String>,
    /// Exactly one member per `stem_group_id` has this set — the one shown
    /// as the browsable row in the main list; the rest are only reachable
    /// through that row's Stems panel. Meaningless when `stem_group_id` is
    /// `None`.
    pub stem_is_primary: bool,
}

/// Fields written by the `AudioAnalysis` background job. `None` leaves the
/// corresponding column unchanged is NOT the semantics here — every field is
/// written as given, so pass the asset's existing value through for anything
/// the caller doesn't want to clear.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AudioAnalysisUpdate {
    pub duration_ms: Option<i64>,
    pub sample_rate: Option<i64>,
    pub bit_depth: Option<i64>,
    pub channels: Option<i64>,
    pub loudness_lufs: Option<f64>,
    pub peak_db: Option<f64>,
    pub bpm: Option<f64>,
    pub bpm_confidence: Option<f64>,
    pub musical_key: Option<String>,
    pub key_confidence: Option<f64>,
    pub perceptual_fingerprint: Option<String>,
    pub detected_key: Option<String>,
    pub key_strength: Option<f64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ReviewState {
    Unreviewed,
    Reviewed,
}

/// A persisted waveform peak payload for one asset — the output of the
/// `WaveformGeneration` job, so browsing an asset never has to decode its
/// source file again just to draw the transport strip. `payload` is an
/// opaque JSON string owned by the desktop shell (the `waveform` crate's
/// `WaveformCache` plus a flat transport strip); storage only persists and
/// returns it verbatim. `waveform_version` mirrors `assets.waveform_version`
/// at the time the payload was generated, so a stale cache can be detected.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WaveformCacheRow {
    pub sample_rate: i64,
    pub payload: String,
    pub waveform_version: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JobKind {
    MetadataExtraction,
    Hashing,
    WaveformGeneration,
    AudioAnalysis,
    InstrumentDetection,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobRecord {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub kind: JobKind,
    pub priority: i64,
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct JobStateCounts {
    pub pending: usize,
    pub failed: usize,
    pub completed: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TagOrigin {
    Filename,
    Metadata,
    AcousticModel,
    UserRule,
    UserCorrection,
    Manual,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TagApprovalState {
    Suggested,
    Accepted,
    Rejected,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TagRecord {
    pub id: Uuid,
    pub name: String,
    pub normalized_name: String,
    pub facet: Option<String>,
    pub is_system: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum CollectionType {
    Manual,
    Smart,
    Project,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CollectionRecord {
    pub id: Uuid,
    pub library_id: Uuid,
    pub name: String,
    pub collection_type: CollectionType,
    pub query_definition: Option<String>,
    /// Project's "sound folder" — where music quick-exports to (e.g. an
    /// editing app's watch folder), so an editor can drag it straight into
    /// a timeline. `None` until the editor configures one.
    pub export_path: Option<String>,
    /// Project's "sound effects folder" — where everything that isn't
    /// music quick-exports to (sound effects, foley, voiceover, ambience),
    /// organized into tag-named subfolders. `None` until configured.
    pub sfx_export_path: Option<String>,
}

/// A categorized export destination on a project beyond the two built-in
/// slots (`CollectionRecord::export_path`/`sfx_export_path`) — see
/// `add_project_export_folder`. `role` uses the same vocabulary a library
/// `folders` row does: `"music"`, `"sound_effect"`, `"voiceover"`,
/// `"foley"`, `"ambience"`, `"documents"`.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectExportFolderRecord {
    pub id: Uuid,
    pub collection_id: Uuid,
    pub path: String,
    pub role: String,
    pub created_at: String,
}

/// One (asset, project) membership row — see `project_memberships_for_library`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AssetProjectMembership {
    pub asset_id: Uuid,
    pub project_id: Uuid,
    pub project_name: String,
    pub export_path: Option<String>,
    pub sfx_export_path: Option<String>,
    pub exported: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum UsageEventType {
    Played,
    Exported,
    Dragged,
    Copied,
    Used,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct UsageEventRecord {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub project_id: Option<Uuid>,
    pub event_type: UsageEventType,
    pub destination: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct SourceRecordDraft {
    pub asset_id: Uuid,
    pub provider: Option<String>,
    pub source_url: Option<String>,
    pub license_type: Option<String>,
    pub license_status: Option<String>,
    pub attribution: Option<String>,
    pub restrictions: Option<String>,
    pub receipt_path: Option<String>,
    /// Path to a copied-in license/usage-rights PDF, relative to the
    /// library's `media_root` (same convention as `AssetPath::Managed`'s
    /// relative path) — e.g. `License/<asset-id>-invoice.pdf`. Set by
    /// `attach_license_document`, never typed directly by the user.
    pub license_document_path: Option<String>,
    /// ISO `YYYY-MM-DD`. Nullable — most license PDFs don't state a start
    /// date, only an expiry.
    pub license_valid_from: Option<String>,
    /// ISO `YYYY-MM-DD`. The authoritative field the expired/valid badge is
    /// computed from — deliberately separate from `license_status`'s free
    /// text, which the user typed and isn't machine-checked.
    pub license_valid_until: Option<String>,
    /// `"manual"` (the user typed or confirmed it) or `"extracted"` (still
    /// exactly what `license-documents::find_date_candidates` suggested,
    /// unconfirmed). Used only for a UI hint, never to gate anything.
    pub license_expiry_source: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectSourceReportRow {
    pub asset_id: Uuid,
    pub asset_title: String,
    pub original_filename: String,
    pub provider: Option<String>,
    pub source_url: Option<String>,
    pub license_type: Option<String>,
    pub license_status: Option<String>,
    pub attribution: Option<String>,
    pub restrictions: Option<String>,
    pub receipt_path: Option<String>,
    pub license_valid_until: Option<String>,
    pub usage_status: String,
    pub destination: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AssetSearchQuery {
    pub text: String,
    pub tag_id: Option<Uuid>,
    pub media_type: Option<String>,
    pub duration_min_ms: Option<i64>,
    pub duration_max_ms: Option<i64>,
    pub bpm_min: Option<f64>,
    pub bpm_max: Option<f64>,
    /// Loudest-peak range in dBFS — the "energy" facet. `peak_db` is
    /// already a real persisted column; a numeric range on `musical_key`
    /// wouldn't be meaningful, so pitch has no range filter.
    pub peak_db_min: Option<f64>,
    pub peak_db_max: Option<f64>,
}

impl AssetSearchQuery {
    pub fn text(text: impl AsRef<str>) -> Self {
        Self {
            text: text.as_ref().to_string(),
            ..Self::default()
        }
    }

    pub fn with_tag(mut self, tag_id: Uuid) -> Self {
        self.tag_id = Some(tag_id);
        self
    }

    pub fn with_media_type(mut self, media_type: impl AsRef<str>) -> Self {
        self.media_type = Some(media_type.as_ref().to_string());
        self
    }
}

pub struct Catalog {
    connection: Connection,
}

impl Catalog {
    pub fn open(path: impl AsRef<Path>) -> Result<Self, StorageError> {
        let connection = Connection::open(path)?;
        connection.pragma_update(None, "foreign_keys", "ON")?;
        // The whole app funnels every catalog read and write through one
        // connection behind a single mutex. WAL lets the in-flight
        // background writes (import registration, analysis results, waveform
        // payloads) stop blocking the interactive reads queued behind them,
        // which is the difference between "the UI is sluggish while a big
        // import runs" and not. `synchronous = NORMAL` is the standard,
        // safe-under-WAL pairing (a crash can lose the last commit, never
        // corrupt the file). `busy_timeout` covers the rare moment another
        // handle touches the file — a backup copy, an external inspector —
        // instead of erroring straight out. The live catalog always lives
        // on local app-data storage (see `run()`), so WAL's shared-memory
        // requirement is never an issue here; a failure to set it (e.g. a
        // catalog handed a path on an exotic filesystem) is not fatal.
        let _ = connection.pragma_update(None, "journal_mode", "WAL");
        let _ = connection.pragma_update(None, "synchronous", "NORMAL");
        let _ = connection.pragma_update(None, "busy_timeout", 5_000);
        let _ = connection.pragma_update(None, "temp_store", "MEMORY");
        let catalog = Self { connection };
        catalog.migrate()?;
        Ok(catalog)
    }

    /// Forces every committed WAL frame back into the main database file
    /// and truncates the WAL — needed before a plain filesystem copy of the
    /// database file (e.g. migrating a legacy shared catalog into a new
    /// per-library `.darkwave` file) so the copy can't silently miss
    /// recent writes that are still sitting only in `-wal`. A no-op, not an
    /// error, if this connection isn't in WAL mode for some reason.
    pub fn checkpoint_wal(&self) -> Result<(), StorageError> {
        self.connection
            .query_row("PRAGMA wal_checkpoint(TRUNCATE)", [], |_row| Ok(()))
            .map_err(StorageError::from)
    }

    /// Opens (creating if needed) a standalone project file: a `.darkwave`
    /// catalog the user saves, moves, and reopens like any other document,
    /// as opposed to a row inside one shared app-data catalog. Each project
    /// file owns exactly one library — if `path` is brand new, `name` seeds
    /// that library's name (with an empty `media_root`, to be set by the
    /// folder-setup step that follows); if `path` already holds a project,
    /// its existing library row is returned untouched and `name` is
    /// ignored. A project file that somehow already has more than one
    /// `libraries` row (should not happen through normal app usage) returns
    /// the first one created rather than erroring, since there's no UI for
    /// picking among several.
    pub fn open_or_init_library_file(
        path: impl AsRef<Path>,
        name: impl AsRef<str>,
    ) -> Result<(Self, LibraryRecord), StorageError> {
        let catalog = Self::open(path)?;
        let library = match catalog.list_libraries()?.into_iter().next() {
            Some(existing) => existing,
            None => catalog.create_library(name, "")?,
        };
        Ok((catalog, library))
    }

    pub fn create_library(
        &self,
        name: impl AsRef<str>,
        media_root: impl AsRef<str>,
    ) -> Result<LibraryRecord, StorageError> {
        let library = LibraryRecord {
            id: Uuid::new_v4(),
            name: name.as_ref().to_string(),
            media_root: media_root.as_ref().to_string(),
            media_root_bookmark: None,
            import_root: None,
            import_subfolders: Vec::new(),
        };
        let now = Utc::now().to_rfc3339();

        self.connection.execute(
            "INSERT INTO libraries (id, name, media_root, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![
                library.id.to_string(),
                library.name,
                library.media_root,
                now
            ],
        )?;

        Ok(library)
    }

    pub fn list_libraries(&self) -> Result<Vec<LibraryRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, media_root, media_root_bookmark, import_root, import_subfolders
             FROM libraries ORDER BY created_at ASC",
        )?;
        let libraries = statement
            .query_map([], |row| {
                Ok(LibraryRecord {
                    id: parse_uuid(row.get::<_, String>(0)?),
                    name: row.get(1)?,
                    media_root: row.get(2)?,
                    media_root_bookmark: row.get(3)?,
                    import_root: row.get(4)?,
                    import_subfolders: parse_import_subfolders(row.get(5)?),
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(libraries)
    }

    pub fn get_library(&self, id: Uuid) -> Result<Option<LibraryRecord>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, name, media_root, media_root_bookmark, import_root, import_subfolders
                 FROM libraries WHERE id = ?1",
                params![id.to_string()],
                |row| {
                    Ok(LibraryRecord {
                        id: parse_uuid(row.get::<_, String>(0)?),
                        name: row.get(1)?,
                        media_root: row.get(2)?,
                        media_root_bookmark: row.get(3)?,
                        import_root: row.get(4)?,
                        import_subfolders: parse_import_subfolders(row.get(5)?),
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    }

    /// Sets a library's media root after the fact — used to establish it
    /// automatically from the first folder someone imports into a library
    /// created without one (see ADR-worthy note in library-core::LibraryDraft).
    /// Overwrites unconditionally rather than only-if-empty, since the only
    /// caller already checks that first.
    pub fn set_library_media_root(
        &self,
        library_id: Uuid,
        media_root: impl AsRef<str>,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE libraries SET media_root = ?1 WHERE id = ?2",
            params![media_root.as_ref(), library_id.to_string()],
        )?;
        Ok(())
    }

    /// macOS App Store build only (see `LibraryRecord::media_root_bookmark`).
    /// `None` clears a stale bookmark — e.g. after the user re-picks the
    /// folder via a fresh dialog and a new bookmark hasn't been minted yet.
    pub fn set_library_media_root_bookmark(
        &self,
        library_id: Uuid,
        media_root_bookmark: Option<&str>,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE libraries SET media_root_bookmark = ?1 WHERE id = ?2",
            params![media_root_bookmark, library_id.to_string()],
        )?;
        Ok(())
    }

    /// Sets (or clears, with `None`) the folder the app scans for new
    /// sounds to auto-import. `Some("")` is treated the same as `None` so
    /// clearing the field from a text input works naturally.
    pub fn set_library_import_root(
        &self,
        library_id: Uuid,
        import_root: Option<&str>,
    ) -> Result<(), StorageError> {
        let import_root = import_root.filter(|path| !path.trim().is_empty());
        self.connection.execute(
            "UPDATE libraries SET import_root = ?1 WHERE id = ?2",
            params![import_root, library_id.to_string()],
        )?;
        Ok(())
    }

    /// Sets (or clears, with an empty slice) the named subfolders under
    /// this library's `media_root` a dropped file can be routed into —
    /// see `LibraryRecord::import_subfolders`. Blank names are dropped;
    /// duplicates (case-insensitive) are collapsed to the first spelling,
    /// since two entries that resolve to the same folder would make the
    /// "which one" prompt nonsensical.
    pub fn set_library_import_subfolders(
        &self,
        library_id: Uuid,
        names: &[String],
    ) -> Result<(), StorageError> {
        let mut seen = std::collections::HashSet::new();
        let cleaned: Vec<String> = names
            .iter()
            .map(|name| name.trim().to_string())
            .filter(|name| !name.is_empty())
            // Subfolders are always joined onto the library's own
            // media_root as a single path segment (see
            // import_dropped_paths) — a name containing a separator or
            // ".." could otherwise redirect a drop outside the library
            // entirely, silently, from what looks like an ordinary
            // settings field. Not a privilege-escalation risk (single-user
            // local app; the user already has whatever filesystem access
            // this would grant), but a real footgun worth closing: a typo
            // here shouldn't be able to make "drop to import" quietly
            // write files somewhere the user never intended.
            .filter(|name| !name.contains('/') && !name.contains('\\') && name != "." && name != "..")
            .filter(|name| seen.insert(name.to_lowercase()))
            .collect();
        let json = if cleaned.is_empty() {
            None
        } else {
            Some(serde_json::to_string(&cleaned).map_err(|error| {
                StorageError::InvalidInput(format!("could not encode import subfolders: {error}"))
            })?)
        };
        self.connection.execute(
            "UPDATE libraries SET import_subfolders = ?1 WHERE id = ?2",
            params![json, library_id.to_string()],
        )?;
        Ok(())
    }

    /// Adds a folder Darkwave should watch for this library — the basic
    /// single-folder setup adds exactly one (`role: None, kind:
    /// "media_root"`, mirroring `media_root` itself); the advanced setup
    /// adds one per additional folder, each optionally tagged with the
    /// media type it's expected to hold. `kind` is a plain string
    /// (`"media_root"` | `"watched"` | `"documents"`) rather than a Rust
    /// enum, matching how `media_type` on `AssetRecord` is already stored —
    /// no validation is done here on either string; callers (the Tauri
    /// commands) own picking from the known set.
    pub fn add_folder(
        &self,
        library_id: Uuid,
        path: impl AsRef<str>,
        role: Option<&str>,
        kind: impl AsRef<str>,
    ) -> Result<FolderRecord, StorageError> {
        let folder = FolderRecord {
            id: Uuid::new_v4(),
            library_id,
            path: path.as_ref().to_string(),
            role: role.map(|value| value.to_string()),
            kind: kind.as_ref().to_string(),
            created_at: Utc::now().to_rfc3339(),
        };
        self.connection.execute(
            "INSERT INTO folders (id, library_id, path, role, kind, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                folder.id.to_string(),
                folder.library_id.to_string(),
                folder.path,
                folder.role,
                folder.kind,
                folder.created_at
            ],
        )?;
        Ok(folder)
    }

    pub fn list_folders(&self, library_id: Uuid) -> Result<Vec<FolderRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, library_id, path, role, kind, created_at
             FROM folders WHERE library_id = ?1 ORDER BY created_at ASC",
        )?;
        let folders = statement
            .query_map(params![library_id.to_string()], |row| {
                Ok(FolderRecord {
                    id: parse_uuid(row.get::<_, String>(0)?),
                    library_id: parse_uuid(row.get::<_, String>(1)?),
                    path: row.get(2)?,
                    role: row.get(3)?,
                    kind: row.get(4)?,
                    created_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(folders)
    }

    /// Removes a watched folder. Only ever the `folders` row itself (plus
    /// its `folder_documents` rows, via cascade) — never touches anything
    /// on disk, and never touches assets already imported from files that
    /// used to live under it (they stay in the catalog, just with no
    /// folder-role signal to fall back on for anything imported later).
    pub fn remove_folder(&self, folder_id: Uuid) -> Result<(), StorageError> {
        self.connection.execute(
            "DELETE FROM folders WHERE id = ?1",
            params![folder_id.to_string()],
        )?;
        Ok(())
    }

    pub fn set_folder_role(&self, folder_id: Uuid, role: Option<&str>) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE folders SET role = ?1 WHERE id = ?2",
            params![role, folder_id.to_string()],
        )?;
        Ok(())
    }

    /// Records a file discovered inside a `kind = "documents"` folder (a
    /// licence/PDF folder, typically) — inventory only, never an
    /// `AssetRecord` and never a background job. `(folder_id, path)` is
    /// unique, so polling the same folder tick after tick is a safe no-op
    /// for files already known; returns `true` only when this call is what
    /// actually added the row, so a caller (the watched-folder poller) can
    /// tell a genuinely new discovery from an already-seen one without a
    /// separate read first.
    pub fn record_folder_document(
        &self,
        folder_id: Uuid,
        library_id: Uuid,
        path: impl AsRef<str>,
    ) -> Result<bool, StorageError> {
        let path = path.as_ref();
        let filename = Path::new(path)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(path);
        let inserted = self.connection.execute(
            "INSERT OR IGNORE INTO folder_documents (id, library_id, folder_id, path, filename, discovered_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                Uuid::new_v4().to_string(),
                library_id.to_string(),
                folder_id.to_string(),
                path,
                filename,
                Utc::now().to_rfc3339()
            ],
        )?;
        Ok(inserted > 0)
    }

    pub fn list_folder_documents(&self, folder_id: Uuid) -> Result<Vec<FolderDocumentRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, library_id, folder_id, path, filename, discovered_at
             FROM folder_documents WHERE folder_id = ?1 ORDER BY discovered_at ASC",
        )?;
        let documents = statement
            .query_map(params![folder_id.to_string()], |row| {
                Ok(FolderDocumentRecord {
                    id: parse_uuid(row.get::<_, String>(0)?),
                    library_id: parse_uuid(row.get::<_, String>(1)?),
                    folder_id: parse_uuid(row.get::<_, String>(2)?),
                    path: row.get(3)?,
                    filename: row.get(4)?,
                    discovered_at: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(documents)
    }

    /// Deletes a library and every catalog row that belongs to it (assets,
    /// background jobs, asset_tags, trash_items, collections,
    /// collection_assets, source_records) via `ON DELETE CASCADE` — the
    /// schema already declares every one of those foreign keys with
    /// cascade, and `foreign_keys` is turned on for this connection, so a
    /// single delete on `libraries` is sufficient and atomic. Deliberately
    /// never touches the filesystem: nothing under the library's
    /// `media_root` is read, referenced, or removed. The global `tags`
    /// table is untouched too — tags aren't library-scoped.
    pub fn delete_library(&self, library_id: Uuid) -> Result<(), StorageError> {
        self.connection.execute(
            "DELETE FROM libraries WHERE id = ?1",
            params![library_id.to_string()],
        )?;
        Ok(())
    }

    /// Registers a new asset, or returns the existing one unchanged if its
    /// content hash + size already match a row in this library — imports
    /// are idempotent by content, not just by path. The `bool` tells the
    /// caller which happened: `true` for a genuinely new row, `false` for
    /// an existing match. Callers that enqueue analysis jobs or write
    /// source/license metadata on import must check this — re-running
    /// those for an asset that's already fully analyzed re-does real,
    /// expensive work (decode + DSP + VAD) for nothing, and re-writing
    /// source metadata over a user's existing edits is a data-loss bug,
    /// not a no-op.
    pub fn register_asset(&self, asset: NewAssetRecord) -> Result<(AssetRecord, bool), StorageError> {
        if let Some(content_hash) = &asset.content_hash {
            if let Some(existing) =
                self.find_asset_by_hash(asset.library_id, content_hash, asset.file_size)?
            {
                return Ok((existing, false));
            }
        }

        let id = Uuid::new_v4();
        let now = Utc::now().to_rfc3339();
        let (relative_path, referenced_path) = match &asset.path {
            AssetPath::Managed(path) => (Some(path.as_str()), None),
            AssetPath::Referenced(path) => (None, Some(path.as_str())),
        };

        self.connection.execute(
            "INSERT INTO assets (
                id, library_id, original_filename, display_name, relative_path, referenced_path,
                storage_mode, content_hash, media_type, file_size, availability_state,
                date_added, last_seen
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                id.to_string(),
                asset.library_id.to_string(),
                asset.original_filename,
                asset.display_name,
                relative_path,
                referenced_path,
                storage_mode_to_db(&asset.storage_mode),
                asset.content_hash,
                asset.media_type,
                asset.file_size as i64,
                availability_to_db(&asset.availability_state),
                now,
                now,
            ],
        )?;
        self.get_asset(id)
            .map(|asset| (asset.expect("inserted asset exists"), true))
    }

    pub fn list_assets(&self, library_id: Uuid) -> Result<Vec<AssetRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, library_id, original_filename, display_name, relative_path, referenced_path,
                storage_mode, content_hash, media_type, file_size, availability_state, review_state, favorite,
                    embedded_title, embedded_genre, embedded_comment,
                    duration_ms, sample_rate, bit_depth, channels, loudness_lufs, peak_db,
                    bpm, bpm_confidence, musical_key, key_confidence, vocal_ratio, detected_key, key_strength,
                    stem_group_id, stem_label, stem_is_primary
             FROM assets
             WHERE library_id = ?1
               AND NOT EXISTS (
                 SELECT 1 FROM trash_items
                 WHERE trash_items.asset_id = assets.id AND trash_items.state = 'in_trash'
               )
             ORDER BY date_added ASC",
        )?;

        let assets = statement
            .query_map(params![library_id.to_string()], asset_from_row)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(assets)
    }

    pub fn enqueue_job(
        &self,
        asset_id: Uuid,
        kind: JobKind,
        priority: i64,
    ) -> Result<JobRecord, StorageError> {
        let job = JobRecord {
            id: Uuid::new_v4(),
            asset_id,
            kind,
            priority,
        };
        let now = Utc::now().to_rfc3339();

        self.connection.execute(
            "INSERT INTO background_jobs (id, asset_id, kind, priority, state, attempts, created_at, updated_at)
             VALUES (?1, ?2, ?3, ?4, 'pending', 0, ?5, ?5)",
            params![
                job.id.to_string(),
                job.asset_id.to_string(),
                job_kind_to_db(&job.kind),
                job.priority,
                now,
            ],
        )?;

        Ok(job)
    }

    pub fn next_pending_job(&self) -> Result<Option<JobRecord>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, asset_id, kind, priority
                 FROM background_jobs
                 WHERE state = 'pending'
                 ORDER BY priority ASC, created_at ASC
                 LIMIT 1",
                [],
                |row| {
                    Ok(JobRecord {
                        id: parse_uuid(row.get::<_, String>(0)?),
                        asset_id: parse_uuid(row.get::<_, String>(1)?),
                        kind: job_kind_from_db(&row.get::<_, String>(2)?),
                        priority: row.get(3)?,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    }

    pub fn complete_job(&self, job_id: Uuid) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE background_jobs SET state = 'completed', updated_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), job_id.to_string()],
        )?;
        Ok(())
    }

    pub fn fail_job(&self, job_id: Uuid, error: &str) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE background_jobs SET state = 'failed', attempts = attempts + 1, error = ?1, updated_at = ?2 WHERE id = ?3",
            params![error, Utc::now().to_rfc3339(), job_id.to_string()],
        )?;
        Ok(())
    }

    /// Puts a claimed (`processing`) job back to `pending` without counting
    /// it as a failure — for "not actually wrong, just not ready yet" cases
    /// like a Referenced/NAS asset whose file isn't reachable this attempt.
    /// Several process_one_*_job functions used to just `return` here with
    /// no DB write at all, leaving the row silently stuck at `processing`
    /// (comments claimed "left pending" but nothing made that true) — which
    /// starved job_state_counts_for_library's `pending` count, which starved
    /// the frontend's "is there work to do" check, which is the only thing
    /// that ever calls back in to let reset_stuck_processing_jobs self-heal
    /// it. A kind whose entire remaining queue hit this path could get
    /// stuck forever with no error and no result — this closes that loop
    /// explicitly instead of hoping something else resets it later.
    pub fn requeue_job_as_pending(&self, job_id: Uuid) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE background_jobs SET state = 'pending', updated_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), job_id.to_string()],
        )?;
        Ok(())
    }

    /// The single most recent job of `kind` for one asset — `(state,
    /// error)`, or `None` if no such job has ever been queued. Used by the
    /// inspector's per-track Analyze buttons to poll *this asset's own*
    /// outcome directly, instead of the shared, per-kind `job_status`
    /// counters those buttons used to rely on: that library-wide view can't
    /// distinguish "my job" from someone else's already-in-flight drain of
    /// the same kind, which is what let a real completion (or failure) go
    /// unreported. `resync_analysis` deletes any prior non-processing job
    /// for this (asset, kind) before inserting a fresh one, so at most one
    /// row ever matches — "most recent" is unambiguous, not a guess.
    pub fn latest_job_state_for_asset(
        &self,
        asset_id: Uuid,
        kind: JobKind,
    ) -> Result<Option<(String, Option<String>)>, StorageError> {
        self.connection
            .query_row(
                "SELECT state, error FROM background_jobs
                 WHERE asset_id = ?1 AND kind = ?2
                 ORDER BY updated_at DESC LIMIT 1",
                params![asset_id.to_string(), job_kind_to_db(&kind)],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?)),
            )
            .optional()
            .map_err(StorageError::from)
    }

    /// Completes whatever job of `kind` is outstanding for `asset_id` —
    /// `'pending'` (the analyze-also-fills-the-waveform-cache path in
    /// `save_audio_analysis_outcome`, where the WaveformGeneration job was
    /// never claimed at all since `claim_pending_waveform_jobs` skips an
    /// asset with an in-flight AudioAnalysis job) or `'processing'` (the
    /// standalone waveform-job path in `process_one_waveform_job`, where
    /// the job was already claimed before the decode/build ran). Matching
    /// only `'pending'` here used to mean the standalone path's own job
    /// was *never* actually completed by this call — it sat at
    /// `'processing'` until `reset_stuck_processing_jobs`'s 3-minute
    /// timeout flipped it back to `'pending'`, which made it claimable
    /// (and decoded, and rebuilt) all over again, forever, for an asset
    /// whose waveform had already been generated correctly the first time.
    pub fn complete_pending_jobs_for_asset(
        &self,
        asset_id: Uuid,
        kind: JobKind,
    ) -> Result<usize, StorageError> {
        let updated = self.connection.execute(
            "UPDATE background_jobs SET state = 'completed', updated_at = ?1
             WHERE asset_id = ?2 AND kind = ?3 AND state IN ('pending', 'processing')",
            params![
                Utc::now().to_rfc3339(),
                asset_id.to_string(),
                job_kind_to_db(&kind),
            ],
        )?;
        Ok(updated)
    }

    pub fn pending_jobs_of_kind(
        &self,
        kind: JobKind,
        limit: usize,
    ) -> Result<Vec<JobRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, asset_id, kind, priority
             FROM background_jobs
             WHERE state = 'pending' AND kind = ?1
             ORDER BY priority ASC, created_at ASC
             LIMIT ?2",
        )?;

        let jobs = statement
            .query_map(params![job_kind_to_db(&kind), limit as i64], |row| {
                Ok(JobRecord {
                    id: parse_uuid(row.get::<_, String>(0)?),
                    asset_id: parse_uuid(row.get::<_, String>(1)?),
                    kind: job_kind_from_db(&row.get::<_, String>(2)?),
                    priority: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(jobs)
    }

    /// Atomically selects up to `limit` pending jobs of `kind` and marks them
    /// `'processing'` in one statement, so two overlapping callers (e.g. a
    /// standing background worker and a frontend-triggered drain) can never
    /// both claim and process the same job — unlike `pending_jobs_of_kind`,
    /// which is a plain read with no such guarantee.
    pub fn claim_pending_jobs(
        &self,
        kind: JobKind,
        limit: usize,
    ) -> Result<Vec<JobRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "UPDATE background_jobs SET state = 'processing', updated_at = ?1
             WHERE id IN (
               SELECT id FROM background_jobs WHERE state = 'pending' AND kind = ?2
               ORDER BY priority ASC, created_at ASC LIMIT ?3
             )
             RETURNING id, asset_id, kind, priority",
        )?;

        let jobs = statement
            .query_map(
                params![Utc::now().to_rfc3339(), job_kind_to_db(&kind), limit as i64],
                |row| {
                    Ok(JobRecord {
                        id: parse_uuid(row.get::<_, String>(0)?),
                        asset_id: parse_uuid(row.get::<_, String>(1)?),
                        kind: job_kind_from_db(&row.get::<_, String>(2)?),
                        priority: row.get(3)?,
                    })
                },
            )?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(jobs)
    }

    /// Same contract as `claim_pending_jobs(WaveformGeneration, limit)`, but
    /// skips any asset that also has a pending or `processing`
    /// `AudioAnalysis` job. `analyze_asset_audio` decodes the file once and
    /// fills the waveform cache as a side effect (see
    /// `complete_pending_jobs_for_asset`'s caller, `persist_waveform_payload`),
    /// so claiming a WaveformGeneration job ahead of that asset's
    /// AudioAnalysis job just means decoding the same file a second time —
    /// the analysis pass completes this job on its own once it gets there.
    /// A bulk backfill that queues both kinds together for the whole library
    /// (the common case after any analysis-schema change) used to pay for
    /// every file's decode twice; this only actually claims a waveform job
    /// for the case the kind was designed for — an asset whose analysis is
    /// already done (or was never queued), so nothing else is ever going to
    /// fill its waveform cache for it.
    pub fn claim_pending_waveform_jobs(&self, limit: usize) -> Result<Vec<JobRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "UPDATE background_jobs SET state = 'processing', updated_at = ?1
             WHERE id IN (
               SELECT b.id FROM background_jobs b
               WHERE b.state = 'pending' AND b.kind = 'waveform_generation'
                 AND NOT EXISTS (
                   SELECT 1 FROM background_jobs a
                   WHERE a.asset_id = b.asset_id
                     AND a.kind = 'audio_analysis'
                     AND a.state IN ('pending', 'processing')
                 )
               ORDER BY b.priority ASC, b.created_at ASC
               LIMIT ?2
             )
             RETURNING id, asset_id, kind, priority",
        )?;

        let jobs = statement
            .query_map(params![Utc::now().to_rfc3339(), limit as i64], |row| {
                Ok(JobRecord {
                    id: parse_uuid(row.get::<_, String>(0)?),
                    asset_id: parse_uuid(row.get::<_, String>(1)?),
                    kind: job_kind_from_db(&row.get::<_, String>(2)?),
                    priority: row.get(3)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(jobs)
    }

    /// Resets any job of `kind` left in `'processing'` for at least
    /// `STUCK_PROCESSING_THRESHOLD` back to `'pending'` — `claim_pending_jobs`
    /// marks a job `'processing'` before doing the real work, so a job whose
    /// claiming call never reaches `complete_job` or `fail_job` (a cancelled
    /// batch, a dev-rebuild restart, a crash) is permanently stuck:
    /// `claim_pending_jobs` only ever selects `'pending'` rows, and
    /// `requeue_failed_jobs` only ever selects `'failed'` ones, so nothing
    /// else would ever pick it back up. Meant to run at the start of a claim
    /// cycle, before the next `claim_pending_jobs` for the same kind.
    ///
    /// The age floor matters: this used to reset *every* `'processing'` row
    /// unconditionally, on every call. Each `process_*_jobs` command already
    /// awaits its whole claimed batch before returning, so in the ordinary
    /// case there's nothing of its own left in `'processing'` to catch — but
    /// a single slow file (a large decode under heavy concurrent load, which
    /// this app has seen run to several minutes) sitting right at the
    /// boundary between one claim cycle and the next got its row yanked back
    /// to `'pending'` and immediately re-claimed by the very next cycle,
    /// before the original attempt had a chance to finish and record
    /// completion. Since a reset keeps the job's original `created_at`, it
    /// sorts right back to the front of `claim_pending_jobs`'s queue too —
    /// so the same handful of slow jobs could get interrupted and restarted
    /// indefinitely without ever completing, which is exactly the "stuck
    /// instrument detection" symptom this was chasing. Giving a claim a real
    /// window to finish before treating it as abandoned fixes that; a
    /// genuinely orphaned claim (dead process, crash) still gets reclaimed,
    /// just on the next cycle at or past the threshold rather than instantly.
    pub fn reset_stuck_processing_jobs(&self, kind: JobKind) -> Result<usize, StorageError> {
        const STUCK_PROCESSING_THRESHOLD: chrono::Duration = chrono::Duration::minutes(3);
        let cutoff = (Utc::now() - STUCK_PROCESSING_THRESHOLD).to_rfc3339();
        let changed = self.connection.execute(
            "UPDATE background_jobs SET state = 'pending', updated_at = ?1
             WHERE state = 'processing' AND kind = ?2 AND updated_at < ?3",
            params![Utc::now().to_rfc3339(), job_kind_to_db(&kind), cutoff],
        )?;
        Ok(changed)
    }

    /// Requeues jobs abandoned after a failure, up to `max_attempts` tries
    /// total. Run periodically by the standing background worker rather than
    /// immediately on failure, so a transient error (e.g. a momentarily
    /// unreachable NAS path) gets a real gap before retrying.
    pub fn requeue_failed_jobs(&self, max_attempts: i64) -> Result<usize, StorageError> {
        let changed = self.connection.execute(
            "UPDATE background_jobs SET state = 'pending', updated_at = ?1
             WHERE state = 'failed' AND attempts < ?2",
            params![Utc::now().to_rfc3339(), max_attempts],
        )?;

        Ok(changed)
    }

    /// Explicit, user-initiated retry for one library + kind — unlike
    /// `requeue_failed_jobs`, ignores the attempt cap entirely (a fix that
    /// resolves the actual root cause, like the 24-bit WAV decode bug this
    /// was built for, makes jobs that already exhausted their 3 automatic
    /// attempts worth retrying again; also resets attempts back to 0 so
    /// they get the same fair shot at automatic retries afterward).
    pub fn retry_failed_jobs_for_library(
        &self,
        library_id: Uuid,
        kind: JobKind,
    ) -> Result<usize, StorageError> {
        let changed = self.connection.execute(
            "UPDATE background_jobs SET state = 'pending', attempts = 0, updated_at = ?1
             WHERE state = 'failed' AND kind = ?2
               AND asset_id IN (SELECT id FROM assets WHERE library_id = ?3)",
            params![Utc::now().to_rfc3339(), job_kind_to_db(&kind), library_id.to_string()],
        )?;

        Ok(changed)
    }

    /// User-initiated "re-run this analysis" (the Sonic Radar sync button).
    /// For every non-trashed asset in `library_id`, drops any of its
    /// `kind` jobs that isn't already mid-flight and queues one fresh
    /// pending job, so re-analysis picks up assets whose original job long
    /// since completed (the backfill path ADRs 0029-0031 left open).
    /// Returns the number of jobs queued.
    pub fn requeue_analysis_for_library(
        &self,
        library_id: Uuid,
        kind: JobKind,
        priority: i64,
    ) -> Result<usize, StorageError> {
        let kind_db = job_kind_to_db(&kind);
        self.connection.execute(
            "DELETE FROM background_jobs
             WHERE kind = ?1 AND state != 'processing'
               AND asset_id IN (
                 SELECT id FROM assets WHERE library_id = ?2
                   AND NOT EXISTS (
                     SELECT 1 FROM trash_items t
                     WHERE t.asset_id = assets.id AND t.state = 'in_trash'
                   )
               )",
            params![kind_db, library_id.to_string()],
        )?;
        let now = Utc::now().to_rfc3339();
        let queued = self.connection.execute(
            "INSERT INTO background_jobs (id, asset_id, kind, priority, state, attempts, created_at, updated_at)
             SELECT lower(hex(randomblob(16))), a.id, ?1, ?2, 'pending', 0, ?3, ?3
             FROM assets a
             WHERE a.library_id = ?4
               AND NOT EXISTS (
                 SELECT 1 FROM trash_items t WHERE t.asset_id = a.id AND t.state = 'in_trash'
               )
               AND NOT EXISTS (
                 SELECT 1 FROM background_jobs j
                 WHERE j.asset_id = a.id AND j.kind = ?1 AND j.state = 'processing'
               )",
            params![kind_db, priority, now, library_id.to_string()],
        )?;
        Ok(queued)
    }

    /// Same as `requeue_analysis_for_library` but scoped to an explicit set
    /// of assets (the Sonic Radar sync button with a selection). Two bulk
    /// statements regardless of selection size — not a per-asset loop.
    pub fn requeue_analysis_for_assets(
        &self,
        asset_ids: &[Uuid],
        kind: JobKind,
        priority: i64,
    ) -> Result<usize, StorageError> {
        if asset_ids.is_empty() {
            return Ok(0);
        }
        let kind_db = job_kind_to_db(&kind);
        let now = Utc::now().to_rfc3339();
        let placeholders = vec!["?"; asset_ids.len()].join(", ");
        let ids: Vec<String> = asset_ids.iter().map(|id| id.to_string()).collect();

        let delete_sql = format!(
            "DELETE FROM background_jobs
             WHERE kind = ?1 AND state != 'processing' AND asset_id IN ({placeholders})"
        );
        let mut delete_params: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(kind_db.to_string())];
        delete_params.extend(ids.iter().map(|id| Box::new(id.clone()) as Box<dyn rusqlite::ToSql>));
        self.connection.execute(
            &delete_sql,
            params_from_iter(delete_params.iter().map(|p| p.as_ref())),
        )?;

        let insert_sql = format!(
            "INSERT INTO background_jobs (id, asset_id, kind, priority, state, attempts, created_at, updated_at)
             SELECT lower(hex(randomblob(16))), a.id, ?1, ?2, 'pending', 0, ?3, ?3
             FROM assets a
             WHERE a.id IN ({placeholders})
               AND NOT EXISTS (
                 SELECT 1 FROM background_jobs j
                 WHERE j.asset_id = a.id AND j.kind = ?1 AND j.state = 'processing'
               )"
        );
        let mut insert_params: Vec<Box<dyn rusqlite::ToSql>> = vec![
            Box::new(kind_db.to_string()),
            Box::new(priority),
            Box::new(now),
        ];
        insert_params.extend(ids.iter().map(|id| Box::new(id.clone()) as Box<dyn rusqlite::ToSql>));
        let queued = self.connection.execute(
            &insert_sql,
            params_from_iter(insert_params.iter().map(|p| p.as_ref())),
        )?;
        Ok(queued)
    }

    pub fn set_embedded_metadata(
        &self,
        asset_id: Uuid,
        title: Option<String>,
        genre: Option<String>,
        comment: Option<String>,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE assets SET embedded_title = ?1, embedded_genre = ?2, embedded_comment = ?3 WHERE id = ?4",
            params![title, genre, comment, asset_id.to_string()],
        )?;
        Ok(())
    }

    /// Sets just the perceptual fingerprint, leaving every other analysis
    /// field untouched — for the similarity-worker's fingerprint, which
    /// now finishes on its own detached timeline well after
    /// `set_audio_analysis` already completed the job (see
    /// `analyze_asset_audio`'s doc comment for why): a full
    /// `set_audio_analysis` call here would blindly overwrite tempo/key/
    /// pitch/vocal-ratio with whatever this call happened to be given,
    /// which for a detached late-arriving write is nothing meaningful.
    pub fn set_perceptual_fingerprint(
        &self,
        asset_id: Uuid,
        fingerprint: Option<String>,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE assets SET perceptual_fingerprint = ?1 WHERE id = ?2",
            params![fingerprint, asset_id.to_string()],
        )?;
        Ok(())
    }

    pub fn set_audio_analysis(
        &self,
        asset_id: Uuid,
        update: AudioAnalysisUpdate,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE assets SET
                duration_ms = ?1, sample_rate = ?2, bit_depth = ?3, channels = ?4,
                loudness_lufs = ?5, peak_db = ?6, bpm = ?7, bpm_confidence = ?8,
                musical_key = ?9, key_confidence = ?10, perceptual_fingerprint = ?11,
                detected_key = ?13, key_strength = ?14
             WHERE id = ?12",
            params![
                update.duration_ms,
                update.sample_rate,
                update.bit_depth,
                update.channels,
                update.loudness_lufs,
                update.peak_db,
                update.bpm,
                update.bpm_confidence,
                update.musical_key,
                update.key_confidence,
                update.perceptual_fingerprint,
                asset_id.to_string(),
                update.detected_key,
                update.key_strength,
            ],
        )?;
        Ok(())
    }

    /// Persists (or replaces) the waveform peak payload for an asset and
    /// bumps `assets.waveform_version` so the stored row's own
    /// `waveform_version` matches. Called by the `WaveformGeneration` job
    /// after it decodes the source once, and by the desktop shell's
    /// client-side fallback so even that path only ever pays the decode
    /// cost once.
    pub fn set_waveform_cache(
        &self,
        asset_id: Uuid,
        sample_rate: i64,
        payload: &str,
    ) -> Result<(), StorageError> {
        let now = Utc::now().to_rfc3339();
        self.connection.execute(
            "UPDATE assets SET waveform_version = waveform_version + 1 WHERE id = ?1",
            params![asset_id.to_string()],
        )?;
        // Asset deleted between decode and persist (a fast trash while a
        // job or the client fallback was in flight): nothing to cache, and
        // the FK on waveform_peaks would reject the insert anyway.
        let Some(version) = self
            .connection
            .query_row(
                "SELECT waveform_version FROM assets WHERE id = ?1",
                params![asset_id.to_string()],
                |row| row.get::<_, i64>(0),
            )
            .optional()?
        else {
            return Ok(());
        };
        self.connection.execute(
            "INSERT INTO waveform_peaks (asset_id, waveform_version, sample_rate, payload, generated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(asset_id) DO UPDATE SET
               waveform_version = excluded.waveform_version,
               sample_rate = excluded.sample_rate,
               payload = excluded.payload,
               generated_at = excluded.generated_at",
            params![
                asset_id.to_string(),
                version,
                sample_rate,
                payload,
                now,
            ],
        )?;
        Ok(())
    }

    /// Returns the cached waveform payload for an asset, or `None` if it has
    /// never been generated (or was invalidated by `clear_waveform_cache`).
    pub fn get_waveform_cache(
        &self,
        asset_id: Uuid,
    ) -> Result<Option<WaveformCacheRow>, StorageError> {
        self.connection
            .query_row(
                "SELECT sample_rate, payload, waveform_version
                 FROM waveform_peaks WHERE asset_id = ?1",
                params![asset_id.to_string()],
                |row| {
                    Ok(WaveformCacheRow {
                        sample_rate: row.get(0)?,
                        payload: row.get(1)?,
                        waveform_version: row.get(2)?,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    }

    /// Drops an asset's cached waveform payload — used when the underlying
    /// file changes out from under the catalog (a relink), so the next
    /// preview regenerates it from the new source rather than showing the
    /// old file's shape.
    pub fn clear_waveform_cache(&self, asset_id: Uuid) -> Result<(), StorageError> {
        self.connection.execute(
            "DELETE FROM waveform_peaks WHERE asset_id = ?1",
            params![asset_id.to_string()],
        )?;
        Ok(())
    }

    /// Bulk-assigns a stem group: every `(asset_id, label, is_primary)` in
    /// `members` gets the same `group_id`. Exactly one member should have
    /// `is_primary = true` — the row shown in the main asset list; callers
    /// (see `import_pipeline::stems::detect_stem_groups`) already
    /// guarantee that, this just persists whatever it's given. Wrapped in
    /// a transaction for the same reason `set_asset_instruments` is: a
    /// mid-loop failure shouldn't leave a group half-assigned.
    pub fn set_stem_group(
        &self,
        group_id: Uuid,
        members: &[(Uuid, String, bool)],
    ) -> Result<(), StorageError> {
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> Result<(), StorageError> {
            for (asset_id, label, is_primary) in members {
                self.connection.execute(
                    "UPDATE assets SET stem_group_id = ?1, stem_label = ?2, stem_is_primary = ?3 WHERE id = ?4",
                    params![group_id.to_string(), label, *is_primary as i64, asset_id.to_string()],
                )?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.connection.execute_batch("COMMIT")?;
                Ok(())
            }
            Err(error) => {
                let _ = self.connection.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    /// Removes `asset_id` from whatever stem group it's in (a no-op if it
    /// isn't in one). Doesn't touch the other members — if that leaves the
    /// group with fewer than 2, it's still technically "a group" in the
    /// DB, just with nothing interesting for the UI to expand.
    pub fn clear_stem_group_for_asset(&self, asset_id: Uuid) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE assets SET stem_group_id = NULL, stem_label = NULL, stem_is_primary = 0 WHERE id = ?1",
            params![asset_id.to_string()],
        )?;
        Ok(())
    }

    /// Every member of one stem group (the primary/full-mix row first,
    /// then the rest alphabetically by label) — what the inspector's Stems
    /// panel shows for a selected asset.
    pub fn stem_group_members(&self, group_id: Uuid) -> Result<Vec<AssetRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, library_id, original_filename, display_name, relative_path, referenced_path,
                storage_mode, content_hash, media_type, file_size, availability_state, review_state, favorite,
                embedded_title, embedded_genre, embedded_comment,
                duration_ms, sample_rate, bit_depth, channels, loudness_lufs, peak_db,
                bpm, bpm_confidence, musical_key, key_confidence, vocal_ratio, detected_key, key_strength,
                stem_group_id, stem_label, stem_is_primary
             FROM assets
             WHERE stem_group_id = ?1
               AND NOT EXISTS (
                 SELECT 1 FROM trash_items WHERE trash_items.asset_id = assets.id AND trash_items.state = 'in_trash'
               )
             ORDER BY stem_is_primary DESC, stem_label ASC",
        )?;
        let members = statement
            .query_map(params![group_id.to_string()], asset_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(members)
    }

    /// `(asset id, original filename)` for every non-trashed, not-already-
    /// grouped asset in a library — the input a retroactive whole-library
    /// stem-detection pass scans (see the `detect_stem_groups` Tauri
    /// command). A freshly-imported batch passes its own file list
    /// directly instead of calling this.
    pub fn asset_filenames_for_library(&self, library_id: Uuid) -> Result<Vec<(Uuid, String)>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, original_filename FROM assets
             WHERE library_id = ?1 AND stem_group_id IS NULL
               AND NOT EXISTS (
                 SELECT 1 FROM trash_items WHERE trash_items.asset_id = assets.id AND trash_items.state = 'in_trash'
               )",
        )?;
        let rows = statement
            .query_map(params![library_id.to_string()], |row| {
                Ok((parse_uuid(row.get::<_, String>(0)?), row.get::<_, String>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Replaces an asset's detected-instrument list (the output of the
    /// `InstrumentDetection` job). An empty slice clears it — a valid
    /// result meaning "the model found nothing", distinct from "not yet
    /// analysed" (no rows and a still-pending job).
    pub fn set_asset_instruments(
        &self,
        asset_id: Uuid,
        instruments: &[(String, f64)],
    ) -> Result<(), StorageError> {
        // Wrapped in a transaction so a mid-loop failure (a disk error, a
        // lock timeout) can't leave the delete committed but only some of
        // the new rows inserted — a real, if rare, way this asset's
        // instrument set could otherwise end up silently truncated rather
        // than either the old set or the new one. `Catalog`'s methods take
        // `&self`, not `&mut self`, so this is raw BEGIN/COMMIT rather than
        // rusqlite's `Connection::transaction` (which needs `&mut`) — same
        // pattern already used for the bulk test fixture insert elsewhere
        // in this file.
        self.connection.execute_batch("BEGIN IMMEDIATE")?;
        let result = (|| -> Result<(), StorageError> {
            self.connection.execute(
                "DELETE FROM asset_instruments WHERE asset_id = ?1",
                params![asset_id.to_string()],
            )?;
            for (instrument, confidence) in instruments {
                self.connection.execute(
                    "INSERT OR REPLACE INTO asset_instruments (asset_id, instrument, confidence)
                     VALUES (?1, ?2, ?3)",
                    params![asset_id.to_string(), instrument, confidence],
                )?;
            }
            Ok(())
        })();
        match result {
            Ok(()) => {
                self.connection.execute_batch("COMMIT")?;
                Ok(())
            }
            Err(error) => {
                // Best-effort — the original error is what's reported
                // either way, this just avoids leaving the connection with
                // an open transaction that would poison every later query.
                let _ = self.connection.execute_batch("ROLLBACK");
                Err(error)
            }
        }
    }

    /// One asset's detected instruments, strongest first.
    pub fn instruments_for_asset(
        &self,
        asset_id: Uuid,
    ) -> Result<Vec<(String, f64)>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT instrument, confidence FROM asset_instruments
             WHERE asset_id = ?1 ORDER BY confidence DESC, instrument ASC",
        )?;
        let rows = statement
            .query_map(params![asset_id.to_string()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Every distinct instrument present in a library with a count of
    /// non-trashed assets that have it — the facet for the instrument page.
    /// Ordered by count desc so the common instruments lead.
    pub fn instrument_counts_for_library(
        &self,
        library_id: Uuid,
    ) -> Result<Vec<(String, i64)>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT ai.instrument, COUNT(*) AS n
             FROM asset_instruments ai
             JOIN assets a ON a.id = ai.asset_id
             WHERE a.library_id = ?1
               AND NOT EXISTS (
                 SELECT 1 FROM trash_items t
                 WHERE t.asset_id = a.id AND t.state = 'in_trash'
               )
             GROUP BY ai.instrument
             ORDER BY n DESC, ai.instrument ASC",
        )?;
        let rows = statement
            .query_map(params![library_id.to_string()], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Non-trashed assets in a library that have ANY of `instruments`
    /// (OR match). Empty `instruments` returns nothing.
    pub fn assets_with_any_instrument(
        &self,
        library_id: Uuid,
        instruments: &[String],
    ) -> Result<Vec<AssetRecord>, StorageError> {
        if instruments.is_empty() {
            return Ok(Vec::new());
        }
        let placeholders = vec!["?"; instruments.len()].join(", ");
        let sql = format!(
            "SELECT DISTINCT assets.id, assets.library_id, assets.original_filename, assets.display_name,
                assets.relative_path, assets.referenced_path, assets.storage_mode, assets.content_hash,
                assets.media_type, assets.file_size, assets.availability_state, assets.review_state, assets.favorite,
                assets.embedded_title, assets.embedded_genre, assets.embedded_comment,
                assets.duration_ms, assets.sample_rate, assets.bit_depth, assets.channels,
                assets.loudness_lufs, assets.peak_db, assets.bpm, assets.bpm_confidence,
                assets.musical_key, assets.key_confidence, assets.vocal_ratio, assets.detected_key, assets.key_strength,
                assets.stem_group_id, assets.stem_label, assets.stem_is_primary
             FROM assets
             JOIN asset_instruments ai ON ai.asset_id = assets.id
             WHERE assets.library_id = ?1
               AND ai.instrument IN ({placeholders})
               AND NOT EXISTS (
                 SELECT 1 FROM trash_items t
                 WHERE t.asset_id = assets.id AND t.state = 'in_trash'
               )
             ORDER BY assets.date_added ASC"
        );
        let mut statement = self.connection.prepare(&sql)?;
        let mut params: Vec<Box<dyn rusqlite::ToSql>> = Vec::with_capacity(instruments.len() + 1);
        params.push(Box::new(library_id.to_string()));
        for instrument in instruments {
            params.push(Box::new(instrument.clone()));
        }
        let assets = statement
            .query_map(params_from_iter(params.iter().map(|p| p.as_ref())), asset_from_row)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(assets)
    }

    /// (asset_id, perceptual_fingerprint JSON) pairs for every non-trashed
    /// asset in the library that has been analyzed. Used by similarity
    /// search — brute-force Euclidean distance over these, no vector index
    /// needed at desktop-library scale.
    pub fn perceptual_fingerprints(
        &self,
        library_id: Uuid,
    ) -> Result<Vec<(Uuid, String)>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, perceptual_fingerprint FROM assets
             WHERE library_id = ?1 AND perceptual_fingerprint IS NOT NULL
               AND NOT EXISTS (
                 SELECT 1 FROM trash_items
                 WHERE trash_items.asset_id = assets.id AND trash_items.state = 'in_trash'
               )",
        )?;
        let rows = statement
            .query_map(params![library_id.to_string()], |row| {
                Ok((
                    parse_uuid(row.get::<_, String>(0)?),
                    row.get::<_, String>(1)?,
                ))
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    pub fn seed_starter_taxonomy(&self) -> Result<(), StorageError> {
        for (name, facet) in [
            ("Music", "media_type"),
            ("Sound Effect", "media_type"),
            ("Ambience", "media_type"),
            ("Foley", "media_type"),
            ("Voice / Dialogue", "media_type"),
            ("Impact", "action"),
            ("Whoosh", "action"),
            ("Rise", "action"),
            ("Metal", "source"),
            ("Glass", "source"),
            ("Cinematic", "character"),
            ("Subtle", "energy"),
            ("High", "energy"),
        ] {
            self.create_tag(name, facet, true)?;
        }

        Ok(())
    }

    pub fn create_tag(
        &self,
        name: impl AsRef<str>,
        facet: impl AsRef<str>,
        is_system: bool,
    ) -> Result<TagRecord, StorageError> {
        let normalized_name = normalize_term(name.as_ref());
        let now = Utc::now().to_rfc3339();

        self.connection.execute(
            "INSERT OR IGNORE INTO tags (id, name, normalized_name, facet, is_system, is_hidden, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, ?6)",
            params![
                Uuid::new_v4().to_string(),
                name.as_ref(),
                normalized_name,
                facet.as_ref(),
                is_system as i64,
                now,
            ],
        )?;

        self.connection
            .query_row(
                "SELECT id, name, normalized_name, facet, is_system FROM tags WHERE normalized_name = ?1",
                params![normalize_term(name.as_ref())],
                tag_from_row,
            )
            .map_err(StorageError::from)
    }

    pub fn list_tags(&self) -> Result<Vec<TagRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, name, normalized_name, facet, is_system FROM tags ORDER BY facet, name",
        )?;

        let tags = statement
            .query_map([], tag_from_row)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)?;

        Ok(tags)
    }

    pub fn apply_tag_to_assets(
        &self,
        asset_ids: &[Uuid],
        tag_id: Uuid,
        origin: TagOrigin,
    ) -> Result<Uuid, StorageError> {
        let now = Utc::now().to_rfc3339();
        for asset_id in asset_ids {
            self.connection.execute(
                "INSERT OR IGNORE INTO asset_tags (asset_id, tag_id, origin, confidence, approval_state, created_at)
                 VALUES (?1, ?2, ?3, 1.0, 'accepted', ?4)",
                params![
                    asset_id.to_string(),
                    tag_id.to_string(),
                    tag_origin_to_db(origin),
                    now,
                ],
            )?;
        }

        self.record_undo(
            "remove_asset_tags",
            &format!(
                "{}|{}|{}",
                tag_id,
                tag_origin_to_db(origin),
                join_uuids(asset_ids)
            ),
        )
    }

    pub fn remove_tag_from_asset(
        &self,
        asset_id: Uuid,
        tag_id: Uuid,
    ) -> Result<Uuid, StorageError> {
        let origin: String = self.connection.query_row(
            "SELECT origin FROM asset_tags WHERE asset_id = ?1 AND tag_id = ?2",
            params![asset_id.to_string(), tag_id.to_string()],
            |row| row.get(0),
        )?;

        self.connection.execute(
            "DELETE FROM asset_tags WHERE asset_id = ?1 AND tag_id = ?2",
            params![asset_id.to_string(), tag_id.to_string()],
        )?;

        self.record_undo(
            "readd_asset_tags",
            &format!("{}|{}|{}", tag_id, origin, join_uuids(&[asset_id])),
        )
    }

    pub fn tags_for_asset(&self, asset_id: Uuid) -> Result<Vec<TagRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT tags.id, tags.name, tags.normalized_name, tags.facet, tags.is_system
             FROM tags
             INNER JOIN asset_tags ON asset_tags.tag_id = tags.id
             WHERE asset_tags.asset_id = ?1
             ORDER BY tags.facet, tags.name",
        )?;

        let tags = statement
            .query_map(params![asset_id.to_string()], tag_from_row)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)?;

        Ok(tags)
    }

    pub fn create_collection(
        &self,
        library_id: Uuid,
        name: impl AsRef<str>,
        collection_type: CollectionType,
    ) -> Result<CollectionRecord, StorageError> {
        let collection = CollectionRecord {
            id: Uuid::new_v4(),
            library_id,
            name: name.as_ref().to_string(),
            collection_type,
            query_definition: None,
            export_path: None,
            sfx_export_path: None,
        };
        let now = Utc::now().to_rfc3339();

        self.connection.execute(
            "INSERT INTO collections (id, library_id, name, type, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                collection.id.to_string(),
                collection.library_id.to_string(),
                collection.name,
                collection_type_to_db(collection.collection_type),
                now,
            ],
        )?;

        Ok(collection)
    }

    pub fn create_smart_collection(
        &self,
        library_id: Uuid,
        name: impl AsRef<str>,
        query: &AssetSearchQuery,
    ) -> Result<CollectionRecord, StorageError> {
        let collection = CollectionRecord {
            id: Uuid::new_v4(),
            library_id,
            name: name.as_ref().to_string(),
            collection_type: CollectionType::Smart,
            query_definition: Some(serde_json::to_string(query).expect("query serializes")),
            export_path: None,
            sfx_export_path: None,
        };
        let now = Utc::now().to_rfc3339();

        self.connection.execute(
            "INSERT INTO collections (id, library_id, name, type, query_definition, created_at)
             VALUES (?1, ?2, ?3, 'smart', ?4, ?5)",
            params![
                collection.id.to_string(),
                collection.library_id.to_string(),
                collection.name,
                collection.query_definition,
                now,
            ],
        )?;

        Ok(collection)
    }

    pub fn get_collection(
        &self,
        collection_id: Uuid,
    ) -> Result<Option<CollectionRecord>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, library_id, name, type, query_definition, export_path, sfx_export_path FROM collections WHERE id = ?1",
                params![collection_id.to_string()],
                collection_from_row,
            )
            .optional()
            .map_err(StorageError::from)
    }

    /// Sets (or clears, with `None`) the folder a project's sounds are
    /// quick-exported to for the DR button. `Some("")` is treated the same
    /// as `None` so clearing the field from a text input works naturally.
    pub fn set_collection_export_path(
        &self,
        collection_id: Uuid,
        export_path: Option<&str>,
    ) -> Result<(), StorageError> {
        let export_path = export_path.filter(|path| !path.trim().is_empty());
        self.connection.execute(
            "UPDATE collections SET export_path = ?1 WHERE id = ?2",
            params![export_path, collection_id.to_string()],
        )?;
        Ok(())
    }

    /// Sets (or clears, with `None`) the folder a project's non-music
    /// sounds are quick-exported to for the "send to project" button. Same
    /// empty-string-clears-the-field behavior as `set_collection_export_path`.
    pub fn set_collection_sfx_export_path(
        &self,
        collection_id: Uuid,
        sfx_export_path: Option<&str>,
    ) -> Result<(), StorageError> {
        let sfx_export_path = sfx_export_path.filter(|path| !path.trim().is_empty());
        self.connection.execute(
            "UPDATE collections SET sfx_export_path = ?1 WHERE id = ?2",
            params![sfx_export_path, collection_id.to_string()],
        )?;
        Ok(())
    }

    /// Adds a categorized export folder to a project — the advanced
    /// counterpart to `export_path`/`sfx_export_path`. `role` uses the same
    /// vocabulary a library `folders` row does (music, sound_effect,
    /// voiceover, foley, ambience, documents); `export_asset_to_project`
    /// checks these before falling back to the two built-in slots.
    pub fn add_project_export_folder(
        &self,
        collection_id: Uuid,
        path: impl AsRef<str>,
        role: impl AsRef<str>,
    ) -> Result<ProjectExportFolderRecord, StorageError> {
        let folder = ProjectExportFolderRecord {
            id: Uuid::new_v4(),
            collection_id,
            path: path.as_ref().to_string(),
            role: role.as_ref().to_string(),
            created_at: Utc::now().to_rfc3339(),
        };
        self.connection.execute(
            "INSERT INTO project_export_folders (id, collection_id, path, role, created_at) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                folder.id.to_string(),
                folder.collection_id.to_string(),
                folder.path,
                folder.role,
                folder.created_at
            ],
        )?;
        Ok(folder)
    }

    pub fn list_project_export_folders(
        &self,
        collection_id: Uuid,
    ) -> Result<Vec<ProjectExportFolderRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, collection_id, path, role, created_at
             FROM project_export_folders WHERE collection_id = ?1 ORDER BY created_at ASC",
        )?;
        let folders = statement
            .query_map(params![collection_id.to_string()], |row| {
                Ok(ProjectExportFolderRecord {
                    id: parse_uuid(row.get::<_, String>(0)?),
                    collection_id: parse_uuid(row.get::<_, String>(1)?),
                    path: row.get(2)?,
                    role: row.get(3)?,
                    created_at: row.get(4)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(folders)
    }

    pub fn remove_project_export_folder(&self, folder_id: Uuid) -> Result<(), StorageError> {
        self.connection.execute(
            "DELETE FROM project_export_folders WHERE id = ?1",
            params![folder_id.to_string()],
        )?;
        Ok(())
    }

    pub fn set_project_export_folder_role(
        &self,
        folder_id: Uuid,
        role: impl AsRef<str>,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE project_export_folders SET role = ?1 WHERE id = ?2",
            params![role.as_ref(), folder_id.to_string()],
        )?;
        Ok(())
    }

    /// Renames a project (or any collection) — the hover edit icon on the
    /// sidebar's Projects list. Trims and refuses a blank name rather than
    /// silently writing empty string, same intent as create_collection's
    /// own name handling.
    pub fn rename_collection(&self, collection_id: Uuid, name: &str) -> Result<(), StorageError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(StorageError::InvalidInput("project name cannot be empty".to_string()));
        }
        self.connection.execute(
            "UPDATE collections SET name = ?1 WHERE id = ?2",
            params![name, collection_id.to_string()],
        )?;
        Ok(())
    }

    pub fn pending_job_count(&self, kind: JobKind) -> Result<usize, StorageError> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM background_jobs WHERE kind = ?1 AND state = 'pending'",
            params![job_kind_to_db(&kind)],
            |row| row.get(0),
        )?;

        Ok(count as usize)
    }

    /// Like `pending_job_count`, but scoped to one library — for a per-library
    /// job-status display where multiple libraries may have pending work of
    /// the same kind.
    pub fn pending_job_count_for_library(
        &self,
        library_id: Uuid,
        kind: JobKind,
    ) -> Result<usize, StorageError> {
        let count: i64 = self.connection.query_row(
            "SELECT COUNT(*) FROM background_jobs
             INNER JOIN assets ON assets.id = background_jobs.asset_id
             WHERE assets.library_id = ?1 AND background_jobs.kind = ?2 AND background_jobs.state = 'pending'",
            params![library_id.to_string(), job_kind_to_db(&kind)],
            |row| row.get(0),
        )?;

        Ok(count as usize)
    }

    /// pending/failed/completed counts for one library + kind in a single
    /// query — used for the Background Activity panel, which needs all
    /// three (pending to know whether to start a drain, failed/completed to
    /// report a real "what happened" summary once one finishes, not just
    /// silently clear the progress bar).
    pub fn job_state_counts_for_library(
        &self,
        library_id: Uuid,
        kind: JobKind,
    ) -> Result<JobStateCounts, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT background_jobs.state, COUNT(*) FROM background_jobs
             INNER JOIN assets ON assets.id = background_jobs.asset_id
             WHERE assets.library_id = ?1 AND background_jobs.kind = ?2
             GROUP BY background_jobs.state",
        )?;
        let rows = statement
            .query_map(params![library_id.to_string(), job_kind_to_db(&kind)], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut counts = JobStateCounts::default();
        for (state, count) in rows {
            let count = count as usize;
            match state.as_str() {
                // "processing" folds into `pending` (every consumer of this
                // struct only ever treats `pending` as "work not done yet",
                // for a progress-bar's total-minus-pending math) — NOT
                // dropped, which was a real bug: a kind whose only
                // outstanding jobs were stuck `processing` (e.g. abandoned
                // by a killed process) reported pending=0, so the frontend's
                // `startPending === 0` drain gate never called the
                // processing command again — and reset_stuck_processing_jobs
                // only ever runs at the start of that same command, so a
                // kind in this state could never self-heal. `+=` because
                // both states can be present in the same grouped result.
                "pending" | "processing" => counts.pending += count,
                "failed" => counts.failed = count,
                "completed" => counts.completed = count,
                _ => {}
            }
        }
        Ok(counts)
    }

    /// Distinct, lowercased file extensions of currently-failed jobs — lets
    /// a failure summary say "1 .aif file" instead of a generic count, so
    /// the format actually at fault is visible without anyone needing to
    /// go spelunking in the catalog database by hand.
    pub fn failed_job_extensions_for_library(
        &self,
        library_id: Uuid,
        kind: JobKind,
    ) -> Result<Vec<String>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT DISTINCT assets.original_filename FROM background_jobs
             INNER JOIN assets ON assets.id = background_jobs.asset_id
             WHERE assets.library_id = ?1 AND background_jobs.kind = ?2 AND background_jobs.state = 'failed'",
        )?;
        let filenames = statement
            .query_map(params![library_id.to_string(), job_kind_to_db(&kind)], |row| {
                row.get::<_, String>(0)
            })?
            .collect::<Result<Vec<_>, _>>()?;

        let mut extensions: Vec<String> = filenames
            .iter()
            .filter_map(|filename| {
                std::path::Path::new(filename)
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .map(|extension| format!(".{}", extension.to_ascii_lowercase()))
            })
            .collect();
        extensions.sort();
        extensions.dedup();
        Ok(extensions)
    }

    pub fn list_collections(
        &self,
        library_id: Uuid,
    ) -> Result<Vec<CollectionRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, library_id, name, type, query_definition, export_path, sfx_export_path FROM collections
             WHERE library_id = ?1 ORDER BY created_at ASC",
        )?;

        let collections = statement
            .query_map(params![library_id.to_string()], collection_from_row)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)?;

        Ok(collections)
    }

    pub fn add_assets_to_collection(
        &self,
        collection_id: Uuid,
        asset_ids: &[Uuid],
    ) -> Result<Uuid, StorageError> {
        let now = Utc::now().to_rfc3339();
        for asset_id in asset_ids {
            self.connection.execute(
                "INSERT OR IGNORE INTO collection_assets (collection_id, asset_id, created_at)
                 VALUES (?1, ?2, ?3)",
                params![collection_id.to_string(), asset_id.to_string(), now],
            )?;
        }

        self.record_undo(
            "remove_collection_assets",
            &format!("{}|{}", collection_id, join_uuids(asset_ids)),
        )
    }

    pub fn assets_in_collection(
        &self,
        collection_id: Uuid,
    ) -> Result<Vec<AssetRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT assets.id, assets.library_id, assets.original_filename, assets.display_name,
                assets.relative_path, assets.referenced_path, assets.storage_mode, assets.content_hash,
                assets.media_type, assets.file_size, assets.availability_state, assets.review_state, assets.favorite,
                assets.embedded_title, assets.embedded_genre, assets.embedded_comment,
                assets.duration_ms, assets.sample_rate, assets.bit_depth, assets.channels,
                assets.loudness_lufs, assets.peak_db, assets.bpm, assets.bpm_confidence,
                assets.musical_key, assets.key_confidence, assets.vocal_ratio, assets.detected_key, assets.key_strength,
                assets.stem_group_id, assets.stem_label, assets.stem_is_primary
             FROM assets
             INNER JOIN collection_assets ON collection_assets.asset_id = assets.id
             WHERE collection_assets.collection_id = ?1
             ORDER BY collection_assets.created_at ASC",
        )?;

        let assets = statement
            .query_map(params![collection_id.to_string()], asset_from_row)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)?;

        Ok(assets)
    }

    /// One row per (asset, project) membership across an entire library —
    /// batched like this so the browser can badge every visible row ("in a
    /// project" / "already exported") and offer a one-click per-row send
    /// without an N+1 query per asset. `exported` reflects whether a
    /// `usage_events` row already recorded that exact (asset, project) pair
    /// being exported, not just exported anywhere.
    pub fn project_memberships_for_library(
        &self,
        library_id: Uuid,
    ) -> Result<Vec<AssetProjectMembership>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT collection_assets.asset_id, collections.id, collections.name, collections.export_path,
                collections.sfx_export_path,
                EXISTS(
                    SELECT 1 FROM usage_events
                    WHERE usage_events.asset_id = collection_assets.asset_id
                      AND usage_events.project_id = collections.id
                      AND usage_events.event_type = 'exported'
                ) AS exported
             FROM collection_assets
             INNER JOIN collections ON collections.id = collection_assets.collection_id
             WHERE collections.library_id = ?1 AND collections.type = 'project'
             ORDER BY collections.created_at ASC",
        )?;

        let memberships = statement
            .query_map(params![library_id.to_string()], |row| {
                Ok(AssetProjectMembership {
                    asset_id: Uuid::parse_str(&row.get::<_, String>(0)?)
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    project_id: Uuid::parse_str(&row.get::<_, String>(1)?)
                        .map_err(|_| rusqlite::Error::InvalidQuery)?,
                    project_name: row.get(2)?,
                    export_path: row.get(3)?,
                    sfx_export_path: row.get(4)?,
                    exported: row.get(5)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)?;

        Ok(memberships)
    }

    /// The "evaluate" half `create_smart_collection` never had: loads the
    /// collection's stored query definition and re-runs it through
    /// `search_assets`, returning live results rather than a static
    /// membership list.
    pub fn assets_in_smart_collection(
        &self,
        collection_id: Uuid,
    ) -> Result<Vec<AssetRecord>, StorageError> {
        let collection = self
            .get_collection(collection_id)?
            .ok_or(StorageError::CollectionNotFound)?;

        if collection.collection_type != CollectionType::Smart {
            return Err(StorageError::NotASmartCollection);
        }

        let query_definition = collection
            .query_definition
            .ok_or_else(|| StorageError::InvalidSmartCollectionQuery("missing query".to_string()))?;
        let query: AssetSearchQuery = serde_json::from_str(&query_definition)
            .map_err(|error| StorageError::InvalidSmartCollectionQuery(error.to_string()))?;

        self.search_assets(collection.library_id, query)
    }

    pub fn set_asset_flags(
        &self,
        asset_id: Uuid,
        favorite: Option<bool>,
        review_state: Option<ReviewState>,
    ) -> Result<(), StorageError> {
        if let Some(favorite) = favorite {
            self.connection.execute(
                "UPDATE assets SET favorite = ?1 WHERE id = ?2",
                params![favorite as i64, asset_id.to_string()],
            )?;
        }

        if let Some(review_state) = review_state {
            self.connection.execute(
                "UPDATE assets SET review_state = ?1 WHERE id = ?2",
                params![review_state_to_db(review_state), asset_id.to_string()],
            )?;
        }

        Ok(())
    }

    /// Sets media_type directly — used by the AudioAnalysis job to flag real
    /// content-detected silence/corruption as "needs_review", the same
    /// pseudo-category the size-based import check already uses. Only ever
    /// called to set that flag, never to clear it, so it can't undo a
    /// user's own corrections.
    pub fn set_media_type(
        &self,
        asset_id: Uuid,
        media_type: impl AsRef<str>,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE assets SET media_type = ?1 WHERE id = ?2",
            params![media_type.as_ref(), asset_id.to_string()],
        )?;
        Ok(())
    }

    /// Fraction (0.0-1.0) of the clip the audio-analysis job's Silero VAD
    /// pass classified as speech. Set once per asset by
    /// `process_audio_analysis_jobs`; `None` until that job has run.
    pub fn set_vocal_ratio(&self, asset_id: Uuid, vocal_ratio: Option<f64>) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE assets SET vocal_ratio = ?1 WHERE id = ?2",
            params![vocal_ratio, asset_id.to_string()],
        )?;
        Ok(())
    }

    /// Fetched on demand for one asset at a time (the selected/playing
    /// sound), rather than joined into every asset list query — nothing
    /// outside the player-color feature needs it yet.
    pub fn get_vocal_ratio(&self, asset_id: Uuid) -> Result<Option<f64>, StorageError> {
        self.connection
            .query_row(
                "SELECT vocal_ratio FROM assets WHERE id = ?1",
                params![asset_id.to_string()],
                |row| row.get(0),
            )
            .optional()
            .map(Option::flatten)
            .map_err(StorageError::from)
    }

    pub fn search_assets(
        &self,
        library_id: Uuid,
        query: AssetSearchQuery,
    ) -> Result<Vec<AssetRecord>, StorageError> {
        let text = query.text.trim().to_ascii_lowercase();
        let use_fts = !text.is_empty();
        let mut sql = String::from(
            "SELECT assets.id, assets.library_id, assets.original_filename, assets.display_name,
                assets.relative_path, assets.referenced_path, assets.storage_mode, assets.content_hash,
                assets.media_type, assets.file_size, assets.availability_state, assets.review_state, assets.favorite,
                assets.embedded_title, assets.embedded_genre, assets.embedded_comment,
                assets.duration_ms, assets.sample_rate, assets.bit_depth, assets.channels,
                assets.loudness_lufs, assets.peak_db, assets.bpm, assets.bpm_confidence,
                assets.musical_key, assets.key_confidence, assets.vocal_ratio, assets.detected_key, assets.key_strength,
                assets.stem_group_id, assets.stem_label, assets.stem_is_primary
             FROM assets",
        );
        let mut query_params: Vec<rusqlite::types::Value> = vec![library_id.to_string().into()];

        if use_fts {
            sql.push_str(" INNER JOIN assets_fts ON assets_fts.rowid = assets.rowid");
        }

        sql.push_str(
            " WHERE assets.library_id = ?
               AND NOT EXISTS (
                 SELECT 1 FROM trash_items
                 WHERE trash_items.asset_id = assets.id AND trash_items.state = 'in_trash'
               )",
        );

        if use_fts {
            sql.push_str(" AND assets_fts MATCH ?");
            query_params.push(fts_query(&text).into());
        }

        if let Some(media_type) = query.media_type {
            sql.push_str(" AND assets.media_type = ?");
            query_params.push(media_type.into());
        }

        if let Some(tag_id) = query.tag_id {
            sql.push_str(
                " AND EXISTS (
                    SELECT 1 FROM asset_tags
                    WHERE asset_tags.asset_id = assets.id
                      AND asset_tags.tag_id = ?
                      AND asset_tags.approval_state = 'accepted'
                )",
            );
            query_params.push(tag_id.to_string().into());
        }

        if let Some(min) = query.duration_min_ms {
            sql.push_str(" AND assets.duration_ms >= ?");
            query_params.push(min.into());
        }
        if let Some(max) = query.duration_max_ms {
            sql.push_str(" AND assets.duration_ms <= ?");
            query_params.push(max.into());
        }
        if let Some(min) = query.bpm_min {
            sql.push_str(" AND assets.bpm >= ?");
            query_params.push(min.into());
        }
        if let Some(max) = query.bpm_max {
            sql.push_str(" AND assets.bpm <= ?");
            query_params.push(max.into());
        }
        if let Some(min) = query.peak_db_min {
            sql.push_str(" AND assets.peak_db >= ?");
            query_params.push(min.into());
        }
        if let Some(max) = query.peak_db_max {
            sql.push_str(" AND assets.peak_db <= ?");
            query_params.push(max.into());
        }

        if use_fts {
            sql.push_str(" ORDER BY rank");
        } else {
            sql.push_str(" ORDER BY assets.date_added ASC");
        }

        let mut statement = self.connection.prepare(&sql)?;
        let assets = statement
            .query_map(params_from_iter(query_params.iter()), asset_from_row)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(assets)
    }

    pub fn suggest_tag_for_asset(
        &self,
        asset_id: Uuid,
        tag_id: Uuid,
        origin: TagOrigin,
        confidence: f32,
    ) -> Result<(), StorageError> {
        let existing_state: Option<String> = self
            .connection
            .query_row(
                "SELECT approval_state FROM asset_tags WHERE asset_id = ?1 AND tag_id = ?2 AND origin = ?3",
                params![asset_id.to_string(), tag_id.to_string(), tag_origin_to_db(origin)],
                |row| row.get(0),
            )
            .optional()?;

        if matches!(existing_state.as_deref(), Some("rejected")) {
            return Ok(());
        }

        self.connection.execute(
            "INSERT INTO asset_tags (asset_id, tag_id, origin, confidence, approval_state, created_at)
             VALUES (?1, ?2, ?3, ?4, 'suggested', ?5)
             ON CONFLICT(asset_id, tag_id, origin) DO UPDATE SET
                confidence = excluded.confidence
             WHERE asset_tags.approval_state != 'rejected'",
            params![
                asset_id.to_string(),
                tag_id.to_string(),
                tag_origin_to_db(origin),
                confidence,
                Utc::now().to_rfc3339(),
            ],
        )?;

        Ok(())
    }

    pub fn pending_suggested_tags(&self, asset_id: Uuid) -> Result<Vec<TagRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT tags.id, tags.name, tags.normalized_name, tags.facet, tags.is_system
             FROM tags
             INNER JOIN asset_tags ON asset_tags.tag_id = tags.id
             WHERE asset_tags.asset_id = ?1 AND asset_tags.approval_state = 'suggested'
             ORDER BY asset_tags.confidence DESC",
        )?;
        let tags = statement
            .query_map(params![asset_id.to_string()], tag_from_row)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)?;

        Ok(tags)
    }

    pub fn set_tag_approval(
        &self,
        asset_id: Uuid,
        tag_id: Uuid,
        origin: TagOrigin,
        approval_state: TagApprovalState,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE asset_tags SET approval_state = ?1
             WHERE asset_id = ?2 AND tag_id = ?3 AND origin = ?4",
            params![
                tag_approval_to_db(approval_state),
                asset_id.to_string(),
                tag_id.to_string(),
                tag_origin_to_db(origin),
            ],
        )?;

        Ok(())
    }

    pub fn validate_media_availability(
        &self,
        library_id: Uuid,
        is_available: impl Fn(&str) -> bool,
    ) -> Result<usize, StorageError> {
        let assets = self.list_assets(library_id)?;
        let mut changed = 0;

        for asset in assets {
            let path = match &asset.path {
                AssetPath::Managed(path) | AssetPath::Referenced(path) => path,
            };
            let next_state = if is_available(path) {
                AvailabilityState::Local
            } else {
                AvailabilityState::Missing
            };

            if asset.availability_state != next_state {
                self.connection.execute(
                    "UPDATE assets SET availability_state = ?1, last_seen = ?2 WHERE id = ?3",
                    params![
                        availability_to_db(&next_state),
                        Utc::now().to_rfc3339(),
                        asset.id.to_string(),
                    ],
                )?;
                changed += 1;
            }
        }

        Ok(changed)
    }

    pub fn relink_asset(
        &self,
        asset_id: Uuid,
        referenced_path: impl AsRef<str>,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE assets
             SET referenced_path = ?1,
                 relative_path = NULL,
                 storage_mode = 'referenced',
                 availability_state = 'local',
                 last_seen = ?2
             WHERE id = ?3",
            params![
                referenced_path.as_ref(),
                Utc::now().to_rfc3339(),
                asset_id.to_string(),
            ],
        )?;

        // The relinked file is a different file on disk — its old waveform
        // shape no longer describes it. Drop the cache so the next preview
        // regenerates from the new source.
        self.clear_waveform_cache(asset_id)?;

        Ok(())
    }

    pub fn record_usage_event(
        &self,
        asset_id: Uuid,
        project_id: Option<Uuid>,
        event_type: UsageEventType,
        destination: impl AsRef<str>,
    ) -> Result<UsageEventRecord, StorageError> {
        let event = UsageEventRecord {
            id: Uuid::new_v4(),
            asset_id,
            project_id,
            event_type,
            destination: Some(destination.as_ref().to_string()),
        };

        self.connection.execute(
            "INSERT INTO usage_events (id, asset_id, project_id, event_type, destination, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                event.id.to_string(),
                event.asset_id.to_string(),
                event.project_id.map(|id| id.to_string()),
                usage_event_type_to_db(event.event_type),
                event.destination,
                Utc::now().to_rfc3339(),
            ],
        )?;

        self.connection.execute(
            "UPDATE assets SET export_count = export_count + ?1 WHERE id = ?2",
            params![
                (event_type == UsageEventType::Exported) as i64,
                asset_id.to_string()
            ],
        )?;

        Ok(event)
    }

    pub fn usage_events_for_project(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<UsageEventRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, asset_id, project_id, event_type, destination
             FROM usage_events
             WHERE project_id = ?1
             ORDER BY created_at ASC",
        )?;
        let events = statement
            .query_map(params![project_id.to_string()], usage_event_from_row)?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(events)
    }

    pub fn set_source_record(&self, draft: SourceRecordDraft) -> Result<(), StorageError> {
        self.connection.execute(
            "DELETE FROM source_records WHERE asset_id = ?1",
            params![draft.asset_id.to_string()],
        )?;
        self.connection.execute(
            "INSERT INTO source_records (
                id, asset_id, provider, source_url, license_type, license_status,
                attribution, restrictions, receipt_path,
                license_document_path, license_valid_from, license_valid_until, license_expiry_source
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)",
            params![
                Uuid::new_v4().to_string(),
                draft.asset_id.to_string(),
                draft.provider,
                draft.source_url,
                draft.license_type,
                draft.license_status,
                draft.attribution,
                draft.restrictions,
                draft.receipt_path,
                draft.license_document_path,
                draft.license_valid_from,
                draft.license_valid_until,
                draft.license_expiry_source,
            ],
        )?;

        Ok(())
    }

    pub fn project_source_report(
        &self,
        project_id: Uuid,
    ) -> Result<Vec<ProjectSourceReportRow>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT
                assets.id,
                assets.display_name,
                assets.original_filename,
                source_records.provider,
                source_records.source_url,
                source_records.license_type,
                source_records.license_status,
                source_records.attribution,
                source_records.restrictions,
                source_records.receipt_path,
                source_records.license_valid_until,
                usage_events.event_type,
                usage_events.destination
             FROM usage_events
             INNER JOIN assets ON assets.id = usage_events.asset_id
             LEFT JOIN source_records ON source_records.asset_id = assets.id
             WHERE usage_events.project_id = ?1
             ORDER BY usage_events.created_at ASC",
        )?;
        let rows = statement
            .query_map(params![project_id.to_string()], |row| {
                Ok(ProjectSourceReportRow {
                    asset_id: parse_uuid(row.get::<_, String>(0)?),
                    asset_title: row.get(1)?,
                    original_filename: row.get(2)?,
                    provider: row.get(3)?,
                    source_url: row.get(4)?,
                    license_type: row.get(5)?,
                    license_status: row.get(6)?,
                    attribution: row.get(7)?,
                    restrictions: row.get(8)?,
                    receipt_path: row.get(9)?,
                    license_valid_until: row.get(10)?,
                    usage_status: row.get(11)?,
                    destination: row.get(12)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    pub fn undo(&self, undo_id: Uuid) -> Result<(), StorageError> {
        let undo = self
            .connection
            .query_row(
                "SELECT kind, payload FROM undo_actions WHERE id = ?1 AND applied_at IS NULL",
                params![undo_id.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;

        let Some((kind, payload)) = undo else {
            return Ok(());
        };

        match kind.as_str() {
            "remove_asset_tags" => {
                let (tag_id, _origin, asset_ids) = split_tag_owner_origin_and_assets(&payload);
                for asset_id in asset_ids {
                    self.connection.execute(
                        "DELETE FROM asset_tags WHERE tag_id = ?1 AND asset_id = ?2",
                        params![tag_id.to_string(), asset_id.to_string()],
                    )?;
                }
            }
            "remove_collection_assets" => {
                let (owner_id, asset_ids) = split_owner_and_assets(&payload);
                for asset_id in asset_ids {
                    self.connection.execute(
                        "DELETE FROM collection_assets WHERE collection_id = ?1 AND asset_id = ?2",
                        params![owner_id.to_string(), asset_id.to_string()],
                    )?;
                }
            }
            "readd_asset_tags" => {
                let (tag_id, origin, asset_ids) = split_tag_owner_origin_and_assets(&payload);
                for asset_id in asset_ids {
                    self.connection.execute(
                        "INSERT OR IGNORE INTO asset_tags (asset_id, tag_id, origin, confidence, approval_state, created_at)
                         VALUES (?1, ?2, ?3, 1.0, 'accepted', ?4)",
                        params![asset_id.to_string(), tag_id.to_string(), origin, Utc::now().to_rfc3339()],
                    )?;
                }
            }
            _ => {}
        }

        self.connection.execute(
            "UPDATE undo_actions SET applied_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), undo_id.to_string()],
        )?;

        Ok(())
    }

    pub fn redo(&self, undo_id: Uuid) -> Result<(), StorageError> {
        let undo = self
            .connection
            .query_row(
                "SELECT kind, payload FROM undo_actions WHERE id = ?1 AND applied_at IS NOT NULL",
                params![undo_id.to_string()],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)),
            )
            .optional()?;

        let Some((kind, payload)) = undo else {
            return Ok(());
        };

        let now = Utc::now().to_rfc3339();
        match kind.as_str() {
            "remove_asset_tags" => {
                let (tag_id, origin, asset_ids) = split_tag_owner_origin_and_assets(&payload);
                for asset_id in asset_ids {
                    self.connection.execute(
                        "INSERT OR IGNORE INTO asset_tags (asset_id, tag_id, origin, confidence, approval_state, created_at)
                         VALUES (?1, ?2, ?3, 1.0, 'accepted', ?4)",
                        params![asset_id.to_string(), tag_id.to_string(), origin, now],
                    )?;
                }
            }
            "remove_collection_assets" => {
                let (owner_id, asset_ids) = split_owner_and_assets(&payload);
                for asset_id in asset_ids {
                    self.connection.execute(
                        "INSERT OR IGNORE INTO collection_assets (collection_id, asset_id, created_at)
                         VALUES (?1, ?2, ?3)",
                        params![owner_id.to_string(), asset_id.to_string(), now],
                    )?;
                }
            }
            "readd_asset_tags" => {
                let (tag_id, _origin, asset_ids) = split_tag_owner_origin_and_assets(&payload);
                for asset_id in asset_ids {
                    self.connection.execute(
                        "DELETE FROM asset_tags WHERE tag_id = ?1 AND asset_id = ?2",
                        params![tag_id.to_string(), asset_id.to_string()],
                    )?;
                }
            }
            _ => {}
        }

        self.connection.execute(
            "UPDATE undo_actions SET applied_at = NULL WHERE id = ?1",
            params![undo_id.to_string()],
        )?;

        Ok(())
    }

    pub fn get_source_record(
        &self,
        asset_id: Uuid,
    ) -> Result<Option<SourceRecordDraft>, StorageError> {
        self.connection
            .query_row(
                "SELECT asset_id, provider, source_url, license_type, license_status,
                    attribution, restrictions, receipt_path,
                    license_document_path, license_valid_from, license_valid_until, license_expiry_source
                 FROM source_records WHERE asset_id = ?1",
                params![asset_id.to_string()],
                |row| {
                    Ok(SourceRecordDraft {
                        asset_id: parse_uuid(row.get::<_, String>(0)?),
                        provider: row.get(1)?,
                        source_url: row.get(2)?,
                        license_type: row.get(3)?,
                        license_status: row.get(4)?,
                        attribution: row.get(5)?,
                        restrictions: row.get(6)?,
                        receipt_path: row.get(7)?,
                        license_document_path: row.get(8)?,
                        license_valid_from: row.get(9)?,
                        license_valid_until: row.get(10)?,
                        license_expiry_source: row.get(11)?,
                    })
                },
            )
            .optional()
            .map_err(StorageError::from)
    }

    /// Which of this library's assets already have a source/license record
    /// — one query instead of `get_source_record` called once per asset.
    /// Backs `maintenance_report`'s license-review-needed check, which used
    /// to run a separate query per asset (an N+1 pattern that, held under
    /// the same catalog mutex every other command needs, was a real
    /// contributor to a multi-second stall on every app launch for
    /// anything but a small library).
    pub fn asset_ids_with_source_record(
        &self,
        library_id: Uuid,
    ) -> Result<std::collections::HashSet<Uuid>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT source_records.asset_id
             FROM source_records
             INNER JOIN assets ON assets.id = source_records.asset_id
             WHERE assets.library_id = ?1",
        )?;
        let ids = statement
            .query_map(params![library_id.to_string()], |row| {
                row.get::<_, String>(0)
            })?
            .map(|result| result.map(parse_uuid))
            .collect::<Result<std::collections::HashSet<_>, _>>()
            .map_err(StorageError::from)?;

        Ok(ids)
    }

    /// Assets in this library whose confirmed `license_valid_until` has
    /// already passed `today` (an ISO `YYYY-MM-DD` string — plain string
    /// comparison is correct here since ISO dates sort lexicographically).
    /// Backs the `LicenseExpired` maintenance finding. A null
    /// `license_valid_until` (no expiry set, or nothing confirmed yet)
    /// never counts as expired.
    pub fn assets_with_expired_license(
        &self,
        library_id: Uuid,
        today: &str,
    ) -> Result<Vec<(Uuid, String)>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT source_records.asset_id, source_records.license_valid_until
             FROM source_records
             INNER JOIN assets ON assets.id = source_records.asset_id
             WHERE assets.library_id = ?1
               AND source_records.license_valid_until IS NOT NULL
               AND source_records.license_valid_until < ?2",
        )?;
        let rows = statement
            .query_map(params![library_id.to_string(), today], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?
            .map(|result| result.map(|(asset_id, expiry)| (parse_uuid(asset_id), expiry)))
            .collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)?;

        Ok(rows)
    }

    pub fn move_asset_to_trash(
        &self,
        asset_id: Uuid,
        reason: impl AsRef<str>,
        now_ms: u64,
    ) -> Result<trash::TrashItem, StorageError> {
        let asset = self
            .get_asset(asset_id)?
            .ok_or(StorageError::AssetNotFound)?;
        let original_path = match asset.path {
            AssetPath::Managed(path) | AssetPath::Referenced(path) => path,
        };
        let item = trash::TrashItem::for_asset(
            asset_id,
            original_path,
            now_ms,
            reason.as_ref().to_string(),
        );

        self.connection.execute(
            "INSERT OR REPLACE INTO trash_items (asset_id, original_path, trashed_at_ms, reason, state, file_deleted)
             VALUES (?1, ?2, ?3, ?4, 'in_trash', 0)",
            params![
                item.asset_id.to_string(),
                item.original_path,
                item.trashed_at_ms as i64,
                item.reason,
            ],
        )?;

        Ok(item)
    }

    pub fn list_trash_items(
        &self,
        library_id: Uuid,
    ) -> Result<Vec<trash::TrashItem>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT trash_items.asset_id, trash_items.original_path, trash_items.trashed_at_ms,
                trash_items.reason, trash_items.state, trash_items.file_deleted
             FROM trash_items
             INNER JOIN assets ON assets.id = trash_items.asset_id
             WHERE assets.library_id = ?1 AND trash_items.state = 'in_trash'
             ORDER BY trash_items.trashed_at_ms DESC",
        )?;

        let items = statement
            .query_map(params![library_id.to_string()], trash_item_from_row)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)?;

        Ok(items)
    }

    pub fn restore_asset_from_trash(&self, asset_id: Uuid) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE trash_items SET state = 'restored' WHERE asset_id = ?1",
            params![asset_id.to_string()],
        )?;

        Ok(())
    }

    /// Permanently removes every currently-trashed asset's catalog row for
    /// one library, bypassing the normal retention wait — an explicit,
    /// user-initiated "empty trash" action, not the automatic timed purge.
    /// Same guarantee as `purge_trash_item`: only ever deletes SQLite rows,
    /// never touches a real file.
    pub fn empty_trash_for_library(&self, library_id: Uuid) -> Result<usize, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT trash_items.asset_id FROM trash_items
             INNER JOIN assets ON assets.id = trash_items.asset_id
             WHERE assets.library_id = ?1 AND trash_items.state = 'in_trash'",
        )?;
        let asset_ids: Vec<String> = statement
            .query_map(params![library_id.to_string()], |row| row.get(0))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(StorageError::from)?;
        drop(statement);

        for asset_id in &asset_ids {
            self.connection.execute(
                "UPDATE trash_items SET state = 'purged' WHERE asset_id = ?1",
                params![asset_id],
            )?;
            self.connection
                .execute("DELETE FROM assets WHERE id = ?1", params![asset_id])?;
        }

        Ok(asset_ids.len())
    }

    pub fn purge_trash_item(
        &self,
        asset_id: Uuid,
        now_ms: u64,
        retention_ms: u64,
    ) -> Result<bool, StorageError> {
        let item = self
            .connection
            .query_row(
                "SELECT asset_id, original_path, trashed_at_ms, reason, state, file_deleted
                 FROM trash_items WHERE asset_id = ?1",
                params![asset_id.to_string()],
                trash_item_from_row,
            )
            .optional()?;

        let Some(item) = item else {
            return Ok(false);
        };

        if !item.is_purge_allowed(now_ms, retention_ms, true) {
            return Ok(false);
        }

        self.connection.execute(
            "UPDATE trash_items SET state = 'purged' WHERE asset_id = ?1",
            params![asset_id.to_string()],
        )?;
        self.connection.execute(
            "DELETE FROM assets WHERE id = ?1",
            params![asset_id.to_string()],
        )?;

        Ok(true)
    }

    /// Finalizes a trash item's removal same as `purge_trash_item`, but
    /// without the retention-window gate and marking `file_deleted = 1`
    /// instead of leaving it `0` — this is the DB half of an explicit,
    /// user-confirmed "Delete" action from the trash view (see
    /// `delete_trash_item_permanently` in the Tauri command layer), which
    /// deletes the real file itself. Deliberately kept out of this crate:
    /// every other trash function here only ever touches SQLite rows, and
    /// that's a real invariant other code (and this crate's own docs) rely
    /// on — actual filesystem deletion belongs one layer up, next to the
    /// asset-path resolution logic it needs.
    pub fn finalize_permanent_deletion(&self, asset_id: Uuid) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE trash_items SET state = 'purged', file_deleted = 1 WHERE asset_id = ?1",
            params![asset_id.to_string()],
        )?;
        self.connection.execute(
            "DELETE FROM assets WHERE id = ?1",
            params![asset_id.to_string()],
        )?;

        Ok(())
    }

    fn migrate(&self) -> Result<(), StorageError> {
        self.connection.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS libraries (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              media_root TEXT NOT NULL,
              created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS assets (
              id TEXT PRIMARY KEY,
              library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
              original_filename TEXT NOT NULL,
              display_name TEXT NOT NULL,
              relative_path TEXT,
              referenced_path TEXT,
              storage_mode TEXT NOT NULL,
              content_hash TEXT,
              perceptual_fingerprint TEXT,
              media_type TEXT NOT NULL DEFAULT 'other',
              duration_ms INTEGER,
              sample_rate INTEGER,
              bit_depth INTEGER,
              channels INTEGER,
              file_size INTEGER NOT NULL DEFAULT 0,
              loudness_lufs REAL,
              peak_db REAL,
              bpm REAL,
              bpm_confidence REAL,
              musical_key TEXT,
              key_confidence REAL,
              waveform_version INTEGER NOT NULL DEFAULT 0,
              availability_state TEXT NOT NULL DEFAULT 'unknown',
              review_state TEXT NOT NULL DEFAULT 'unreviewed',
              date_added TEXT NOT NULL,
              last_seen TEXT,
              last_played TEXT,
              play_count INTEGER NOT NULL DEFAULT 0,
              export_count INTEGER NOT NULL DEFAULT 0,
              favorite INTEGER NOT NULL DEFAULT 0,
              notes TEXT,
              embedded_title TEXT,
              embedded_genre TEXT,
              embedded_comment TEXT
            );

            CREATE TABLE IF NOT EXISTS waveform_peaks (
              asset_id TEXT PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
              waveform_version INTEGER NOT NULL,
              sample_rate INTEGER NOT NULL,
              payload TEXT NOT NULL,
              generated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS asset_instruments (
              asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
              instrument TEXT NOT NULL,
              confidence REAL NOT NULL DEFAULT 0,
              PRIMARY KEY (asset_id, instrument)
            );
            CREATE INDEX IF NOT EXISTS idx_asset_instruments_instrument
              ON asset_instruments(instrument);

            CREATE TABLE IF NOT EXISTS background_jobs (
              id TEXT PRIMARY KEY,
              asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
              kind TEXT NOT NULL,
              priority INTEGER NOT NULL DEFAULT 100,
              state TEXT NOT NULL DEFAULT 'pending',
              attempts INTEGER NOT NULL DEFAULT 0,
              error TEXT,
              created_at TEXT NOT NULL,
              updated_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS tags (
              id TEXT PRIMARY KEY,
              name TEXT NOT NULL,
              normalized_name TEXT NOT NULL UNIQUE,
              facet TEXT,
              parent_id TEXT REFERENCES tags(id) ON DELETE SET NULL,
              preferred_term_id TEXT REFERENCES tags(id) ON DELETE SET NULL,
              is_system INTEGER NOT NULL DEFAULT 0,
              is_hidden INTEGER NOT NULL DEFAULT 0,
              created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS asset_tags (
              asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
              tag_id TEXT NOT NULL REFERENCES tags(id) ON DELETE CASCADE,
              origin TEXT NOT NULL,
              confidence REAL NOT NULL DEFAULT 1.0,
              approval_state TEXT NOT NULL,
              created_at TEXT NOT NULL,
              PRIMARY KEY (asset_id, tag_id, origin)
            );

            CREATE TABLE IF NOT EXISTS trash_items (
              asset_id TEXT PRIMARY KEY REFERENCES assets(id) ON DELETE CASCADE,
              original_path TEXT NOT NULL,
              trashed_at_ms INTEGER NOT NULL,
              reason TEXT NOT NULL,
              state TEXT NOT NULL DEFAULT 'in_trash',
              file_deleted INTEGER NOT NULL DEFAULT 0
            );

            CREATE TABLE IF NOT EXISTS collections (
              id TEXT PRIMARY KEY,
              library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
              name TEXT NOT NULL,
              type TEXT NOT NULL,
              query_definition TEXT,
              parent_id TEXT REFERENCES collections(id) ON DELETE SET NULL,
              created_at TEXT NOT NULL,
              archived_at TEXT
            );

            CREATE TABLE IF NOT EXISTS collection_assets (
              collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
              asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
              created_at TEXT NOT NULL,
              PRIMARY KEY (collection_id, asset_id)
            );

            -- A categorized export destination on a project, beyond the two
            -- built-in ones (`collections.export_path`/`sfx_export_path`):
            -- role uses the same vocabulary as a library `folders` row
            -- (music/sound_effect/voiceover/foley/ambience/documents), so
            -- an asset already carrying that media_type routes here on
            -- export instead of falling through to the generic sfx bucket.
            -- Basic projects never touch this table — export_path/
            -- sfx_export_path alone still fully define where things go.
            CREATE TABLE IF NOT EXISTS project_export_folders (
              id TEXT PRIMARY KEY,
              collection_id TEXT NOT NULL REFERENCES collections(id) ON DELETE CASCADE,
              path TEXT NOT NULL,
              role TEXT NOT NULL,
              created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS source_records (
              id TEXT PRIMARY KEY,
              asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
              provider TEXT,
              source_url TEXT,
              downloaded_at TEXT,
              license_type TEXT,
              license_status TEXT,
              attribution TEXT,
              restrictions TEXT,
              receipt_path TEXT,
              notes TEXT
            );

            CREATE TABLE IF NOT EXISTS usage_events (
              id TEXT PRIMARY KEY,
              asset_id TEXT NOT NULL REFERENCES assets(id) ON DELETE CASCADE,
              project_id TEXT REFERENCES collections(id) ON DELETE SET NULL,
              event_type TEXT NOT NULL,
              destination TEXT,
              created_at TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS undo_actions (
              id TEXT PRIMARY KEY,
              kind TEXT NOT NULL,
              payload TEXT NOT NULL,
              created_at TEXT NOT NULL,
              applied_at TEXT
            );

            -- A folder the user has told Darkwave about, beyond the single
            -- implicit `media_root` every library already had: the basic
            -- single-folder setup gets exactly one row here (role NULL,
            -- kind 'media_root'), mirroring `libraries.media_root`; the
            -- advanced multi-folder setup adds one row per additional
            -- folder, each optionally tagged with the media type it's
            -- expected to hold (a strong classification signal on import —
            -- see import-pipeline — but never the only one). `kind`
            -- 'documents' (e.g. a folder of licence PDFs) is watched for
            -- inventory only, never fed through audio import/analysis.
            CREATE TABLE IF NOT EXISTS folders (
              id TEXT PRIMARY KEY,
              library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
              path TEXT NOT NULL,
              role TEXT,
              kind TEXT NOT NULL DEFAULT 'media_root',
              created_at TEXT NOT NULL
            );

            -- A file discovered inside a `kind = 'documents'` folder —
            -- inventory only: no analysis job, no AssetRecord. See
            -- docs/genesis plan and ADR discussion on the license-documents
            -- folder case for why this stays deliberately lightweight.
            CREATE TABLE IF NOT EXISTS folder_documents (
              id TEXT PRIMARY KEY,
              library_id TEXT NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
              folder_id TEXT NOT NULL REFERENCES folders(id) ON DELETE CASCADE,
              path TEXT NOT NULL,
              filename TEXT NOT NULL,
              discovered_at TEXT NOT NULL,
              UNIQUE (folder_id, path)
            );

            CREATE VIRTUAL TABLE IF NOT EXISTS assets_fts USING fts5(
              display_name,
              original_filename,
              notes,
              content='assets',
              content_rowid='rowid'
            );

            CREATE TRIGGER IF NOT EXISTS assets_fts_insert AFTER INSERT ON assets BEGIN
              INSERT INTO assets_fts(rowid, display_name, original_filename, notes)
              VALUES (new.rowid, new.display_name, new.original_filename, new.notes);
            END;

            CREATE TRIGGER IF NOT EXISTS assets_fts_delete AFTER DELETE ON assets BEGIN
              INSERT INTO assets_fts(assets_fts, rowid, display_name, original_filename, notes)
              VALUES ('delete', old.rowid, old.display_name, old.original_filename, old.notes);
            END;

            CREATE TRIGGER IF NOT EXISTS assets_fts_update AFTER UPDATE ON assets BEGIN
              INSERT INTO assets_fts(assets_fts, rowid, display_name, original_filename, notes)
              VALUES ('delete', old.rowid, old.display_name, old.original_filename, old.notes);
              INSERT INTO assets_fts(rowid, display_name, original_filename, notes)
              VALUES (new.rowid, new.display_name, new.original_filename, new.notes);
            END;

            CREATE UNIQUE INDEX IF NOT EXISTS idx_assets_library_hash_size
              ON assets(library_id, content_hash, file_size)
              WHERE content_hash IS NOT NULL;
            CREATE INDEX IF NOT EXISTS idx_assets_library_media_date
              ON assets(library_id, media_type, date_added);
            CREATE INDEX IF NOT EXISTS idx_background_jobs_pending
              ON background_jobs(state, priority, created_at);
            -- latest_job_state_for_asset (the Sonic Radar inspector's
            -- per-track Analyze buttons poll this every ~800ms while
            -- waiting on a result) filters by asset_id + kind and orders
            -- by updated_at — without this, every one of those polls was
            -- a full table scan of every job ever queued, competing for
            -- the same catalog mutex background job processing needs.
            CREATE INDEX IF NOT EXISTS idx_background_jobs_asset_kind
              ON background_jobs(asset_id, kind, updated_at);
            CREATE INDEX IF NOT EXISTS idx_asset_tags_asset ON asset_tags(asset_id);
            CREATE INDEX IF NOT EXISTS idx_asset_tags_tag_state_asset
              ON asset_tags(tag_id, approval_state, asset_id);
            CREATE INDEX IF NOT EXISTS idx_collection_assets_collection ON collection_assets(collection_id);
            CREATE INDEX IF NOT EXISTS idx_usage_events_project ON usage_events(project_id);
            CREATE INDEX IF NOT EXISTS idx_usage_events_asset ON usage_events(asset_id);
            ",
        )?;

        // `CREATE TABLE IF NOT EXISTS` above doesn't touch columns on a table
        // that already exists from an earlier app version, so new columns on
        // existing tables need an explicit, idempotent ADD COLUMN step.
        self.ensure_column("collections", "export_path", "TEXT")?;
        self.ensure_column("collections", "sfx_export_path", "TEXT")?;
        self.ensure_column("assets", "vocal_ratio", "REAL")?;
        self.ensure_column("assets", "detected_key", "TEXT")?;
        self.ensure_column("assets", "key_strength", "REAL")?;
        self.ensure_column("libraries", "media_root_bookmark", "TEXT")?;
        self.ensure_column("libraries", "import_root", "TEXT")?;
        self.ensure_column("assets", "stem_group_id", "TEXT")?;
        self.ensure_column("assets", "stem_label", "TEXT")?;
        self.ensure_column("assets", "stem_is_primary", "INTEGER NOT NULL DEFAULT 0")?;
        // JSON array of subfolder names under media_root a dropped file can
        // be routed into (e.g. ["Soundtrack", "SFX"]) — see
        // set_library_import_subfolders. `None`/absent means "just one
        // destination, media_root itself" — no prompt needed on drop.
        self.ensure_column("libraries", "import_subfolders", "TEXT")?;
        // See SourceRecordDraft's doc comments for what each of these
        // means — license_valid_until is the one the expired/valid badge
        // and the LicenseExpired maintenance finding are computed from.
        self.ensure_column("source_records", "license_document_path", "TEXT")?;
        self.ensure_column("source_records", "license_valid_from", "TEXT")?;
        self.ensure_column("source_records", "license_valid_until", "TEXT")?;
        self.ensure_column("source_records", "license_expiry_source", "TEXT")?;

        Ok(())
    }

    /// Adds `column` to `table` if it isn't already there. `table`/`column`/
    /// `sql_type` must be trusted, internally-controlled literals — they're
    /// interpolated directly since SQLite doesn't allow binding identifiers
    /// as query parameters (including inside PRAGMA).
    fn ensure_column(&self, table: &str, column: &str, sql_type: &str) -> Result<(), StorageError> {
        let mut statement = self
            .connection
            .prepare(&format!("PRAGMA table_info({table})"))?;
        let has_column = statement
            .query_map([], |row| row.get::<_, String>(1))?
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .any(|name| name == column);

        if !has_column {
            self.connection.execute_batch(&format!(
                "ALTER TABLE {table} ADD COLUMN {column} {sql_type}"
            ))?;
        }

        Ok(())
    }

    pub fn get_asset(&self, id: Uuid) -> Result<Option<AssetRecord>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, library_id, original_filename, display_name, relative_path, referenced_path,
                    storage_mode, content_hash, media_type, file_size, availability_state, review_state, favorite,
                    embedded_title, embedded_genre, embedded_comment,
                    duration_ms, sample_rate, bit_depth, channels, loudness_lufs, peak_db,
                    bpm, bpm_confidence, musical_key, key_confidence, vocal_ratio, detected_key, key_strength,
                    stem_group_id, stem_label, stem_is_primary
                 FROM assets WHERE id = ?1",
                params![id.to_string()],
                asset_from_row,
            )
            .optional()
            .map_err(StorageError::from)
    }

    fn find_asset_by_hash(
        &self,
        library_id: Uuid,
        content_hash: &str,
        file_size: u64,
    ) -> Result<Option<AssetRecord>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, library_id, original_filename, display_name, relative_path, referenced_path,
                    storage_mode, content_hash, media_type, file_size, availability_state, review_state, favorite,
                    embedded_title, embedded_genre, embedded_comment,
                    duration_ms, sample_rate, bit_depth, channels, loudness_lufs, peak_db,
                    bpm, bpm_confidence, musical_key, key_confidence, vocal_ratio, detected_key, key_strength,
                    stem_group_id, stem_label, stem_is_primary
                 FROM assets
                 WHERE library_id = ?1 AND content_hash = ?2 AND file_size = ?3
                 LIMIT 1",
                params![library_id.to_string(), content_hash, file_size as i64],
                asset_from_row,
            )
            .optional()
            .map_err(StorageError::from)
    }

    fn record_undo(&self, kind: &str, payload: &str) -> Result<Uuid, StorageError> {
        let id = Uuid::new_v4();
        self.connection.execute(
            "INSERT INTO undo_actions (id, kind, payload, created_at) VALUES (?1, ?2, ?3, ?4)",
            params![id.to_string(), kind, payload, Utc::now().to_rfc3339()],
        )?;
        Ok(id)
    }
}

/// A malformed stored value (shouldn't happen — only `set_library_import_subfolders`
/// ever writes this column) is treated the same as absent: no subfolders,
/// no prompt, rather than an error surfacing on every library load.
fn parse_import_subfolders(raw: Option<String>) -> Vec<String> {
    raw.and_then(|json| serde_json::from_str::<Vec<String>>(&json).ok())
        .unwrap_or_default()
}

fn asset_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<AssetRecord> {
    let relative_path: Option<String> = row.get(4)?;
    let referenced_path: Option<String> = row.get(5)?;
    let path = match (relative_path, referenced_path) {
        (Some(path), _) => AssetPath::Managed(path),
        (_, Some(path)) => AssetPath::Referenced(path),
        (None, None) => AssetPath::Referenced(String::new()),
    };

    Ok(AssetRecord {
        id: parse_uuid(row.get::<_, String>(0)?),
        library_id: parse_uuid(row.get::<_, String>(1)?),
        original_filename: row.get(2)?,
        display_name: row.get(3)?,
        path,
        storage_mode: storage_mode_from_db(&row.get::<_, String>(6)?),
        content_hash: row.get(7)?,
        media_type: row.get(8)?,
        file_size: row.get::<_, i64>(9)? as u64,
        availability_state: availability_from_db(&row.get::<_, String>(10)?),
        review_state: review_state_from_db(&row.get::<_, String>(11)?),
        favorite: row.get::<_, i64>(12)? != 0,
        embedded_title: row.get(13)?,
        embedded_genre: row.get(14)?,
        embedded_comment: row.get(15)?,
        duration_ms: row.get(16)?,
        sample_rate: row.get(17)?,
        bit_depth: row.get(18)?,
        channels: row.get(19)?,
        loudness_lufs: row.get(20)?,
        peak_db: row.get(21)?,
        bpm: row.get(22)?,
        bpm_confidence: row.get(23)?,
        musical_key: row.get(24)?,
        key_confidence: row.get(25)?,
        vocal_ratio: row.get(26)?,
        detected_key: row.get(27)?,
        key_strength: row.get(28)?,
        stem_group_id: row.get::<_, Option<String>>(29)?.map(parse_uuid),
        stem_label: row.get(30)?,
        stem_is_primary: row.get::<_, i64>(31)? != 0,
    })
}

fn tag_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<TagRecord> {
    Ok(TagRecord {
        id: parse_uuid(row.get::<_, String>(0)?),
        name: row.get(1)?,
        normalized_name: row.get(2)?,
        facet: row.get(3)?,
        is_system: row.get::<_, i64>(4)? != 0,
    })
}

fn collection_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CollectionRecord> {
    Ok(CollectionRecord {
        id: parse_uuid(row.get::<_, String>(0)?),
        library_id: parse_uuid(row.get::<_, String>(1)?),
        name: row.get(2)?,
        collection_type: collection_type_from_db(&row.get::<_, String>(3)?),
        query_definition: row.get(4)?,
        export_path: row.get(5)?,
        sfx_export_path: row.get(6)?,
    })
}

fn trash_item_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<trash::TrashItem> {
    Ok(trash::TrashItem {
        asset_id: parse_uuid(row.get::<_, String>(0)?),
        original_path: row.get(1)?,
        trashed_at_ms: row.get::<_, i64>(2)? as u64,
        reason: row.get(3)?,
        state: trash_state_from_db(&row.get::<_, String>(4)?),
        file_deleted: row.get::<_, i64>(5)? != 0,
    })
}

fn trash_state_from_db(value: &str) -> trash::TrashState {
    match value {
        "restored" => trash::TrashState::Restored,
        "purged" => trash::TrashState::Purged,
        _ => trash::TrashState::InTrash,
    }
}

fn parse_uuid(value: String) -> Uuid {
    Uuid::parse_str(&value).expect("database contains valid uuid")
}

fn storage_mode_to_db(mode: &StorageMode) -> &'static str {
    match mode {
        StorageMode::Managed => "managed",
        StorageMode::Referenced => "referenced",
        StorageMode::Hybrid => "hybrid",
    }
}

fn storage_mode_from_db(value: &str) -> StorageMode {
    match value {
        "managed" => StorageMode::Managed,
        "hybrid" => StorageMode::Hybrid,
        _ => StorageMode::Referenced,
    }
}

fn availability_to_db(state: &AvailabilityState) -> &'static str {
    match state {
        AvailabilityState::Unknown => "unknown",
        AvailabilityState::Local => "local",
        AvailabilityState::Cached => "cached",
        AvailabilityState::Missing => "missing",
    }
}

fn availability_from_db(value: &str) -> AvailabilityState {
    match value {
        "local" => AvailabilityState::Local,
        "cached" => AvailabilityState::Cached,
        "missing" => AvailabilityState::Missing,
        _ => AvailabilityState::Unknown,
    }
}

fn review_state_to_db(state: ReviewState) -> &'static str {
    match state {
        ReviewState::Unreviewed => "unreviewed",
        ReviewState::Reviewed => "reviewed",
    }
}

fn review_state_from_db(value: &str) -> ReviewState {
    match value {
        "reviewed" => ReviewState::Reviewed,
        _ => ReviewState::Unreviewed,
    }
}

fn tag_origin_to_db(origin: TagOrigin) -> &'static str {
    match origin {
        TagOrigin::Filename => "filename",
        TagOrigin::Metadata => "metadata",
        TagOrigin::AcousticModel => "acoustic_model",
        TagOrigin::UserRule => "user_rule",
        TagOrigin::UserCorrection => "user_correction",
        TagOrigin::Manual => "manual",
    }
}

fn collection_type_to_db(collection_type: CollectionType) -> &'static str {
    match collection_type {
        CollectionType::Manual => "manual",
        CollectionType::Smart => "smart",
        CollectionType::Project => "project",
    }
}

fn collection_type_from_db(value: &str) -> CollectionType {
    match value {
        "smart" => CollectionType::Smart,
        "project" => CollectionType::Project,
        _ => CollectionType::Manual,
    }
}

fn tag_approval_to_db(approval_state: TagApprovalState) -> &'static str {
    match approval_state {
        TagApprovalState::Suggested => "suggested",
        TagApprovalState::Accepted => "accepted",
        TagApprovalState::Rejected => "rejected",
    }
}

fn usage_event_type_to_db(event_type: UsageEventType) -> &'static str {
    match event_type {
        UsageEventType::Played => "played",
        UsageEventType::Exported => "exported",
        UsageEventType::Dragged => "dragged",
        UsageEventType::Copied => "copied",
        UsageEventType::Used => "used",
    }
}

fn usage_event_type_from_db(value: &str) -> UsageEventType {
    match value {
        "exported" => UsageEventType::Exported,
        "dragged" => UsageEventType::Dragged,
        "copied" => UsageEventType::Copied,
        "used" => UsageEventType::Used,
        _ => UsageEventType::Played,
    }
}

fn usage_event_from_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<UsageEventRecord> {
    let project_id: Option<String> = row.get(2)?;
    Ok(UsageEventRecord {
        id: parse_uuid(row.get::<_, String>(0)?),
        asset_id: parse_uuid(row.get::<_, String>(1)?),
        project_id: project_id.map(parse_uuid),
        event_type: usage_event_type_from_db(&row.get::<_, String>(3)?),
        destination: row.get(4)?,
    })
}

fn normalize_term(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace([' ', '/', '-'], "_")
}

fn fts_query(value: &str) -> String {
    value
        .split_whitespace()
        .map(|token| format!("{}*", token.replace('"', "")))
        .collect::<Vec<_>>()
        .join(" ")
}

fn join_uuids(ids: &[Uuid]) -> String {
    ids.iter()
        .map(Uuid::to_string)
        .collect::<Vec<_>>()
        .join(",")
}

fn split_owner_and_assets(payload: &str) -> (Uuid, Vec<Uuid>) {
    let (owner, assets) = payload
        .split_once('|')
        .expect("undo payload contains owner and asset ids");
    let asset_ids = assets
        .split(',')
        .filter(|value| !value.is_empty())
        .map(|value| Uuid::parse_str(value).expect("undo payload contains valid asset uuid"))
        .collect();

    (
        Uuid::parse_str(owner).expect("undo payload contains valid owner uuid"),
        asset_ids,
    )
}

fn split_tag_owner_origin_and_assets(payload: &str) -> (Uuid, String, Vec<Uuid>) {
    let mut parts = payload.splitn(3, '|');
    let tag_id = parts.next().expect("undo payload contains tag id");
    let second = parts.next().expect("undo payload contains asset ids");
    let third = parts.next();
    let (origin, assets) = match third {
        Some(assets) => (second.to_string(), assets),
        None => ("manual".to_string(), second),
    };
    let asset_ids = assets
        .split(',')
        .filter(|value| !value.is_empty())
        .map(|value| Uuid::parse_str(value).expect("undo payload contains valid asset uuid"))
        .collect();

    (
        Uuid::parse_str(tag_id).expect("undo payload contains valid tag uuid"),
        origin,
        asset_ids,
    )
}

fn job_kind_to_db(kind: &JobKind) -> &'static str {
    match kind {
        JobKind::MetadataExtraction => "metadata_extraction",
        JobKind::Hashing => "hashing",
        JobKind::WaveformGeneration => "waveform_generation",
        JobKind::AudioAnalysis => "audio_analysis",
        JobKind::InstrumentDetection => "instrument_detection",
    }
}

fn job_kind_from_db(value: &str) -> JobKind {
    match value {
        "hashing" => JobKind::Hashing,
        "waveform_generation" => JobKind::WaveformGeneration,
        "audio_analysis" => JobKind::AudioAnalysis,
        "instrument_detection" => JobKind::InstrumentDetection,
        _ => JobKind::MetadataExtraction,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_types::{AvailabilityState, StorageMode};
    use std::fs;
    use std::path::PathBuf;

    use uuid::Uuid;

    #[test]
    fn nas_paths_are_not_valid_live_catalog_locations() {
        let valid = is_network_tolerant_catalog_path("/Volumes/TrueNAS/catalog.sqlite")
            .expect("absolute path is valid input");

        assert!(!valid);
    }

    #[test]
    fn catalog_creates_library_and_persists_it_after_reopen() {
        let catalog_path = unique_catalog_path("create-library");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Editor Library", "/Volumes/TrueNAS/SFX")
            .expect("create library");
        drop(catalog);

        let reopened = Catalog::open(&catalog_path).expect("reopen catalog");
        let loaded = reopened
            .get_library(library.id)
            .expect("load library")
            .expect("library exists");

        assert_eq!(loaded.name, "Editor Library");
        assert_eq!(loaded.media_root, "/Volumes/TrueNAS/SFX");
    }

    #[test]
    fn checkpoint_wal_flushes_writes_to_the_main_file_for_a_plain_copy() {
        let catalog_path = unique_catalog_path("checkpoint-wal");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        catalog
            .create_library("Checkpoint Test", "")
            .expect("create library");
        catalog.checkpoint_wal().expect("checkpoint wal");
        drop(catalog);

        // A copy of the main file alone (no -wal/-shm sidecars) must be a
        // fully valid, complete catalog — exactly what a raw fs::copy-based
        // migration relies on.
        let copy_path = unique_catalog_path("checkpoint-wal-copy");
        fs::copy(&catalog_path, &copy_path).expect("copy main db file");
        let copied = Catalog::open(&copy_path).expect("open copied catalog");
        assert_eq!(copied.list_libraries().expect("list").len(), 1);
    }

    #[test]
    fn open_or_init_library_file_creates_a_single_library_on_a_fresh_file() {
        let project_path = unique_catalog_path("project-fresh");
        let (catalog, library) =
            Catalog::open_or_init_library_file(&project_path, "My Library").expect("init project");

        assert_eq!(library.name, "My Library");
        assert_eq!(library.media_root, "");
        assert_eq!(catalog.list_libraries().expect("list").len(), 1);
    }

    #[test]
    fn open_or_init_library_file_reopens_the_existing_library_unchanged() {
        let project_path = unique_catalog_path("project-reopen");
        let (catalog, created) =
            Catalog::open_or_init_library_file(&project_path, "My Library").expect("init project");
        catalog
            .set_library_media_root(created.id, "/Users/example/Sounds")
            .expect("set media root");
        drop(catalog);

        // Passing a different name on reopen must not rename or duplicate
        // the existing library — a project file only ever gets its name
        // from the New Setup flow, once.
        let (reopened, library) =
            Catalog::open_or_init_library_file(&project_path, "Ignored Name").expect("reopen project");

        assert_eq!(library.id, created.id);
        assert_eq!(library.name, "My Library");
        assert_eq!(library.media_root, "/Users/example/Sounds");
        assert_eq!(reopened.list_libraries().expect("list").len(), 1);
    }

    #[test]
    fn set_library_media_root_updates_an_initially_empty_root() {
        let catalog_path = unique_catalog_path("set-media-root");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Home Studio", "")
            .expect("create library without a media root");
        assert_eq!(library.media_root, "");

        catalog
            .set_library_media_root(library.id, "/Volumes/TrueNAS/SFX")
            .expect("set media root");

        let loaded = catalog
            .get_library(library.id)
            .expect("load library")
            .expect("library exists");
        assert_eq!(loaded.media_root, "/Volumes/TrueNAS/SFX");
    }

    #[test]
    fn import_root_defaults_to_none_and_can_be_set_and_cleared() {
        let catalog_path = unique_catalog_path("set-import-root");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Home Studio", "/Volumes/TrueNAS/SFX")
            .expect("create library");
        assert_eq!(library.import_root, None);

        catalog
            .set_library_import_root(library.id, Some("/Users/editor/Drop"))
            .expect("set import root");
        let with_import_root = catalog
            .get_library(library.id)
            .expect("load library")
            .expect("library exists");
        assert_eq!(with_import_root.import_root.as_deref(), Some("/Users/editor/Drop"));

        catalog
            .set_library_import_root(library.id, Some("   "))
            .expect("clear import root with blank string");
        let cleared = catalog
            .get_library(library.id)
            .expect("load library")
            .expect("library exists");
        assert_eq!(cleared.import_root, None);
    }

    #[test]
    fn media_root_bookmark_defaults_to_none_and_can_be_set_and_cleared() {
        let catalog_path = unique_catalog_path("media-root-bookmark");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Home Studio", "/Volumes/TrueNAS/SFX")
            .expect("create library");
        assert_eq!(library.media_root_bookmark, None);

        catalog
            .set_library_media_root_bookmark(library.id, Some("base64bookmarkbytes"))
            .expect("set bookmark");
        let with_bookmark = catalog
            .get_library(library.id)
            .expect("load library")
            .expect("library exists");
        assert_eq!(
            with_bookmark.media_root_bookmark.as_deref(),
            Some("base64bookmarkbytes")
        );

        catalog
            .set_library_media_root_bookmark(library.id, None)
            .expect("clear bookmark");
        let cleared = catalog
            .get_library(library.id)
            .expect("load library")
            .expect("library exists");
        assert_eq!(cleared.media_root_bookmark, None);
    }

    #[test]
    fn list_libraries_returns_all_libraries_in_creation_order() {
        let catalog_path = unique_catalog_path("list-libraries");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let first = catalog
            .create_library("Home Studio", "/Volumes/TrueNAS/SFX")
            .expect("create first library");
        let second = catalog
            .create_library("Freelance Kit", "/Users/editor/Sounds")
            .expect("create second library");

        let libraries = catalog.list_libraries().expect("list libraries");

        assert_eq!(
            libraries
                .iter()
                .map(|library| library.id)
                .collect::<Vec<_>>(),
            vec![first.id, second.id]
        );
        assert_eq!(libraries[1].name, second.name);
    }

    #[test]
    fn add_folder_defaults_and_list_folders_orders_by_creation() {
        let catalog_path = unique_catalog_path("folders-basic");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Home Studio", "/Users/editor/Sounds")
            .expect("create library");

        let media_root_folder = catalog
            .add_folder(library.id, "/Users/editor/Sounds", None, "media_root")
            .expect("add media root folder");
        let sfx_folder = catalog
            .add_folder(library.id, "/Users/editor/SFX", Some("sound_effect"), "watched")
            .expect("add sfx folder");

        assert_eq!(media_root_folder.role, None);
        assert_eq!(media_root_folder.kind, "media_root");
        assert_eq!(sfx_folder.role, Some("sound_effect".to_string()));

        let folders = catalog.list_folders(library.id).expect("list folders");
        assert_eq!(
            folders.iter().map(|folder| folder.id).collect::<Vec<_>>(),
            vec![media_root_folder.id, sfx_folder.id]
        );
    }

    #[test]
    fn list_folders_is_scoped_to_its_own_library() {
        let catalog_path = unique_catalog_path("folders-scoped");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let one = catalog.create_library("One", "/one").expect("create one");
        let two = catalog.create_library("Two", "/two").expect("create two");
        catalog
            .add_folder(one.id, "/one/Music", Some("music"), "watched")
            .expect("add folder to one");
        catalog
            .add_folder(two.id, "/two/Foley", Some("foley"), "watched")
            .expect("add folder to two");

        assert_eq!(catalog.list_folders(one.id).expect("list one").len(), 1);
        assert_eq!(catalog.list_folders(two.id).expect("list two").len(), 1);
    }

    #[test]
    fn set_folder_role_updates_an_existing_folder() {
        let catalog_path = unique_catalog_path("folders-set-role");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Home Studio", "/sounds").expect("create library");
        let folder = catalog
            .add_folder(library.id, "/sounds/Misc", None, "watched")
            .expect("add folder");

        catalog
            .set_folder_role(folder.id, Some("ambience"))
            .expect("set role");

        let updated = catalog
            .list_folders(library.id)
            .expect("list folders")
            .into_iter()
            .find(|entry| entry.id == folder.id)
            .expect("folder still present");
        assert_eq!(updated.role, Some("ambience".to_string()));
    }

    #[test]
    fn remove_folder_deletes_only_the_targeted_folder() {
        let catalog_path = unique_catalog_path("folders-remove");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Home Studio", "/sounds").expect("create library");
        let keep = catalog
            .add_folder(library.id, "/sounds/Keep", None, "watched")
            .expect("add keep folder");
        let remove = catalog
            .add_folder(library.id, "/sounds/Remove", None, "watched")
            .expect("add remove folder");

        catalog.remove_folder(remove.id).expect("remove folder");

        let remaining = catalog.list_folders(library.id).expect("list folders");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, keep.id);
    }

    #[test]
    fn deleting_a_library_cascades_to_its_folders() {
        let catalog_path = unique_catalog_path("folders-cascade");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Home Studio", "/sounds").expect("create library");
        catalog
            .add_folder(library.id, "/sounds/Misc", None, "watched")
            .expect("add folder");

        catalog.delete_library(library.id).expect("delete library");

        assert_eq!(catalog.list_folders(library.id).expect("list folders").len(), 0);
    }

    #[test]
    fn record_folder_document_reports_true_only_for_a_genuinely_new_path() {
        let catalog_path = unique_catalog_path("folder-documents-new");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Home Studio", "/sounds").expect("create library");
        let folder = catalog
            .add_folder(library.id, "/sounds/Licence", Some("documents"), "documents")
            .expect("add folder");

        let first = catalog
            .record_folder_document(folder.id, library.id, "/sounds/Licence/receipt.pdf")
            .expect("record first");
        let second = catalog
            .record_folder_document(folder.id, library.id, "/sounds/Licence/receipt.pdf")
            .expect("record duplicate");

        assert!(first, "first sighting of a path must report true");
        assert!(!second, "re-polling the same path must not report true again");
        assert_eq!(catalog.list_folder_documents(folder.id).expect("list").len(), 1);
    }

    #[test]
    fn list_folder_documents_derives_filename_and_orders_by_discovery() {
        let catalog_path = unique_catalog_path("folder-documents-list");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Home Studio", "/sounds").expect("create library");
        let folder = catalog
            .add_folder(library.id, "/sounds/Licence", Some("documents"), "documents")
            .expect("add folder");

        catalog
            .record_folder_document(folder.id, library.id, "/sounds/Licence/a.pdf")
            .expect("record a");
        catalog
            .record_folder_document(folder.id, library.id, "/sounds/Licence/b.pdf")
            .expect("record b");

        let documents = catalog.list_folder_documents(folder.id).expect("list");
        assert_eq!(documents.len(), 2);
        assert_eq!(documents[0].filename, "a.pdf");
        assert_eq!(documents[1].filename, "b.pdf");
    }

    #[test]
    fn catalog_suppresses_duplicate_assets_by_content_hash() {
        let catalog_path = unique_catalog_path("duplicate-assets");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Editor Library", "/library")
            .expect("create library");

        let first = NewAssetRecord {
            library_id: library.id,
            original_filename: "impact.wav".to_string(),
            display_name: "impact".to_string(),
            path: AssetPath::Referenced("/packs/impact.wav".to_string()),
            storage_mode: StorageMode::Referenced,
            content_hash: Some("hash-1".to_string()),
            media_type: "sound_effect".to_string(),
            file_size: 123,
            availability_state: AvailabilityState::Local,
        };

        let (first_asset, first_is_new) = catalog.register_asset(first.clone()).expect("first import");
        let (duplicate_asset, duplicate_is_new) =
            catalog.register_asset(first).expect("duplicate import");

        assert!(first_is_new);
        assert!(!duplicate_is_new, "a content-hash duplicate must not report itself as new");
        assert_eq!(first_asset.id, duplicate_asset.id);
        assert_eq!(catalog.list_assets(library.id).expect("assets").len(), 1);
    }

    #[test]
    fn job_queue_persists_pending_work_after_reopen() {
        let catalog_path = unique_catalog_path("jobs");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Jobs", "/library").expect("library");
        let (asset, _) = catalog
            .register_asset(NewAssetRecord {
                library_id: library.id,
                original_filename: "tone.wav".to_string(),
                display_name: "tone".to_string(),
                path: AssetPath::Managed("Media/00/tone.wav".to_string()),
                storage_mode: StorageMode::Managed,
                content_hash: Some("hash-2".to_string()),
                media_type: "sound_effect".to_string(),
                file_size: 12,
                availability_state: AvailabilityState::Local,
            })
            .expect("asset");

        catalog
            .enqueue_job(asset.id, JobKind::MetadataExtraction, 10)
            .expect("enqueue");
        drop(catalog);

        let reopened = Catalog::open(&catalog_path).expect("reopen catalog");
        let job = reopened
            .next_pending_job()
            .expect("query job")
            .expect("job");

        assert_eq!(job.asset_id, asset.id);
        assert_eq!(job.kind, JobKind::MetadataExtraction);
    }

    #[test]
    fn pending_job_count_reports_only_matching_kind() {
        let catalog_path = unique_catalog_path("job-counts");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Jobs", "/library").expect("library");
        let asset = test_asset(&catalog, library.id, "tone.wav", "hash-job-count");

        catalog
            .enqueue_job(asset.id, JobKind::WaveformGeneration, 30)
            .expect("enqueue waveform");
        catalog
            .enqueue_job(asset.id, JobKind::Hashing, 20)
            .expect("enqueue hashing");

        assert_eq!(
            catalog
                .pending_job_count(JobKind::WaveformGeneration)
                .expect("count"),
            1
        );
        assert_eq!(
            catalog.pending_job_count(JobKind::Hashing).expect("count"),
            1
        );
        assert_eq!(
            catalog
                .pending_job_count(JobKind::MetadataExtraction)
                .expect("count"),
            0
        );
    }

    #[test]
    fn pending_job_count_for_library_only_counts_that_librarys_assets() {
        let catalog_path = unique_catalog_path("job-counts-per-library");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let first_library = catalog.create_library("First", "/first").expect("library");
        let second_library = catalog.create_library("Second", "/second").expect("library");
        let first_asset = test_asset(&catalog, first_library.id, "one.wav", "hash-first-lib");
        let second_asset = test_asset(&catalog, second_library.id, "two.wav", "hash-second-lib");

        catalog
            .enqueue_job(first_asset.id, JobKind::AudioAnalysis, 40)
            .expect("enqueue first");
        catalog
            .enqueue_job(second_asset.id, JobKind::AudioAnalysis, 40)
            .expect("enqueue second");

        assert_eq!(
            catalog
                .pending_job_count_for_library(first_library.id, JobKind::AudioAnalysis)
                .expect("count"),
            1
        );
        assert_eq!(
            catalog
                .pending_job_count_for_library(second_library.id, JobKind::AudioAnalysis)
                .expect("count"),
            1
        );
        assert_eq!(
            catalog
                .pending_job_count_for_library(first_library.id, JobKind::MetadataExtraction)
                .expect("count"),
            0
        );
    }

    #[test]
    fn job_state_counts_for_library_splits_pending_failed_and_completed() {
        let catalog_path = unique_catalog_path("job-state-counts");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Counts", "/counts").expect("library");
        let pending_asset = test_asset(&catalog, library.id, "pending.wav", "hash-counts-pending");
        let failed_asset = test_asset(&catalog, library.id, "failed.wav", "hash-counts-failed");
        let done_asset = test_asset(&catalog, library.id, "done.wav", "hash-counts-done");

        catalog
            .enqueue_job(pending_asset.id, JobKind::AudioAnalysis, 40)
            .expect("enqueue pending");
        let failed_job = catalog
            .enqueue_job(failed_asset.id, JobKind::AudioAnalysis, 40)
            .expect("enqueue failed");
        catalog.fail_job(failed_job.id, "test error").expect("fail job");
        let done_job = catalog
            .enqueue_job(done_asset.id, JobKind::AudioAnalysis, 40)
            .expect("enqueue done");
        catalog.complete_job(done_job.id).expect("complete job");

        let counts = catalog
            .job_state_counts_for_library(library.id, JobKind::AudioAnalysis)
            .expect("job state counts");

        assert_eq!(counts.pending, 1);
        assert_eq!(counts.failed, 1);
        assert_eq!(counts.completed, 1);
    }

    #[test]
    fn failed_job_extensions_for_library_dedupes_and_ignores_other_states() {
        let catalog_path = unique_catalog_path("job-failed-extensions");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Extensions", "/extensions").expect("library");
        let mp3_asset = test_asset(&catalog, library.id, "one.mp3", "hash-ext-mp3-1");
        let mp3_asset_two = test_asset(&catalog, library.id, "two.MP3", "hash-ext-mp3-2");
        let aif_asset = test_asset(&catalog, library.id, "three.aif", "hash-ext-aif");
        let pending_asset = test_asset(&catalog, library.id, "four.wav", "hash-ext-pending");

        for asset in [&mp3_asset, &mp3_asset_two, &aif_asset] {
            let job = catalog
                .enqueue_job(asset.id, JobKind::AudioAnalysis, 40)
                .expect("enqueue");
            catalog.fail_job(job.id, "test error").expect("fail job");
        }
        catalog
            .enqueue_job(pending_asset.id, JobKind::AudioAnalysis, 40)
            .expect("enqueue pending, should not appear");

        let extensions = catalog
            .failed_job_extensions_for_library(library.id, JobKind::AudioAnalysis)
            .expect("failed extensions");

        assert_eq!(extensions, vec![".aif".to_string(), ".mp3".to_string()]);
    }

    #[test]
    fn claim_pending_jobs_marks_them_processing_so_a_second_claim_finds_nothing() {
        let catalog_path = unique_catalog_path("job-claim");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Jobs", "/library").expect("library");
        let asset = test_asset(&catalog, library.id, "tone.wav", "hash-job-claim");
        catalog
            .enqueue_job(asset.id, JobKind::AudioAnalysis, 40)
            .expect("enqueue");

        let first_claim = catalog
            .claim_pending_jobs(JobKind::AudioAnalysis, 10)
            .expect("first claim");
        assert_eq!(first_claim.len(), 1);

        let second_claim = catalog
            .claim_pending_jobs(JobKind::AudioAnalysis, 10)
            .expect("second claim");
        assert!(
            second_claim.is_empty(),
            "a job already claimed as 'processing' must not be claimable again"
        );

        assert!(catalog
            .pending_jobs_of_kind(JobKind::AudioAnalysis, 10)
            .expect("pending jobs")
            .is_empty());
    }

    #[test]
    fn claim_pending_waveform_jobs_skips_assets_with_a_pending_audio_analysis_twin() {
        let catalog_path = unique_catalog_path("waveform-claim-skip");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Jobs", "/library").expect("library");

        // Backfilled together, like a real bulk import: both jobs queued for
        // this asset. Its AudioAnalysis pass will decode the file and fill
        // the waveform cache as a side effect, so its WaveformGeneration job
        // shouldn't be claimed (and independently decoded) ahead of that.
        let both_pending = test_asset(&catalog, library.id, "both.wav", "hash-both-pending");
        catalog
            .enqueue_job(both_pending.id, JobKind::WaveformGeneration, 40)
            .expect("enqueue waveform");
        catalog
            .enqueue_job(both_pending.id, JobKind::AudioAnalysis, 40)
            .expect("enqueue analysis");

        // The case claim_pending_waveform_jobs exists for: a library
        // retrofitted with waveform caching after analysis already ran, so
        // nothing else is ever going to fill this one's waveform cache.
        let waveform_only = test_asset(&catalog, library.id, "solo.wav", "hash-waveform-only");
        catalog
            .enqueue_job(waveform_only.id, JobKind::WaveformGeneration, 40)
            .expect("enqueue waveform");

        let claimed = catalog
            .claim_pending_waveform_jobs(10)
            .expect("claim waveform jobs");

        assert_eq!(
            claimed.len(),
            1,
            "only the asset with no pending/processing AudioAnalysis twin should be claimed"
        );
        assert_eq!(claimed[0].asset_id, waveform_only.id);

        // The skipped job is untouched — still pending, still claimable once
        // its analysis twin is gone (completed, failed, whatever).
        let still_pending = catalog
            .pending_jobs_of_kind(JobKind::WaveformGeneration, 10)
            .expect("pending waveform jobs");
        assert_eq!(still_pending.len(), 1);
        assert_eq!(still_pending[0].asset_id, both_pending.id);
    }

    #[test]
    fn reset_stuck_processing_jobs_makes_an_abandoned_claim_claimable_again() {
        let catalog_path = unique_catalog_path("job-reset-stuck");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Jobs", "/library").expect("library");
        let asset = test_asset(&catalog, library.id, "tone.wav", "hash-job-reset-stuck");
        catalog
            .enqueue_job(asset.id, JobKind::AudioAnalysis, 40)
            .expect("enqueue");
        catalog
            .claim_pending_jobs(JobKind::AudioAnalysis, 10)
            .expect("claim");
        // Simulate a claim genuinely abandoned a while ago (dead process,
        // crash) rather than one still legitimately in flight — see the age
        // floor on reset_stuck_processing_jobs itself.
        catalog
            .connection
            .execute(
                "UPDATE background_jobs SET updated_at = ?1 WHERE kind = 'audio_analysis'",
                params![(Utc::now() - chrono::Duration::minutes(10)).to_rfc3339()],
            )
            .expect("backdate claim");

        let reset = catalog
            .reset_stuck_processing_jobs(JobKind::AudioAnalysis)
            .expect("reset stuck jobs");
        assert_eq!(reset, 1);

        let reclaimed = catalog
            .claim_pending_jobs(JobKind::AudioAnalysis, 10)
            .expect("reclaim");
        assert_eq!(
            reclaimed.len(),
            1,
            "a job left 'processing' by an abandoned claim must become claimable again after reset"
        );
    }

    #[test]
    fn reset_stuck_processing_jobs_leaves_a_freshly_claimed_job_alone() {
        // A claim that's only seconds old is most likely still legitimately
        // in flight (a slow decode under load, not an abandoned process) —
        // resetting it out from under the worker still holding it would let
        // the very next claim cycle re-claim (and start over on) the same
        // job before the original attempt ever got a chance to finish. See
        // the reset_stuck_processing_jobs doc comment for the full story
        // (this was a real, observed cause of jobs that never completed).
        let catalog_path = unique_catalog_path("job-reset-stuck-fresh");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Jobs", "/library").expect("library");
        let asset = test_asset(&catalog, library.id, "tone.wav", "hash-job-reset-stuck-fresh");
        catalog
            .enqueue_job(asset.id, JobKind::AudioAnalysis, 40)
            .expect("enqueue");
        catalog
            .claim_pending_jobs(JobKind::AudioAnalysis, 10)
            .expect("claim");

        let reset = catalog
            .reset_stuck_processing_jobs(JobKind::AudioAnalysis)
            .expect("reset stuck jobs");
        assert_eq!(reset, 0, "a claim only seconds old must not be treated as abandoned");
    }

    #[test]
    fn requeue_failed_jobs_respects_the_attempt_cap() {
        let catalog_path = unique_catalog_path("job-requeue");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Jobs", "/library").expect("library");
        let asset = test_asset(&catalog, library.id, "tone.wav", "hash-job-requeue");
        let job = catalog
            .enqueue_job(asset.id, JobKind::AudioAnalysis, 40)
            .expect("enqueue");

        catalog.fail_job(job.id, "test error").expect("fail once");
        let requeued = catalog.requeue_failed_jobs(3).expect("requeue");
        assert_eq!(requeued, 1);
        assert_eq!(
            catalog
                .pending_jobs_of_kind(JobKind::AudioAnalysis, 10)
                .expect("pending jobs")
                .len(),
            1
        );

        // Fail it two more times (3 attempts total) — the cap should stop requeuing.
        catalog.fail_job(job.id, "test error").expect("fail twice");
        catalog.fail_job(job.id, "test error").expect("fail thrice");
        let requeued_after_cap = catalog.requeue_failed_jobs(3).expect("requeue at cap");
        assert_eq!(requeued_after_cap, 0);
        assert!(catalog
            .pending_jobs_of_kind(JobKind::AudioAnalysis, 10)
            .expect("pending jobs")
            .is_empty());
    }

    #[test]
    fn retry_failed_jobs_for_library_ignores_the_attempt_cap_and_resets_attempts() {
        let catalog_path = unique_catalog_path("job-manual-retry");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Jobs", "/library").expect("library");
        let other_library = catalog.create_library("Other", "/other").expect("library");
        let asset = test_asset(&catalog, library.id, "tone.wav", "hash-job-manual-retry");
        let other_asset = test_asset(&catalog, other_library.id, "tone2.wav", "hash-job-manual-retry-2");
        let job = catalog
            .enqueue_job(asset.id, JobKind::AudioAnalysis, 40)
            .expect("enqueue");
        let other_job = catalog
            .enqueue_job(other_asset.id, JobKind::AudioAnalysis, 40)
            .expect("enqueue other library");

        // Exhaust the normal 3-attempt cap for both jobs (the second library's
        // job stays failed throughout — it's here to prove retry is scoped
        // to the target library, not to exercise the cap itself).
        catalog.fail_job(job.id, "test error").expect("fail once");
        catalog.fail_job(job.id, "test error").expect("fail twice");
        catalog.fail_job(job.id, "test error").expect("fail thrice");
        catalog.fail_job(other_job.id, "test error").expect("fail once");
        catalog.fail_job(other_job.id, "test error").expect("fail twice");
        catalog.fail_job(other_job.id, "test error").expect("fail thrice");

        let retried = catalog
            .retry_failed_jobs_for_library(library.id, JobKind::AudioAnalysis)
            .expect("manual retry");
        assert_eq!(retried, 1, "only this library's failed job should be retried");

        let pending = catalog
            .pending_jobs_of_kind(JobKind::AudioAnalysis, 10)
            .expect("pending jobs");
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].id, job.id);

        // Attempts reset, so it survives the normal cap again if it fails once
        // more — other_job stays excluded since it's still at 3 attempts.
        catalog.fail_job(job.id, "test error").expect("fail after manual retry");
        assert_eq!(catalog.requeue_failed_jobs(3).expect("requeue after retry"), 1);
    }

    #[test]
    fn completing_a_job_removes_it_from_the_pending_queue() {
        let catalog_path = unique_catalog_path("job-complete");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Jobs", "/library").expect("library");
        let asset = test_asset(&catalog, library.id, "tone.wav", "hash-job-complete");
        let job = catalog
            .enqueue_job(asset.id, JobKind::MetadataExtraction, 10)
            .expect("enqueue");

        assert_eq!(
            catalog
                .pending_jobs_of_kind(JobKind::MetadataExtraction, 10)
                .expect("pending jobs")
                .len(),
            1
        );

        catalog.complete_job(job.id).expect("complete job");

        assert!(catalog
            .pending_jobs_of_kind(JobKind::MetadataExtraction, 10)
            .expect("pending jobs")
            .is_empty());
    }

    #[test]
    fn failing_a_job_increments_attempts_and_clears_it_from_pending() {
        let catalog_path = unique_catalog_path("job-fail");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Jobs", "/library").expect("library");
        let asset = test_asset(&catalog, library.id, "tone.wav", "hash-job-fail");
        let job = catalog
            .enqueue_job(asset.id, JobKind::MetadataExtraction, 10)
            .expect("enqueue");

        catalog.fail_job(job.id, "test error").expect("fail job");

        assert!(catalog
            .pending_jobs_of_kind(JobKind::MetadataExtraction, 10)
            .expect("pending jobs")
            .is_empty());
    }

    #[test]
    fn completing_pending_jobs_for_asset_only_touches_matching_asset_and_kind() {
        let catalog_path = unique_catalog_path("job-complete-for-asset");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Jobs", "/library").expect("library");
        let first = test_asset(&catalog, library.id, "one.wav", "hash-complete-1");
        let second = test_asset(&catalog, library.id, "two.wav", "hash-complete-2");

        catalog
            .enqueue_job(first.id, JobKind::WaveformGeneration, 30)
            .expect("enqueue waveform");
        catalog
            .enqueue_job(first.id, JobKind::MetadataExtraction, 10)
            .expect("enqueue metadata");
        catalog
            .enqueue_job(second.id, JobKind::WaveformGeneration, 30)
            .expect("enqueue waveform for other asset");

        let completed = catalog
            .complete_pending_jobs_for_asset(first.id, JobKind::WaveformGeneration)
            .expect("complete");

        assert_eq!(completed, 1);
        assert_eq!(
            catalog
                .pending_job_count(JobKind::WaveformGeneration)
                .expect("count"),
            1
        );
        assert_eq!(
            catalog
                .pending_job_count(JobKind::MetadataExtraction)
                .expect("count"),
            1
        );
    }

    #[test]
    fn completing_pending_jobs_for_asset_also_completes_an_already_claimed_processing_job() {
        // Regression for the standalone-waveform-job path: claim_pending_waveform_jobs
        // marks a job 'processing' before process_one_waveform_job does the real
        // decode/build work, so by the time persist_waveform_payload calls this to
        // record success, the row is 'processing', not 'pending'. Matching only
        // 'pending' meant that job was never actually completed — it sat
        // 'processing' until reset_stuck_processing_jobs's timeout put it back to
        // 'pending', making it claimable (and redecoded, and rebuilt) again, forever.
        let catalog_path = unique_catalog_path("job-complete-processing");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Jobs", "/library").expect("library");
        let asset = test_asset(&catalog, library.id, "one.wav", "hash-complete-processing");

        catalog
            .enqueue_job(asset.id, JobKind::WaveformGeneration, 30)
            .expect("enqueue waveform");
        let claimed = catalog
            .claim_pending_waveform_jobs(10)
            .expect("claim waveform jobs");
        assert_eq!(claimed.len(), 1, "job should be claimed into 'processing'");

        let completed = catalog
            .complete_pending_jobs_for_asset(asset.id, JobKind::WaveformGeneration)
            .expect("complete");

        assert_eq!(completed, 1);
        assert_eq!(
            catalog
                .latest_job_state_for_asset(asset.id, JobKind::WaveformGeneration)
                .expect("state")
                .map(|(state, _)| state),
            Some("completed".to_string())
        );
    }

    #[test]
    fn embedded_metadata_round_trips_through_get_asset() {
        let catalog_path = unique_catalog_path("embedded-metadata");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Org", "/library").expect("library");
        let asset = test_asset(&catalog, library.id, "tone.wav", "hash-embedded");

        assert_eq!(
            catalog.get_asset(asset.id).expect("asset").expect("some"),
            AssetRecord {
                embedded_title: None,
                embedded_genre: None,
                embedded_comment: None,
                ..asset.clone()
            }
        );

        catalog
            .set_embedded_metadata(
                asset.id,
                Some("Rain Loop".to_string()),
                Some("Ambience".to_string()),
                None,
            )
            .expect("set embedded metadata");

        let reloaded = catalog.get_asset(asset.id).expect("asset").expect("some");
        assert_eq!(reloaded.embedded_title, Some("Rain Loop".to_string()));
        assert_eq!(reloaded.embedded_genre, Some("Ambience".to_string()));
        assert_eq!(reloaded.embedded_comment, None);
    }

    #[test]
    fn starter_taxonomy_seeds_system_tags_once() {
        let catalog_path = unique_catalog_path("taxonomy");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");

        catalog.seed_starter_taxonomy().expect("seed");
        catalog.seed_starter_taxonomy().expect("seed again");
        let tags = catalog.list_tags().expect("tags");

        assert!(tags
            .iter()
            .any(|tag| tag.name == "Impact" && tag.facet == Some("action".to_string())));
        assert!(tags
            .iter()
            .any(|tag| tag.name == "Music" && tag.facet == Some("media_type".to_string())));
        assert_eq!(tags.iter().filter(|tag| tag.name == "Impact").count(), 1);
    }

    #[test]
    fn bulk_tagging_assets_is_undoable() {
        let catalog_path = unique_catalog_path("tag-undo");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Org", "/library").expect("library");
        let first = test_asset(&catalog, library.id, "one.wav", "hash-one");
        let second = test_asset(&catalog, library.id, "two.wav", "hash-two");
        let tag = catalog.create_tag("Impact", "action", true).expect("tag");

        let undo_id = catalog
            .apply_tag_to_assets(&[first.id, second.id], tag.id, TagOrigin::Manual)
            .expect("apply tag");

        assert_eq!(
            catalog.tags_for_asset(first.id).expect("first tags").len(),
            1
        );
        assert_eq!(
            catalog
                .tags_for_asset(second.id)
                .expect("second tags")
                .len(),
            1
        );

        catalog.undo(undo_id).expect("undo");

        assert!(catalog
            .tags_for_asset(first.id)
            .expect("first tags")
            .is_empty());
        assert!(catalog
            .tags_for_asset(second.id)
            .expect("second tags")
            .is_empty());
    }

    #[test]
    fn bulk_tagging_assets_can_be_redone_after_undo() {
        let catalog_path = unique_catalog_path("tag-redo");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Org", "/library").expect("library");
        let first = test_asset(&catalog, library.id, "one.wav", "hash-one");
        let second = test_asset(&catalog, library.id, "two.wav", "hash-two");
        let tag = catalog.create_tag("Impact", "action", true).expect("tag");
        let undo_id = catalog
            .apply_tag_to_assets(&[first.id, second.id], tag.id, TagOrigin::Manual)
            .expect("apply tag");

        catalog.undo(undo_id).expect("undo");
        catalog.redo(undo_id).expect("redo");

        assert_eq!(
            catalog.tags_for_asset(first.id).expect("first tags"),
            vec![tag.clone()]
        );
        assert_eq!(
            catalog.tags_for_asset(second.id).expect("second tags"),
            vec![tag]
        );
    }

    #[test]
    fn removing_a_tag_is_undoable_and_redoable() {
        let catalog_path = unique_catalog_path("tag-remove");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Org", "/library").expect("library");
        let asset = test_asset(&catalog, library.id, "one.wav", "hash-one");
        let tag = catalog.create_tag("Impact", "action", true).expect("tag");
        catalog
            .apply_tag_to_assets(&[asset.id], tag.id, TagOrigin::Manual)
            .expect("apply tag");

        let undo_id = catalog
            .remove_tag_from_asset(asset.id, tag.id)
            .expect("remove tag");

        assert!(catalog.tags_for_asset(asset.id).expect("tags").is_empty());

        catalog.undo(undo_id).expect("undo");
        assert_eq!(
            catalog.tags_for_asset(asset.id).expect("tags"),
            vec![tag.clone()]
        );

        catalog.redo(undo_id).expect("redo");
        assert!(catalog.tags_for_asset(asset.id).expect("tags").is_empty());
    }

    #[test]
    fn redo_preserves_bulk_tag_origin() {
        let catalog_path = unique_catalog_path("tag-redo-origin");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Org", "/library").expect("library");
        let asset = test_asset(&catalog, library.id, "one.wav", "hash-one");
        let tag = catalog.create_tag("Impact", "action", true).expect("tag");
        let undo_id = catalog
            .apply_tag_to_assets(&[asset.id], tag.id, TagOrigin::UserCorrection)
            .expect("apply tag");

        catalog.undo(undo_id).expect("undo");
        catalog.redo(undo_id).expect("redo");

        let origin: String = catalog
            .connection
            .query_row(
                "SELECT origin FROM asset_tags WHERE asset_id = ?1 AND tag_id = ?2",
                params![asset.id.to_string(), tag.id.to_string()],
                |row| row.get(0),
            )
            .expect("origin");
        assert_eq!(origin, "user_correction");
    }

    #[test]
    fn project_collection_membership_and_favorite_state_are_undoable() {
        let catalog_path = unique_catalog_path("collection-favorite");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Org", "/library").expect("library");
        let asset = test_asset(&catalog, library.id, "hit.wav", "hash-hit");
        let project = catalog
            .create_collection(library.id, "Film Trailer", CollectionType::Project)
            .expect("project");

        let membership_undo = catalog
            .add_assets_to_collection(project.id, &[asset.id])
            .expect("membership");
        catalog
            .set_asset_flags(asset.id, Some(true), Some(ReviewState::Reviewed))
            .expect("flags");
        let updated = catalog.get_asset(asset.id).expect("asset").expect("exists");

        assert!(updated.favorite);
        assert_eq!(updated.review_state, ReviewState::Reviewed);
        assert_eq!(
            catalog
                .assets_in_collection(project.id)
                .expect("collection assets")
                .len(),
            1
        );

        catalog.undo(membership_undo).expect("undo membership");

        assert!(catalog
            .assets_in_collection(project.id)
            .expect("collection assets")
            .is_empty());
    }

    #[test]
    fn project_collection_membership_can_be_redone_after_undo() {
        let catalog_path = unique_catalog_path("collection-redo");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Org", "/library").expect("library");
        let asset = test_asset(&catalog, library.id, "hit.wav", "hash-hit");
        let project = catalog
            .create_collection(library.id, "Film Trailer", CollectionType::Project)
            .expect("project");
        let undo_id = catalog
            .add_assets_to_collection(project.id, &[asset.id])
            .expect("membership");

        catalog.undo(undo_id).expect("undo");
        catalog.redo(undo_id).expect("redo");

        assert_eq!(
            catalog
                .assets_in_collection(project.id)
                .expect("collection assets")
                .iter()
                .map(|asset| asset.id)
                .collect::<Vec<_>>(),
            vec![asset.id]
        );
    }

    fn test_asset(catalog: &Catalog, library_id: Uuid, filename: &str, hash: &str) -> AssetRecord {
        catalog
            .register_asset(NewAssetRecord {
                library_id,
                original_filename: filename.to_string(),
                display_name: filename.trim_end_matches(".wav").to_string(),
                path: AssetPath::Referenced(format!("/fixtures/{filename}")),
                storage_mode: StorageMode::Referenced,
                content_hash: Some(hash.to_string()),
                media_type: "sound_effect".to_string(),
                file_size: 10,
                availability_state: AvailabilityState::Local,
            })
            .expect("asset")
            .0
    }

    #[test]
    fn full_text_search_finds_assets_by_display_name_and_tag_filter() {
        let catalog_path = unique_catalog_path("fts-search");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Search", "/library")
            .expect("library");
        let impact = test_asset(&catalog, library.id, "dark-impact.wav", "hash-dark");
        let ambience = test_asset(&catalog, library.id, "room-tone.wav", "hash-room");
        let tag = catalog.create_tag("Impact", "action", true).expect("tag");
        catalog
            .apply_tag_to_assets(&[impact.id], tag.id, TagOrigin::Manual)
            .expect("tag");

        let text_results = catalog
            .search_assets(library.id, AssetSearchQuery::text("dark"))
            .expect("search");
        let filtered_results = catalog
            .search_assets(library.id, AssetSearchQuery::text("").with_tag(tag.id))
            .expect("filtered");

        assert_eq!(
            text_results
                .iter()
                .map(|asset| asset.id)
                .collect::<Vec<_>>(),
            vec![impact.id]
        );
        assert_eq!(
            filtered_results
                .iter()
                .map(|asset| asset.id)
                .collect::<Vec<_>>(),
            vec![impact.id]
        );
        assert!(!filtered_results.iter().any(|asset| asset.id == ambience.id));
    }

    #[test]
    fn search_combines_text_media_type_and_accepted_tag_filters() {
        let catalog_path = unique_catalog_path("combined-search");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Search", "/library")
            .expect("library");
        let matching = test_asset(&catalog, library.id, "dark-impact.wav", "hash-dark-impact");
        let wrong_media = catalog
            .register_asset(NewAssetRecord {
                library_id: library.id,
                original_filename: "dark-loop.wav".to_string(),
                display_name: "dark-loop".to_string(),
                path: AssetPath::Referenced("/fixtures/dark-loop.wav".to_string()),
                storage_mode: StorageMode::Referenced,
                content_hash: Some("hash-dark-loop".to_string()),
                media_type: "music_loop".to_string(),
                file_size: 10,
                availability_state: AvailabilityState::Local,
            })
            .expect("asset")
            .0;
        let untagged = test_asset(
            &catalog,
            library.id,
            "dark-untagged.wav",
            "hash-dark-untagged",
        );
        let tag = catalog.create_tag("Impact", "action", true).expect("tag");
        catalog
            .apply_tag_to_assets(&[matching.id, wrong_media.id], tag.id, TagOrigin::Manual)
            .expect("tag");

        let results = catalog
            .search_assets(
                library.id,
                AssetSearchQuery::text("dark")
                    .with_media_type("sound_effect")
                    .with_tag(tag.id),
            )
            .expect("search");

        assert_eq!(
            results.iter().map(|asset| asset.id).collect::<Vec<_>>(),
            vec![matching.id]
        );
        assert!(!results.iter().any(|asset| asset.id == wrong_media.id));
        assert!(!results.iter().any(|asset| asset.id == untagged.id));
    }

    #[test]
    #[ignore = "profiles 100,000-asset search explicitly; set DARKWAVE_LARGE_CATALOG_SEARCH_MAX_MS to enforce a timing budget"]
    fn large_catalog_search_profile_exercises_one_hundred_thousand_assets() {
        let catalog_path = unique_catalog_path("large-catalog-search");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Large Search", "/library")
            .expect("library");
        let tag = catalog.create_tag("Impact", "action", true).expect("tag");
        let target_index = 90_000;
        let target_id = Uuid::new_v4();

        catalog
            .connection
            .execute_batch("BEGIN IMMEDIATE")
            .expect("begin bulk insert");
        {
            let mut insert = catalog
                .connection
                .prepare(
                    "INSERT INTO assets (
                        id, library_id, original_filename, display_name, referenced_path,
                        storage_mode, media_type, file_size, availability_state, date_added, last_seen
                    ) VALUES (?1, ?2, ?3, ?4, ?5, 'referenced', ?6, ?7, 'local', ?8, ?8)",
                )
                .expect("prepare insert");

            for index in 0..100_000 {
                let id = if index == target_index {
                    target_id
                } else {
                    Uuid::new_v4()
                };
                let display_name = if index == target_index {
                    "dark benchmark impact".to_string()
                } else {
                    format!("ambient pad {index}")
                };
                let media_type = if index % 10 == 0 {
                    "sound_effect"
                } else {
                    "music_loop"
                };
                let original_filename = format!("asset-{index:06}.wav");
                let referenced_path = format!("/fixtures/{original_filename}");

                insert
                    .execute(params![
                        id.to_string(),
                        library.id.to_string(),
                        original_filename,
                        display_name,
                        referenced_path,
                        media_type,
                        10_i64,
                        "2026-01-01T00:00:00Z",
                    ])
                    .expect("insert asset");
            }
        }
        catalog
            .connection
            .execute_batch("COMMIT")
            .expect("commit bulk insert");
        catalog
            .apply_tag_to_assets(&[target_id], tag.id, TagOrigin::Manual)
            .expect("tag target");

        let started = std::time::Instant::now();
        let results = catalog
            .search_assets(
                library.id,
                AssetSearchQuery::text("dark benchmark")
                    .with_media_type("sound_effect")
                    .with_tag(tag.id),
            )
            .expect("search");
        let elapsed = started.elapsed();

        eprintln!(
            "100k catalog search returned {} row(s) in {} ms",
            results.len(),
            elapsed.as_millis()
        );
        assert_eq!(
            results.iter().map(|asset| asset.id).collect::<Vec<_>>(),
            vec![target_id]
        );

        if let Ok(max_ms) = std::env::var("DARKWAVE_LARGE_CATALOG_SEARCH_MAX_MS") {
            let max_ms = max_ms.parse::<u128>().expect("valid millisecond budget");
            assert!(
                elapsed.as_millis() <= max_ms,
                "search took {} ms, budget was {max_ms} ms",
                elapsed.as_millis()
            );
        }
    }

    #[test]
    fn suggested_tags_can_be_accepted_or_rejected_without_reappearing_as_pending() {
        let catalog_path = unique_catalog_path("suggestions");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Suggestions", "/library")
            .expect("library");
        let asset = test_asset(&catalog, library.id, "metal-hit.wav", "hash-metal-hit");
        let tag = catalog.create_tag("Impact", "action", true).expect("tag");

        catalog
            .suggest_tag_for_asset(asset.id, tag.id, TagOrigin::Filename, 0.82)
            .expect("suggest");
        assert_eq!(
            catalog
                .pending_suggested_tags(asset.id)
                .expect("pending")
                .len(),
            1
        );

        catalog
            .set_tag_approval(
                asset.id,
                tag.id,
                TagOrigin::Filename,
                TagApprovalState::Rejected,
            )
            .expect("reject");
        catalog
            .suggest_tag_for_asset(asset.id, tag.id, TagOrigin::Filename, 0.91)
            .expect("suggest again");

        assert!(catalog
            .pending_suggested_tags(asset.id)
            .expect("pending")
            .is_empty());
    }

    #[test]
    fn smart_collection_stores_visible_query_definition() {
        let catalog_path = unique_catalog_path("smart-collection");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Smart", "/library")
            .expect("library");
        let query = AssetSearchQuery::text("dark").with_media_type("sound_effect");

        let collection = catalog
            .create_smart_collection(library.id, "Dark SFX", &query)
            .expect("smart collection");
        let loaded = catalog
            .get_collection(collection.id)
            .expect("load")
            .expect("exists");

        assert_eq!(loaded.collection_type, CollectionType::Smart);
        assert!(loaded
            .query_definition
            .expect("query")
            .contains("sound_effect"));
    }

    #[test]
    fn search_assets_filters_by_duration_and_bpm_range() {
        let catalog_path = unique_catalog_path("search-ranges");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Ranges", "/library").expect("library");
        let short_slow = test_asset(&catalog, library.id, "short.wav", "hash-short");
        let long_fast = test_asset(&catalog, library.id, "long.wav", "hash-long");

        catalog
            .set_audio_analysis(
                short_slow.id,
                AudioAnalysisUpdate {
                    duration_ms: Some(1_000),
                    bpm: Some(80.0),
                    ..Default::default()
                },
            )
            .expect("analysis short");
        catalog
            .set_audio_analysis(
                long_fast.id,
                AudioAnalysisUpdate {
                    duration_ms: Some(10_000),
                    bpm: Some(160.0),
                    ..Default::default()
                },
            )
            .expect("analysis long");

        let results = catalog
            .search_assets(
                library.id,
                AssetSearchQuery {
                    duration_min_ms: Some(5_000),
                    bpm_min: Some(120.0),
                    ..AssetSearchQuery::text("")
                },
            )
            .expect("search");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, long_fast.id);
    }

    #[test]
    fn assets_in_smart_collection_evaluates_the_stored_query() {
        let catalog_path = unique_catalog_path("smart-collection-eval");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("SmartEval", "/library")
            .expect("library");
        let matching = test_asset(&catalog, library.id, "match.wav", "hash-match");
        let _non_matching = test_asset(&catalog, library.id, "other.wav", "hash-other");

        catalog
            .set_audio_analysis(
                matching.id,
                AudioAnalysisUpdate {
                    bpm: Some(140.0),
                    ..Default::default()
                },
            )
            .expect("analysis");

        let query = AssetSearchQuery {
            bpm_min: Some(100.0),
            ..AssetSearchQuery::text("")
        };
        let collection = catalog
            .create_smart_collection(library.id, "Fast", &query)
            .expect("smart collection");

        let results = catalog
            .assets_in_smart_collection(collection.id)
            .expect("evaluate");

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].id, matching.id);
    }

    #[test]
    fn assets_in_smart_collection_rejects_a_manual_collection() {
        let catalog_path = unique_catalog_path("smart-collection-reject");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Manual", "/library")
            .expect("library");
        let manual = catalog
            .create_collection(library.id, "Not Smart", CollectionType::Manual)
            .expect("collection");

        let error = catalog
            .assets_in_smart_collection(manual.id)
            .expect_err("manual collection should be rejected");

        assert!(matches!(error, StorageError::NotASmartCollection));
    }

    #[test]
    fn list_collections_returns_only_collections_for_the_given_library() {
        let catalog_path = unique_catalog_path("list-collections");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("One", "/library").expect("library");
        let other_library = catalog.create_library("Two", "/other").expect("library");
        let project = catalog
            .create_collection(library.id, "Trailer", CollectionType::Project)
            .expect("project");
        catalog
            .create_collection(other_library.id, "Unrelated", CollectionType::Manual)
            .expect("unrelated collection");

        let collections = catalog.list_collections(library.id).expect("list");

        assert_eq!(
            collections.iter().map(|c| c.id).collect::<Vec<_>>(),
            vec![project.id]
        );
    }

    #[test]
    fn vocal_ratio_defaults_to_none_and_can_be_set() {
        let catalog_path = unique_catalog_path("vocal-ratio");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("One", "/library").expect("library");
        let asset = test_asset(&catalog, library.id, "vo-take-3.wav", "hash-vocal-ratio");

        assert_eq!(catalog.get_vocal_ratio(asset.id).expect("get"), None);

        catalog
            .set_vocal_ratio(asset.id, Some(0.82))
            .expect("set vocal ratio");

        assert_eq!(catalog.get_vocal_ratio(asset.id).expect("get"), Some(0.82));
    }

    #[test]
    fn detected_key_round_trips_through_audio_analysis() {
        let catalog_path = unique_catalog_path("detected-key");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Keys", "/library").expect("library");
        let asset = test_asset(&catalog, library.id, "pad.wav", "hash-detected-key");

        let loaded = catalog.get_asset(asset.id).expect("get").expect("exists");
        assert_eq!(loaded.detected_key, None);
        assert_eq!(loaded.key_strength, None);

        catalog
            .set_audio_analysis(
                asset.id,
                AudioAnalysisUpdate {
                    detected_key: Some("A minor".to_string()),
                    key_strength: Some(0.83),
                    ..Default::default()
                },
            )
            .expect("analysis");

        let loaded = catalog.get_asset(asset.id).expect("get").expect("exists");
        assert_eq!(loaded.detected_key.as_deref(), Some("A minor"));
        assert_eq!(loaded.key_strength, Some(0.83));
    }

    #[test]
    fn set_perceptual_fingerprint_leaves_the_rest_of_the_analysis_alone() {
        // The similarity-worker's fingerprint now arrives on its own
        // detached timeline, well after set_audio_analysis already
        // completed the job — this must be a narrow, single-column write,
        // not something that could stomp tempo/key/pitch/vocal-ratio with
        // whatever a late write happens to carry.
        let catalog_path = unique_catalog_path("perceptual-fingerprint");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Fingerprints", "/library").expect("library");
        let asset = test_asset(&catalog, library.id, "song.wav", "hash-fingerprint");

        catalog
            .set_audio_analysis(
                asset.id,
                AudioAnalysisUpdate {
                    detected_key: Some("A minor".to_string()),
                    key_strength: Some(0.83),
                    bpm: Some(120.0),
                    ..Default::default()
                },
            )
            .expect("analysis");

        catalog
            .set_perceptual_fingerprint(asset.id, Some("[0.1,0.2,0.3]".to_string()))
            .expect("fingerprint");

        let loaded = catalog.get_asset(asset.id).expect("get").expect("exists");
        assert_eq!(loaded.detected_key.as_deref(), Some("A minor"));
        assert_eq!(loaded.key_strength, Some(0.83));
        assert_eq!(loaded.bpm, Some(120.0));

        // perceptual_fingerprint isn't on AssetRecord (it's an opaque
        // internal vector, never surfaced to the UI) — perceptual_fingerprints
        // is its own accessor, used by similarity search.
        let fingerprints = catalog.perceptual_fingerprints(library.id).expect("fingerprints");
        assert_eq!(fingerprints, vec![(asset.id, "[0.1,0.2,0.3]".to_string())]);
    }

    #[test]
    fn requeue_analysis_replaces_finished_jobs_but_leaves_in_flight_ones() {
        let catalog_path = unique_catalog_path("requeue-analysis");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Lib", "/library").expect("library");
        let done = test_asset(&catalog, library.id, "done.wav", "hash-done");
        let running = test_asset(&catalog, library.id, "running.wav", "hash-running");

        // `done` has a completed analysis job; `running` has one mid-flight.
        catalog
            .enqueue_job(done.id, JobKind::AudioAnalysis, 40)
            .expect("enqueue done");
        let done_jobs = catalog
            .claim_pending_jobs(JobKind::AudioAnalysis, 10)
            .expect("claim");
        for job in &done_jobs {
            catalog.complete_job(job.id).expect("complete");
        }
        catalog
            .enqueue_job(running.id, JobKind::AudioAnalysis, 40)
            .expect("enqueue running");
        catalog
            .claim_pending_jobs(JobKind::AudioAnalysis, 10)
            .expect("claim running");

        let queued = catalog
            .requeue_analysis_for_library(library.id, JobKind::AudioAnalysis, 40)
            .expect("requeue");

        // Only `done` gets a fresh pending job; `running` is left alone.
        assert_eq!(queued, 1);
        assert_eq!(
            catalog
                .pending_job_count(JobKind::AudioAnalysis)
                .expect("count"),
            1
        );

        // Selection-scoped form: bulk, queues exactly one job per named
        // asset, and de-dupes a second call (delete-then-insert).
        let scoped = catalog
            .requeue_analysis_for_assets(&[done.id, running.id], JobKind::InstrumentDetection, 50)
            .expect("scoped");
        assert_eq!(scoped, 2);
        let scoped_again = catalog
            .requeue_analysis_for_assets(&[done.id, running.id], JobKind::InstrumentDetection, 50)
            .expect("scoped again");
        assert_eq!(scoped_again, 2);
        assert_eq!(
            catalog
                .pending_job_count(JobKind::InstrumentDetection)
                .expect("count"),
            2
        );
    }

    #[test]
    fn set_waveform_cache_is_a_no_op_for_a_deleted_asset() {
        let catalog_path = unique_catalog_path("waveform-cache-missing");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let missing = Uuid::new_v4();
        assert!(catalog
            .set_waveform_cache(missing, 48_000, "{\"peaks\":[]}")
            .is_ok());
        assert!(catalog
            .get_waveform_cache(missing)
            .expect("query")
            .is_none());
    }

    #[test]
    fn asset_instruments_store_facet_counts_and_or_filter() {
        let catalog_path = unique_catalog_path("asset-instruments");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Band", "/library").expect("library");
        let track_a = test_asset(&catalog, library.id, "a.wav", "hash-a");
        let track_b = test_asset(&catalog, library.id, "b.wav", "hash-b");
        let track_c = test_asset(&catalog, library.id, "c.wav", "hash-c");

        catalog
            .set_asset_instruments(track_a.id, &[("Guitar".into(), 0.9), ("Drums".into(), 0.7)])
            .expect("set a");
        catalog
            .set_asset_instruments(track_b.id, &[("Guitar".into(), 0.6), ("Piano".into(), 0.8)])
            .expect("set b");
        catalog
            .set_asset_instruments(track_c.id, &[("Strings".into(), 0.5)])
            .expect("set c");

        assert_eq!(
            catalog.instruments_for_asset(track_a.id).expect("a"),
            vec![("Guitar".to_string(), 0.9), ("Drums".to_string(), 0.7)]
        );

        let counts = catalog
            .instrument_counts_for_library(library.id)
            .expect("counts");
        assert_eq!(counts[0], ("Guitar".to_string(), 2));
        assert!(counts.contains(&("Piano".to_string(), 1)));
        assert!(counts.contains(&("Strings".to_string(), 1)));

        // OR match: Guitar covers a + b, Strings adds c.
        let mut hits = catalog
            .assets_with_any_instrument(library.id, &["Guitar".into(), "Strings".into()])
            .expect("filter")
            .into_iter()
            .map(|asset| asset.id)
            .collect::<Vec<_>>();
        hits.sort();
        let mut expected = vec![track_a.id, track_b.id, track_c.id];
        expected.sort();
        assert_eq!(hits, expected);

        // Re-running replaces rather than appends.
        catalog
            .set_asset_instruments(track_a.id, &[("Bass".into(), 0.95)])
            .expect("replace a");
        assert_eq!(
            catalog.instruments_for_asset(track_a.id).expect("a2"),
            vec![("Bass".to_string(), 0.95)]
        );
    }

    #[test]
    fn stem_group_links_members_and_marks_exactly_one_primary() {
        let catalog_path = unique_catalog_path("stem-group");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Songs", "/library").expect("library");
        let full_mix = test_asset(&catalog, library.id, "Song - Artist.mp3", "hash-full");
        let drums = test_asset(&catalog, library.id, "Song STEMS DRUMS - Artist.mp3", "hash-drums");
        let melody = test_asset(&catalog, library.id, "Song STEMS MELODY - Artist.mp3", "hash-melody");
        let unrelated = test_asset(&catalog, library.id, "Other Song.mp3", "hash-other");

        let group_id = Uuid::new_v4();
        catalog
            .set_stem_group(
                group_id,
                &[
                    (full_mix.id, "Full Mix".to_string(), true),
                    (drums.id, "Drums".to_string(), false),
                    (melody.id, "Melody".to_string(), false),
                ],
            )
            .expect("set stem group");

        let members = catalog.stem_group_members(group_id).expect("members");
        assert_eq!(members.len(), 3);
        assert_eq!(members[0].id, full_mix.id, "the primary member sorts first");
        assert!(members[0].stem_is_primary);
        assert_eq!(members[0].stem_label.as_deref(), Some("Full Mix"));
        assert!(members.iter().any(|member| member.id == drums.id && !member.stem_is_primary));
        assert!(members.iter().any(|member| member.id == melody.id && !member.stem_is_primary));
        assert!(members.iter().all(|member| member.stem_group_id == Some(group_id)));

        // asset_filenames_for_library only returns un-grouped assets — the
        // input a retroactive scan should re-scan, not the ones a previous
        // pass already resolved.
        let ungrouped = catalog
            .asset_filenames_for_library(library.id)
            .expect("ungrouped");
        assert_eq!(ungrouped, vec![(unrelated.id, "Other Song.mp3".to_string())]);

        catalog.clear_stem_group_for_asset(drums.id).expect("clear");
        let loaded = catalog.get_asset(drums.id).expect("get").expect("exists");
        assert_eq!(loaded.stem_group_id, None);
        assert_eq!(loaded.stem_label, None);
        assert!(!loaded.stem_is_primary);
        // Untouched siblings stay grouped.
        assert_eq!(
            catalog.stem_group_members(group_id).expect("members after clear").len(),
            2
        );
    }

    #[test]
    fn library_import_subfolders_default_empty_dedupe_and_can_be_cleared() {
        let catalog_path = unique_catalog_path("import-subfolders");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("One", "/library").expect("library");
        assert_eq!(library.import_subfolders, Vec::<String>::new());

        catalog
            .set_library_import_subfolders(
                library.id,
                &["Soundtrack".to_string(), " SFX ".to_string(), "soundtrack".to_string(), "".to_string()],
            )
            .expect("set subfolders");
        let loaded = catalog.get_library(library.id).expect("get").expect("exists");
        // Blank entries dropped, whitespace trimmed, case-insensitive dupes
        // collapsed to the first spelling.
        assert_eq!(loaded.import_subfolders, vec!["Soundtrack".to_string(), "SFX".to_string()]);

        catalog
            .set_library_import_subfolders(library.id, &[])
            .expect("clear subfolders");
        let cleared = catalog.get_library(library.id).expect("get").expect("exists");
        assert_eq!(cleared.import_subfolders, Vec::<String>::new());
    }

    #[test]
    fn library_import_subfolders_rejects_path_traversal_and_separators() {
        let catalog_path = unique_catalog_path("import-subfolders-traversal");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("One", "/library").expect("library");

        catalog
            .set_library_import_subfolders(
                library.id,
                &[
                    "Soundtrack".to_string(),
                    "../../etc".to_string(),
                    "..".to_string(),
                    ".".to_string(),
                    "nested/path".to_string(),
                    "back\\slash".to_string(),
                ],
            )
            .expect("set subfolders");
        let loaded = catalog.get_library(library.id).expect("get").expect("exists");

        // Only the one legitimate single-segment name survives — a
        // subfolder is always joined onto media_root as one path segment
        // (import_dropped_paths), so anything that could redirect a drop
        // outside the library is dropped rather than stored.
        assert_eq!(loaded.import_subfolders, vec!["Soundtrack".to_string()]);
    }

    #[test]
    fn project_export_path_defaults_to_none_and_can_be_set_and_cleared() {
        let catalog_path = unique_catalog_path("project-export-path");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("One", "/library").expect("library");
        let project = catalog
            .create_collection(library.id, "Trailer", CollectionType::Project)
            .expect("project");
        assert_eq!(project.export_path, None);

        catalog
            .set_collection_export_path(project.id, Some("/Volumes/Edit/Trailer/Sounds"))
            .expect("set export path");
        let reloaded = catalog
            .get_collection(project.id)
            .expect("get collection")
            .expect("collection exists");
        assert_eq!(
            reloaded.export_path.as_deref(),
            Some("/Volumes/Edit/Trailer/Sounds")
        );

        // Blank strings are treated the same as clearing the field, so a
        // text input the editor empties out behaves as "unset" rather than
        // storing an empty string.
        catalog
            .set_collection_export_path(project.id, Some("   "))
            .expect("clear export path with blank string");
        let cleared = catalog
            .get_collection(project.id)
            .expect("get collection")
            .expect("collection exists");
        assert_eq!(cleared.export_path, None);
    }

    #[test]
    fn project_export_folders_round_trip_and_order_by_creation() {
        let catalog_path = unique_catalog_path("project-export-folders");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("One", "/library").expect("library");
        let project = catalog
            .create_collection(library.id, "Trailer", CollectionType::Project)
            .expect("project");

        let foley = catalog
            .add_project_export_folder(project.id, "/Edit/Trailer/Foley", "foley")
            .expect("add foley folder");
        let ambience = catalog
            .add_project_export_folder(project.id, "/Edit/Trailer/Ambience", "ambience")
            .expect("add ambience folder");

        let folders = catalog
            .list_project_export_folders(project.id)
            .expect("list folders");
        assert_eq!(
            folders.iter().map(|folder| folder.id).collect::<Vec<_>>(),
            vec![foley.id, ambience.id]
        );
        assert_eq!(folders[0].role, "foley");
    }

    #[test]
    fn set_project_export_folder_role_updates_an_existing_folder() {
        let catalog_path = unique_catalog_path("project-export-folders-set-role");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("One", "/library").expect("library");
        let project = catalog
            .create_collection(library.id, "Trailer", CollectionType::Project)
            .expect("project");
        let folder = catalog
            .add_project_export_folder(project.id, "/Edit/Trailer/Misc", "sound_effect")
            .expect("add folder");

        catalog
            .set_project_export_folder_role(folder.id, "voiceover")
            .expect("set role");

        let updated = catalog
            .list_project_export_folders(project.id)
            .expect("list folders")
            .into_iter()
            .find(|entry| entry.id == folder.id)
            .expect("folder still present");
        assert_eq!(updated.role, "voiceover");
    }

    #[test]
    fn remove_project_export_folder_deletes_only_the_targeted_folder() {
        let catalog_path = unique_catalog_path("project-export-folders-remove");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("One", "/library").expect("library");
        let project = catalog
            .create_collection(library.id, "Trailer", CollectionType::Project)
            .expect("project");
        let keep = catalog
            .add_project_export_folder(project.id, "/Edit/Trailer/Keep", "music")
            .expect("add keep folder");
        let remove = catalog
            .add_project_export_folder(project.id, "/Edit/Trailer/Remove", "documents")
            .expect("add remove folder");

        catalog
            .remove_project_export_folder(remove.id)
            .expect("remove folder");

        let remaining = catalog
            .list_project_export_folders(project.id)
            .expect("list folders");
        assert_eq!(remaining.len(), 1);
        assert_eq!(remaining[0].id, keep.id);
    }

    #[test]
    fn deleting_a_library_cascades_to_its_projects_export_folders() {
        let catalog_path = unique_catalog_path("project-export-folders-cascade");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("One", "/library").expect("library");
        let project = catalog
            .create_collection(library.id, "Trailer", CollectionType::Project)
            .expect("project");
        catalog
            .add_project_export_folder(project.id, "/Edit/Trailer/Foley", "foley")
            .expect("add folder");

        catalog.delete_library(library.id).expect("delete library");

        assert_eq!(
            catalog
                .list_project_export_folders(project.id)
                .expect("list folders")
                .len(),
            0
        );
    }

    #[test]
    fn project_sfx_export_path_defaults_to_none_and_can_be_set_and_cleared() {
        let catalog_path = unique_catalog_path("project-sfx-export-path");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("One", "/library").expect("library");
        let project = catalog
            .create_collection(library.id, "Trailer", CollectionType::Project)
            .expect("project");
        assert_eq!(project.sfx_export_path, None);

        catalog
            .set_collection_sfx_export_path(project.id, Some("/Volumes/Edit/Trailer/SFX"))
            .expect("set sfx export path");
        let reloaded = catalog
            .get_collection(project.id)
            .expect("get collection")
            .expect("collection exists");
        assert_eq!(reloaded.sfx_export_path.as_deref(), Some("/Volumes/Edit/Trailer/SFX"));
        // export_path is independent of sfx_export_path.
        assert_eq!(reloaded.export_path, None);

        catalog
            .set_collection_sfx_export_path(project.id, Some("   "))
            .expect("clear sfx export path with blank string");
        let cleared = catalog
            .get_collection(project.id)
            .expect("get collection")
            .expect("collection exists");
        assert_eq!(cleared.sfx_export_path, None);
    }

    #[test]
    fn migrate_is_idempotent_when_export_path_column_already_exists() {
        let catalog_path = unique_catalog_path("migrate-idempotent");
        {
            let catalog = Catalog::open(&catalog_path).expect("open catalog");
            catalog.create_library("One", "/library").expect("library");
        }

        // Reopening runs `migrate()` again against a database that already
        // has the export_path column — must not error on a duplicate ALTER.
        let catalog = Catalog::open(&catalog_path).expect("reopen catalog");
        let libraries = catalog.list_libraries().expect("list libraries");
        assert_eq!(libraries.len(), 1);
    }

    #[test]
    fn unavailable_media_root_marks_originals_missing_but_keeps_catalog_searchable() {
        let catalog_path = unique_catalog_path("offline");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Offline", "/missing-root")
            .expect("library");
        let asset = test_asset(&catalog, library.id, "nas-impact.wav", "hash-offline");

        let changed = catalog
            .validate_media_availability(library.id, |_| false)
            .expect("validate");
        let loaded = catalog.get_asset(asset.id).expect("asset").expect("exists");
        let results = catalog
            .search_assets(library.id, AssetSearchQuery::text("nas"))
            .expect("search");

        assert_eq!(changed, 1);
        assert_eq!(loaded.availability_state, AvailabilityState::Missing);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn relinking_moved_asset_updates_path_and_restores_local_availability() {
        let catalog_path = unique_catalog_path("relink");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Relink", "/library")
            .expect("library");
        let asset = test_asset(&catalog, library.id, "moved.wav", "hash-moved");
        catalog
            .validate_media_availability(library.id, |_| false)
            .expect("offline");

        catalog
            .relink_asset(asset.id, "/new/location/moved.wav")
            .expect("relink");
        let loaded = catalog.get_asset(asset.id).expect("asset").expect("exists");

        assert_eq!(
            loaded.path,
            AssetPath::Referenced("/new/location/moved.wav".to_string())
        );
        assert_eq!(loaded.availability_state, AvailabilityState::Local);
    }

    #[test]
    fn open_enables_wal_journal_mode() {
        let catalog_path = unique_catalog_path("wal");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let mode: String = catalog
            .connection
            .query_row("PRAGMA journal_mode", [], |row| row.get(0))
            .expect("journal_mode");
        assert_eq!(mode.to_lowercase(), "wal");
    }

    #[test]
    fn waveform_cache_is_stored_retrieved_and_versioned() {
        let catalog_path = unique_catalog_path("waveform-cache");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Waves", "/library")
            .expect("library");
        let asset = test_asset(&catalog, library.id, "hit.wav", "hash-hit");

        assert!(catalog
            .get_waveform_cache(asset.id)
            .expect("query")
            .is_none());

        catalog
            .set_waveform_cache(asset.id, 48_000, "{\"peaks\":[0.1,0.9]}")
            .expect("store");
        let first = catalog
            .get_waveform_cache(asset.id)
            .expect("query")
            .expect("row");
        assert_eq!(first.sample_rate, 48_000);
        assert_eq!(first.payload, "{\"peaks\":[0.1,0.9]}");
        assert_eq!(first.waveform_version, 1);

        // Regenerating replaces the row in place and advances the version.
        catalog
            .set_waveform_cache(asset.id, 44_100, "{\"peaks\":[0.2]}")
            .expect("restore");
        let second = catalog
            .get_waveform_cache(asset.id)
            .expect("query")
            .expect("row");
        assert_eq!(second.sample_rate, 44_100);
        assert_eq!(second.waveform_version, 2);

        catalog.clear_waveform_cache(asset.id).expect("clear");
        assert!(catalog
            .get_waveform_cache(asset.id)
            .expect("query")
            .is_none());
    }

    #[test]
    fn relink_drops_the_stale_waveform_cache() {
        let catalog_path = unique_catalog_path("waveform-relink");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Waves", "/library")
            .expect("library");
        let asset = test_asset(&catalog, library.id, "moved.wav", "hash-moved");
        catalog
            .set_waveform_cache(asset.id, 48_000, "{\"peaks\":[0.5]}")
            .expect("store");

        catalog
            .relink_asset(asset.id, "/new/location/moved.wav")
            .expect("relink");

        assert!(catalog
            .get_waveform_cache(asset.id)
            .expect("query")
            .is_none());
    }

    #[test]
    fn export_usage_event_is_recorded_for_project() {
        let catalog_path = unique_catalog_path("usage");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Usage", "/library")
            .expect("library");
        let asset = test_asset(&catalog, library.id, "used.wav", "hash-used");
        let project = catalog
            .create_collection(library.id, "Trailer", CollectionType::Project)
            .expect("project");

        let event = catalog
            .record_usage_event(
                asset.id,
                Some(project.id),
                UsageEventType::Exported,
                "/projects/trailer/audio/used.wav",
            )
            .expect("usage");

        assert_eq!(event.asset_id, asset.id);
        assert_eq!(event.project_id, Some(project.id));
        assert_eq!(
            catalog
                .usage_events_for_project(project.id)
                .expect("events")
                .len(),
            1
        );
    }

    #[test]
    fn project_source_license_report_includes_traceable_asset_rows() {
        let catalog_path = unique_catalog_path("report");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Report", "/library")
            .expect("library");
        let asset = test_asset(&catalog, library.id, "licensed.wav", "hash-licensed");
        let project = catalog
            .create_collection(library.id, "Client Film", CollectionType::Project)
            .expect("project");
        catalog
            .set_source_record(SourceRecordDraft {
                asset_id: asset.id,
                provider: Some("Boom Library".to_string()),
                source_url: Some("https://example.com/sound".to_string()),
                license_type: Some("subscription".to_string()),
                license_status: Some("active".to_string()),
                attribution: Some("Boom Library / Artist Pack".to_string()),
                restrictions: Some("client project only".to_string()),
                receipt_path: Some("receipts/boom-library-2026-07.pdf".to_string()),
                ..Default::default()
            })
            .expect("source");
        catalog
            .record_usage_event(
                asset.id,
                Some(project.id),
                UsageEventType::Exported,
                "/project/audio/licensed.wav",
            )
            .expect("usage");

        let report = catalog.project_source_report(project.id).expect("report");

        assert_eq!(report.len(), 1);
        assert_eq!(report[0].asset_id, asset.id);
        assert_eq!(report[0].provider.as_deref(), Some("Boom Library"));
        assert_eq!(report[0].license_status.as_deref(), Some("active"));
        assert_eq!(
            report[0].attribution.as_deref(),
            Some("Boom Library / Artist Pack")
        );
        assert_eq!(
            report[0].restrictions.as_deref(),
            Some("client project only")
        );
        assert_eq!(
            report[0].receipt_path.as_deref(),
            Some("receipts/boom-library-2026-07.pdf")
        );
        assert_eq!(report[0].usage_status, "exported");
    }

    #[test]
    fn setting_source_record_replaces_existing_asset_context() {
        let catalog_path = unique_catalog_path("source-replace");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Report", "/library")
            .expect("library");
        let asset = test_asset(&catalog, library.id, "licensed.wav", "hash-source-replace");
        let project = catalog
            .create_collection(library.id, "Client Film", CollectionType::Project)
            .expect("project");

        catalog
            .set_source_record(SourceRecordDraft {
                asset_id: asset.id,
                provider: Some("Old Provider".to_string()),
                source_url: None,
                license_type: None,
                license_status: Some("uncertain".to_string()),
                attribution: None,
                restrictions: None,
                receipt_path: None,
                ..Default::default()
            })
            .expect("old source");
        catalog
            .set_source_record(SourceRecordDraft {
                asset_id: asset.id,
                provider: Some("New Provider".to_string()),
                source_url: Some("https://example.com/new".to_string()),
                license_type: Some("subscription".to_string()),
                license_status: Some("active".to_string()),
                attribution: None,
                restrictions: None,
                receipt_path: None,
                ..Default::default()
            })
            .expect("new source");
        catalog
            .record_usage_event(
                asset.id,
                Some(project.id),
                UsageEventType::Exported,
                "/project/audio/licensed.wav",
            )
            .expect("usage");

        let report = catalog.project_source_report(project.id).expect("report");

        assert_eq!(report.len(), 1);
        assert_eq!(report[0].provider.as_deref(), Some("New Provider"));
        assert_eq!(report[0].license_status.as_deref(), Some("active"));

        let source = catalog
            .get_source_record(asset.id)
            .expect("get source")
            .expect("source exists");
        assert_eq!(source.provider.as_deref(), Some("New Provider"));
        assert_eq!(source.license_status.as_deref(), Some("active"));
    }

    #[test]
    fn get_source_record_returns_none_when_unset() {
        let catalog_path = unique_catalog_path("source-unset");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Report", "/library")
            .expect("library");
        let asset = test_asset(&catalog, library.id, "no-source.wav", "hash-no-source");

        assert_eq!(catalog.get_source_record(asset.id).expect("query"), None);
    }

    #[test]
    fn license_document_and_expiry_fields_round_trip() {
        let catalog_path = unique_catalog_path("source-license-fields");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("License Fields", "/library")
            .expect("library");
        let asset = test_asset(&catalog, library.id, "licensed.wav", "hash-license-fields");

        catalog
            .set_source_record(SourceRecordDraft {
                asset_id: asset.id,
                license_document_path: Some("License/licensed-invoice.pdf".to_string()),
                license_valid_from: Some("2026-01-01".to_string()),
                license_valid_until: Some("2027-01-05".to_string()),
                license_expiry_source: Some("extracted".to_string()),
                ..Default::default()
            })
            .expect("save source with license fields");

        let source = catalog
            .get_source_record(asset.id)
            .expect("query")
            .expect("record exists");
        assert_eq!(source.license_document_path.as_deref(), Some("License/licensed-invoice.pdf"));
        assert_eq!(source.license_valid_from.as_deref(), Some("2026-01-01"));
        assert_eq!(source.license_valid_until.as_deref(), Some("2027-01-05"));
        assert_eq!(source.license_expiry_source.as_deref(), Some("extracted"));
    }

    #[test]
    fn assets_with_expired_license_only_returns_past_confirmed_dates_in_that_library() {
        let catalog_path = unique_catalog_path("source-license-expired");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Expiry", "/library")
            .expect("library");
        let other_library = catalog
            .create_library("Other Expiry", "/other")
            .expect("other library");

        let expired = test_asset(&catalog, library.id, "expired.wav", "hash-expired");
        let still_valid = test_asset(&catalog, library.id, "valid.wav", "hash-valid");
        let no_expiry = test_asset(&catalog, library.id, "no-expiry.wav", "hash-no-expiry");
        let expired_elsewhere = test_asset(&catalog, other_library.id, "expired-other.wav", "hash-expired-other");

        catalog
            .set_source_record(SourceRecordDraft {
                asset_id: expired.id,
                license_valid_until: Some("2020-01-01".to_string()),
                ..Default::default()
            })
            .expect("expired source");
        catalog
            .set_source_record(SourceRecordDraft {
                asset_id: still_valid.id,
                license_valid_until: Some("2099-01-01".to_string()),
                ..Default::default()
            })
            .expect("valid source");
        catalog
            .set_source_record(SourceRecordDraft {
                asset_id: no_expiry.id,
                provider: Some("Some Provider".to_string()),
                ..Default::default()
            })
            .expect("no-expiry source");
        catalog
            .set_source_record(SourceRecordDraft {
                asset_id: expired_elsewhere.id,
                license_valid_until: Some("2020-01-01".to_string()),
                ..Default::default()
            })
            .expect("expired source in other library");

        let expired_assets = catalog
            .assets_with_expired_license(library.id, "2026-09-12")
            .expect("query");

        assert_eq!(expired_assets, vec![(expired.id, "2020-01-01".to_string())]);
    }

    #[test]
    fn asset_ids_with_source_record_matches_per_asset_lookups_and_stays_library_scoped() {
        let catalog_path = unique_catalog_path("source-ids-batch");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Batch", "/library")
            .expect("library");
        let other_library = catalog
            .create_library("Other", "/other")
            .expect("other library");

        let with_source = test_asset(&catalog, library.id, "licensed.wav", "hash-batch-licensed");
        let without_source = test_asset(&catalog, library.id, "unlicensed.wav", "hash-batch-unlicensed");
        let other_with_source = test_asset(&catalog, other_library.id, "other.wav", "hash-batch-other");

        catalog
            .set_source_record(SourceRecordDraft {
                asset_id: with_source.id,
                provider: Some("Boom Library".to_string()),
                source_url: None,
                license_type: None,
                license_status: None,
                attribution: None,
                restrictions: None,
                receipt_path: None,
                ..Default::default()
            })
            .expect("source");
        catalog
            .set_source_record(SourceRecordDraft {
                asset_id: other_with_source.id,
                provider: Some("Boom Library".to_string()),
                source_url: None,
                license_type: None,
                license_status: None,
                attribution: None,
                restrictions: None,
                receipt_path: None,
                ..Default::default()
            })
            .expect("source");

        let ids = catalog
            .asset_ids_with_source_record(library.id)
            .expect("batch query");

        assert!(ids.contains(&with_source.id));
        assert!(!ids.contains(&without_source.id));
        assert!(
            !ids.contains(&other_with_source.id),
            "must stay scoped to the requested library"
        );
    }

    #[test]
    fn trashed_asset_is_hidden_from_list_and_search_until_restored() {
        let catalog_path = unique_catalog_path("trash-hide");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Trash", "/library")
            .expect("library");
        let asset = test_asset(&catalog, library.id, "unwanted.wav", "hash-trash-hide");

        catalog
            .move_asset_to_trash(asset.id, "duplicate review", 1_000)
            .expect("trash asset");

        assert!(catalog.list_assets(library.id).expect("list").is_empty());
        assert!(catalog
            .search_assets(library.id, AssetSearchQuery::text(""))
            .expect("search")
            .is_empty());

        catalog.restore_asset_from_trash(asset.id).expect("restore");

        assert_eq!(
            catalog
                .list_assets(library.id)
                .expect("list after restore")
                .iter()
                .map(|entry| entry.id)
                .collect::<Vec<_>>(),
            vec![asset.id]
        );
    }

    #[test]
    fn list_trash_items_reports_only_items_currently_in_trash() {
        let catalog_path = unique_catalog_path("trash-list");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Trash", "/library")
            .expect("library");
        let kept = test_asset(&catalog, library.id, "kept.wav", "hash-trash-kept");
        let trashed = test_asset(&catalog, library.id, "trashed.wav", "hash-trash-trashed");

        catalog
            .move_asset_to_trash(trashed.id, "duplicate", 2_000)
            .expect("trash asset");
        catalog
            .move_asset_to_trash(kept.id, "mistake", 1_000)
            .expect("trash then restore");
        catalog
            .restore_asset_from_trash(kept.id)
            .expect("restore kept asset");

        let items = catalog.list_trash_items(library.id).expect("list trash");

        assert_eq!(
            items.iter().map(|item| item.asset_id).collect::<Vec<_>>(),
            vec![trashed.id]
        );
        assert_eq!(items[0].reason, "duplicate");
    }

    #[test]
    fn empty_trash_for_library_purges_only_in_trash_assets_for_that_library() {
        let catalog_path = unique_catalog_path("empty-trash-library");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Empty Trash", "/library")
            .expect("library");
        let other_library = catalog
            .create_library("Other", "/other")
            .expect("other library");

        let trashed = test_asset(&catalog, library.id, "trashed.wav", "hash-empty-trashed");
        let kept = test_asset(&catalog, library.id, "kept.wav", "hash-empty-kept");
        let other_trashed = test_asset(&catalog, other_library.id, "other.wav", "hash-empty-other");

        catalog
            .move_asset_to_trash(trashed.id, "duplicate", 1_000)
            .expect("trash asset");
        catalog
            .move_asset_to_trash(other_trashed.id, "duplicate", 1_000)
            .expect("trash other library asset");

        let purged = catalog
            .empty_trash_for_library(library.id)
            .expect("empty trash");

        assert_eq!(purged, 1);
        assert!(catalog.get_asset(trashed.id).expect("get").is_none());
        assert!(catalog.get_asset(kept.id).expect("get").is_some());
        assert!(
            catalog.get_asset(other_trashed.id).expect("get").is_some(),
            "another library's trashed asset must not be purged"
        );
    }

    #[test]
    fn delete_library_cascades_to_assets_tags_and_collections_but_not_other_libraries() {
        let catalog_path = unique_catalog_path("delete-library");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Deleted", "/library")
            .expect("library");
        let other_library = catalog
            .create_library("Kept", "/other")
            .expect("other library");

        let asset = test_asset(&catalog, library.id, "gone.wav", "hash-delete-library");
        let tag = catalog
            .create_tag("impact", "action", true)
            .expect("create tag");
        catalog
            .apply_tag_to_assets(&[asset.id], tag.id, TagOrigin::Manual)
            .expect("apply tag");
        let project = catalog
            .create_collection(library.id, "Trailer", CollectionType::Project)
            .expect("create collection");
        catalog
            .add_assets_to_collection(project.id, &[asset.id])
            .expect("add to collection");
        let other_asset = test_asset(&catalog, other_library.id, "stays.wav", "hash-delete-other");

        catalog.delete_library(library.id).expect("delete library");

        assert!(catalog.get_library(library.id).expect("get").is_none());
        assert!(catalog.get_asset(asset.id).expect("get").is_none());
        assert!(
            catalog
                .list_tags()
                .expect("list tags")
                .iter()
                .any(|candidate| candidate.id == tag.id),
            "tags are global, not library-scoped, and must survive"
        );
        assert!(catalog.get_library(other_library.id).expect("get").is_some());
        assert!(catalog.get_asset(other_asset.id).expect("get").is_some());
    }

    #[test]
    fn purge_trash_item_requires_retention_age_then_deletes_asset() {
        let catalog_path = unique_catalog_path("trash-purge");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Trash", "/library")
            .expect("library");
        let asset = test_asset(&catalog, library.id, "old.wav", "hash-trash-purge");

        catalog
            .move_asset_to_trash(asset.id, "cleanup", 1_000)
            .expect("trash asset");

        assert!(!catalog
            .purge_trash_item(asset.id, 1_000 + 6_000, 7_000)
            .expect("too early"));
        assert!(catalog.get_asset(asset.id).expect("query").is_some());

        assert!(catalog
            .purge_trash_item(asset.id, 1_000 + 7_000, 7_000)
            .expect("purge"));
        assert!(catalog.get_asset(asset.id).expect("query").is_none());
    }

    #[test]
    fn finalize_permanent_deletion_bypasses_retention_and_removes_asset() {
        let catalog_path = unique_catalog_path("trash-permanent-delete");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Trash", "/library")
            .expect("library");
        let asset = test_asset(&catalog, library.id, "old.wav", "hash-permanent-delete");

        catalog
            .move_asset_to_trash(asset.id, "manual", 1_000)
            .expect("trash asset");

        // No retention wait needed, unlike purge_trash_item — a freshly
        // trashed item can be force-deleted immediately.
        catalog
            .finalize_permanent_deletion(asset.id)
            .expect("finalize permanent deletion");

        assert!(catalog.get_asset(asset.id).expect("query").is_none());
        assert!(catalog
            .list_trash_items(library.id)
            .expect("list trash items")
            .is_empty());
    }

    #[test]
    fn project_memberships_reports_export_status_per_asset_and_project() {
        let catalog_path = unique_catalog_path("project-memberships");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog
            .create_library("Memberships", "/library")
            .expect("library");
        let asset = test_asset(&catalog, library.id, "theme.wav", "hash-memberships");
        let project = catalog
            .create_collection(library.id, "Trailer", CollectionType::Project)
            .expect("project");
        let manual_collection = catalog
            .create_collection(library.id, "Favorites", CollectionType::Manual)
            .expect("manual collection");

        catalog
            .add_assets_to_collection(project.id, &[asset.id])
            .expect("add to project");
        catalog
            .add_assets_to_collection(manual_collection.id, &[asset.id])
            .expect("add to manual collection");

        let memberships = catalog
            .project_memberships_for_library(library.id)
            .expect("memberships");

        // Only the Project-type collection shows up, not the Manual one.
        assert_eq!(memberships.len(), 1);
        assert_eq!(memberships[0].asset_id, asset.id);
        assert_eq!(memberships[0].project_id, project.id);
        assert_eq!(memberships[0].project_name, "Trailer");
        assert!(!memberships[0].exported);

        catalog
            .record_usage_event(asset.id, Some(project.id), UsageEventType::Exported, "/export/theme.wav")
            .expect("record export");

        let memberships = catalog
            .project_memberships_for_library(library.id)
            .expect("memberships after export");
        assert!(memberships[0].exported);
    }

    fn unique_catalog_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("darkwave-{name}-{}.sqlite", Uuid::new_v4()));
        let _ = fs::remove_file(&path);
        path
    }
}
