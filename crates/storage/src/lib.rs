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
                storage_mode, content_hash, media_type, file_size, availability_state
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

            CREATE UNIQUE INDEX IF NOT EXISTS idx_assets_library_hash_size
              ON assets(library_id, content_hash, file_size)
              WHERE content_hash IS NOT NULL;
            CREATE INDEX IF NOT EXISTS idx_background_jobs_pending
              ON background_jobs(state, priority, created_at);
            ",
        )?;

        Ok(())
    }

    fn get_asset(&self, id: Uuid) -> Result<Option<AssetRecord>, StorageError> {
        self.connection
            .query_row(
                "SELECT id, library_id, original_filename, display_name, relative_path, referenced_path,
                    storage_mode, content_hash, media_type, file_size, availability_state
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
                    storage_mode, content_hash, media_type, file_size, availability_state
                 FROM assets
                 WHERE library_id = ?1 AND content_hash = ?2 AND file_size = ?3
                 LIMIT 1",
                params![library_id.to_string(), content_hash, file_size as i64],
                asset_from_row,
            )
            .optional()
            .map_err(StorageError::from)
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

    fn unique_catalog_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("darkwave-{name}-{}.sqlite", Uuid::new_v4()));
        let _ = fs::remove_file(&path);
        path
    }
}
