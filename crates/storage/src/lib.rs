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
    /// see docs/adr/0025-real-audio-analysis.md.
    pub musical_key: Option<String>,
    pub key_confidence: Option<f64>,
    /// Fraction of the clip Silero VAD classifies as speech (ADR 0027).
    /// `None` until the analysis job runs, or if it couldn't run at all.
    pub vocal_ratio: Option<f64>,
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ReviewState {
    Unreviewed,
    Reviewed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JobKind {
    MetadataExtraction,
    Hashing,
    WaveformGeneration,
    AudioAnalysis,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobRecord {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub kind: JobKind,
    pub priority: i64,
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
    /// Folder on disk this project's sounds get quick-exported to (e.g. a
    /// DaVinci Resolve "Sounds" watch folder), so an editor can drag them
    /// straight into a timeline. `None` until the editor configures one.
    pub export_path: Option<String>,
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
        let catalog = Self { connection };
        catalog.migrate()?;
        Ok(catalog)
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
        let mut statement = self
            .connection
            .prepare("SELECT id, name, media_root FROM libraries ORDER BY created_at ASC")?;
        let libraries = statement
            .query_map([], |row| {
                Ok(LibraryRecord {
                    id: parse_uuid(row.get::<_, String>(0)?),
                    name: row.get(1)?,
                    media_root: row.get(2)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;

        Ok(libraries)
    }

    pub fn get_library(&self, id: Uuid) -> Result<Option<LibraryRecord>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, name, media_root FROM libraries WHERE id = ?1",
                params![id.to_string()],
                |row| {
                    Ok(LibraryRecord {
                        id: parse_uuid(row.get::<_, String>(0)?),
                        name: row.get(1)?,
                        media_root: row.get(2)?,
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
                    bpm, bpm_confidence, musical_key, key_confidence, vocal_ratio
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

    pub fn fail_job(&self, job_id: Uuid) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE background_jobs SET state = 'failed', attempts = attempts + 1, updated_at = ?1 WHERE id = ?2",
            params![Utc::now().to_rfc3339(), job_id.to_string()],
        )?;
        Ok(())
    }

    pub fn complete_pending_jobs_for_asset(
        &self,
        asset_id: Uuid,
        kind: JobKind,
    ) -> Result<usize, StorageError> {
        let updated = self.connection.execute(
            "UPDATE background_jobs SET state = 'completed', updated_at = ?1
             WHERE asset_id = ?2 AND kind = ?3 AND state = 'pending'",
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

    pub fn set_audio_analysis(
        &self,
        asset_id: Uuid,
        update: AudioAnalysisUpdate,
    ) -> Result<(), StorageError> {
        self.connection.execute(
            "UPDATE assets SET
                duration_ms = ?1, sample_rate = ?2, bit_depth = ?3, channels = ?4,
                loudness_lufs = ?5, peak_db = ?6, bpm = ?7, bpm_confidence = ?8,
                musical_key = ?9, key_confidence = ?10, perceptual_fingerprint = ?11
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
            ],
        )?;
        Ok(())
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
                "SELECT id, library_id, name, type, query_definition, export_path FROM collections WHERE id = ?1",
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

    pub fn list_collections(
        &self,
        library_id: Uuid,
    ) -> Result<Vec<CollectionRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, library_id, name, type, query_definition, export_path FROM collections
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
                assets.musical_key, assets.key_confidence, assets.vocal_ratio
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
                assets.musical_key, assets.key_confidence, assets.vocal_ratio
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
                attribution, restrictions, receipt_path
             ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
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
                    usage_status: row.get(10)?,
                    destination: row.get(11)?,
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
                    attribution, restrictions, receipt_path
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
            .map(|result| result.map(|id| parse_uuid(id)))
            .collect::<Result<std::collections::HashSet<_>, _>>()
            .map_err(StorageError::from)?;

        Ok(ids)
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
        self.ensure_column("assets", "vocal_ratio", "REAL")?;

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
                    bpm, bpm_confidence, musical_key, key_confidence, vocal_ratio
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
                    bpm, bpm_confidence, musical_key, key_confidence, vocal_ratio
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
    }
}

fn job_kind_from_db(value: &str) -> JobKind {
    match value {
        "hashing" => JobKind::Hashing,
        "waveform_generation" => JobKind::WaveformGeneration,
        "audio_analysis" => JobKind::AudioAnalysis,
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
    fn requeue_failed_jobs_respects_the_attempt_cap() {
        let catalog_path = unique_catalog_path("job-requeue");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Jobs", "/library").expect("library");
        let asset = test_asset(&catalog, library.id, "tone.wav", "hash-job-requeue");
        let job = catalog
            .enqueue_job(asset.id, JobKind::AudioAnalysis, 40)
            .expect("enqueue");

        catalog.fail_job(job.id).expect("fail once");
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
        catalog.fail_job(job.id).expect("fail twice");
        catalog.fail_job(job.id).expect("fail thrice");
        let requeued_after_cap = catalog.requeue_failed_jobs(3).expect("requeue at cap");
        assert_eq!(requeued_after_cap, 0);
        assert!(catalog
            .pending_jobs_of_kind(JobKind::AudioAnalysis, 10)
            .expect("pending jobs")
            .is_empty());
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

        catalog.fail_job(job.id).expect("fail job");

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

    fn unique_catalog_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("darkwave-{name}-{}.sqlite", Uuid::new_v4()));
        let _ = fs::remove_file(&path);
        path
    }
}
