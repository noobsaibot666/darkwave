use chrono::Utc;
use rusqlite::{params, Connection, OptionalExtension};
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
}

pub fn is_network_tolerant_catalog_path(path: &str) -> Result<bool, StorageError> {
    if !path.starts_with('/') && !path.contains(":\\") {
        return Err(StorageError::RelativePath);
    }

    Ok(!path.contains("/Volumes/") && !path.starts_with("\\\\"))
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryRecord {
    pub id: Uuid,
    pub name: String,
    pub media_root: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
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

#[derive(Clone, Debug, Eq, PartialEq)]
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
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReviewState {
    Unreviewed,
    Reviewed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum JobKind {
    MetadataExtraction,
    Hashing,
    WaveformGeneration,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct JobRecord {
    pub id: Uuid,
    pub asset_id: Uuid,
    pub kind: JobKind,
    pub priority: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TagOrigin {
    Filename,
    Metadata,
    AcousticModel,
    UserRule,
    UserCorrection,
    Manual,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TagRecord {
    pub id: Uuid,
    pub name: String,
    pub normalized_name: String,
    pub facet: Option<String>,
    pub is_system: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CollectionType {
    Manual,
    Smart,
    Project,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CollectionRecord {
    pub id: Uuid,
    pub library_id: Uuid,
    pub name: String,
    pub collection_type: CollectionType,
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

    pub fn register_asset(&self, asset: NewAssetRecord) -> Result<AssetRecord, StorageError> {
        if let Some(content_hash) = &asset.content_hash {
            if let Some(existing) =
                self.find_asset_by_hash(asset.library_id, content_hash, asset.file_size)?
            {
                return Ok(existing);
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
            .map(|asset| asset.expect("inserted asset exists"))
    }

    pub fn list_assets(&self, library_id: Uuid) -> Result<Vec<AssetRecord>, StorageError> {
        let mut statement = self.connection.prepare(
            "SELECT id, library_id, original_filename, display_name, relative_path, referenced_path,
                storage_mode, content_hash, media_type, file_size, availability_state, review_state, favorite
             FROM assets
             WHERE library_id = ?1
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
            &format!("{}|{}", tag_id, join_uuids(asset_ids)),
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
                assets.media_type, assets.file_size, assets.availability_state, assets.review_state, assets.favorite
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

        let (owner_id, asset_ids) = split_owner_and_assets(&payload);
        match kind.as_str() {
            "remove_asset_tags" => {
                for asset_id in asset_ids {
                    self.connection.execute(
                        "DELETE FROM asset_tags WHERE tag_id = ?1 AND asset_id = ?2",
                        params![owner_id.to_string(), asset_id.to_string()],
                    )?;
                }
            }
            "remove_collection_assets" => {
                for asset_id in asset_ids {
                    self.connection.execute(
                        "DELETE FROM collection_assets WHERE collection_id = ?1 AND asset_id = ?2",
                        params![owner_id.to_string(), asset_id.to_string()],
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
              notes TEXT
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

            CREATE TABLE IF NOT EXISTS undo_actions (
              id TEXT PRIMARY KEY,
              kind TEXT NOT NULL,
              payload TEXT NOT NULL,
              created_at TEXT NOT NULL,
              applied_at TEXT
            );

            CREATE UNIQUE INDEX IF NOT EXISTS idx_assets_library_hash_size
              ON assets(library_id, content_hash, file_size)
              WHERE content_hash IS NOT NULL;
            CREATE INDEX IF NOT EXISTS idx_background_jobs_pending
              ON background_jobs(state, priority, created_at);
            CREATE INDEX IF NOT EXISTS idx_asset_tags_asset ON asset_tags(asset_id);
            CREATE INDEX IF NOT EXISTS idx_collection_assets_collection ON collection_assets(collection_id);
            ",
        )?;

        Ok(())
    }

    pub fn get_asset(&self, id: Uuid) -> Result<Option<AssetRecord>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, library_id, original_filename, display_name, relative_path, referenced_path,
                    storage_mode, content_hash, media_type, file_size, availability_state, review_state, favorite
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
                    storage_mode, content_hash, media_type, file_size, availability_state, review_state, favorite
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

fn normalize_term(value: &str) -> String {
    value
        .trim()
        .to_ascii_lowercase()
        .replace([' ', '/', '-'], "_")
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

fn job_kind_to_db(kind: &JobKind) -> &'static str {
    match kind {
        JobKind::MetadataExtraction => "metadata_extraction",
        JobKind::Hashing => "hashing",
        JobKind::WaveformGeneration => "waveform_generation",
    }
}

fn job_kind_from_db(value: &str) -> JobKind {
    match value {
        "hashing" => JobKind::Hashing,
        "waveform_generation" => JobKind::WaveformGeneration,
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

        let first_id = catalog
            .register_asset(first.clone())
            .expect("first import")
            .id;
        let duplicate_id = catalog.register_asset(first).expect("duplicate import").id;

        assert_eq!(first_id, duplicate_id);
        assert_eq!(catalog.list_assets(library.id).expect("assets").len(), 1);
    }

    #[test]
    fn job_queue_persists_pending_work_after_reopen() {
        let catalog_path = unique_catalog_path("jobs");
        let catalog = Catalog::open(&catalog_path).expect("open catalog");
        let library = catalog.create_library("Jobs", "/library").expect("library");
        let asset = catalog
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
    }

    fn unique_catalog_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("darkwave-{name}-{}.sqlite", Uuid::new_v4()));
        let _ = fs::remove_file(&path);
        path
    }
}
