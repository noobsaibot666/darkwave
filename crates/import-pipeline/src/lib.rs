use audio_metadata::{extract_immediate_metadata, supported_mvp_format, MetadataError};
use sha2::{Digest, Sha256};
use shared_types::{AvailabilityState, StorageMode};
use std::fs;
use std::path::Path;
use storage::{AssetPath, AssetRecord, Catalog, JobKind, NewAssetRecord, StorageError};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImportMode {
    Managed,
    Referenced,
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("unsupported audio format: {0}")]
    UnsupportedFormat(String),
    #[error("file name is required")]
    MissingFilename,
    #[error("metadata error: {0}")]
    Metadata(#[from] MetadataError),
    #[error("storage error: {0}")]
    Storage(#[from] StorageError),
    #[error("file read failed: {0}")]
    Io(#[from] std::io::Error),
}

pub fn should_ignore_watched_file(filename: &str) -> bool {
    let lower = filename.to_ascii_lowercase();
    lower.ends_with(".crdownload")
        || lower.ends_with(".download")
        || lower.ends_with(".part")
        || lower.ends_with(".tmp")
}

pub fn is_stable_watched_file(filename: &str, previous_size: u64, current_size: u64) -> bool {
    !should_ignore_watched_file(filename) && previous_size == current_size
}

pub fn import_file(
    catalog: &Catalog,
    library_id: Uuid,
    path: impl AsRef<Path>,
    mode: ImportMode,
) -> Result<AssetRecord, ImportError> {
    let path = path.as_ref();
    let metadata = extract_immediate_metadata(path)?;

    if !supported_mvp_format(&metadata.extension) {
        return Err(ImportError::UnsupportedFormat(metadata.extension));
    }

    let original_filename = path
        .file_name()
        .and_then(|file_name| file_name.to_str())
        .ok_or(ImportError::MissingFilename)?
        .to_string();
    let display_name = path
        .file_stem()
        .and_then(|file_stem| file_stem.to_str())
        .unwrap_or(&original_filename)
        .replace(['_', '-'], " ");
    let content_hash = lightweight_content_hash(path)?;
    let storage_mode = match mode {
        ImportMode::Managed => StorageMode::Managed,
        ImportMode::Referenced => StorageMode::Referenced,
    };
    let asset_path = match mode {
        ImportMode::Managed => AssetPath::Managed(format!("Media/00/{original_filename}")),
        ImportMode::Referenced => AssetPath::Referenced(path.to_string_lossy().to_string()),
    };

    let asset = catalog.register_asset(NewAssetRecord {
        library_id,
        original_filename,
        display_name,
        path: asset_path,
        storage_mode,
        content_hash: Some(content_hash),
        media_type: "other".to_string(),
        file_size: metadata.file_size,
        availability_state: AvailabilityState::Local,
    })?;

    catalog.enqueue_job(asset.id, JobKind::MetadataExtraction, 10)?;
    catalog.enqueue_job(asset.id, JobKind::Hashing, 20)?;
    catalog.enqueue_job(asset.id, JobKind::WaveformGeneration, 30)?;

    Ok(asset)
}

fn lightweight_content_hash(path: &Path) -> Result<String, ImportError> {
    let bytes = fs::read(path)?;
    let digest = Sha256::digest(bytes);
    Ok(format!("{digest:x}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use shared_types::StorageMode;
    use std::fs;
    use std::path::PathBuf;
    use storage::Catalog;
    use uuid::Uuid;

    #[test]
    fn incomplete_browser_downloads_are_ignored() {
        assert!(should_ignore_watched_file("track.wav.crdownload"));
        assert!(!should_ignore_watched_file("track.wav"));
    }

    #[test]
    fn watched_file_is_ready_only_after_size_stabilizes() {
        assert!(!is_stable_watched_file("track.wav", 42, 84));
        assert!(is_stable_watched_file("track.wav", 84, 84));
        assert!(!is_stable_watched_file("track.wav.crdownload", 84, 84));
    }

    #[test]
    fn referenced_import_registers_asset_and_metadata_jobs() {
        let catalog_path = unique_catalog_path("referenced-import");
        let audio_path = unique_audio_path("referenced-impact.wav");
        fs::write(&audio_path, b"not real wav yet").expect("fixture");

        let catalog = Catalog::open(&catalog_path).expect("catalog");
        let library = catalog
            .create_library("Import", "/library")
            .expect("library");
        let imported =
            import_file(&catalog, library.id, &audio_path, ImportMode::Referenced).expect("import");

        assert_eq!(imported.original_filename, "referenced-impact.wav");
        assert_eq!(imported.storage_mode, StorageMode::Referenced);
        assert_eq!(catalog.list_assets(library.id).expect("assets").len(), 1);
        assert!(catalog.next_pending_job().expect("job query").is_some());
    }

    fn unique_catalog_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("darkwave-{name}-{}.sqlite", Uuid::new_v4()));
        let _ = fs::remove_file(&path);
        path
    }

    fn unique_audio_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("darkwave-audio-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).expect("fixture directory");
        path.push(name);
        path
    }
}
