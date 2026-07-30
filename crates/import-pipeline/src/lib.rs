use audio_metadata::{
    extract_embedded_metadata, extract_immediate_metadata, supported_mvp_format,
    EmbeddedFileMetadata, MetadataError,
};
use sha2::{Digest, Sha256};
use shared_types::{AvailabilityState, StorageMode};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use storage::{
    AssetPath, AssetRecord, Catalog, JobKind, NewAssetRecord, SourceRecordDraft, StorageError,
    TagOrigin,
};
use thiserror::Error;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ImportMode {
    Managed,
    Referenced,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct ImportSourceContext {
    pub provider: Option<String>,
    pub source_url: Option<String>,
    pub license_type: Option<String>,
    pub license_status: Option<String>,
    pub attribution: Option<String>,
    pub restrictions: Option<String>,
    pub receipt_path: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WatchedImportCandidate {
    pub path: PathBuf,
    pub previous_size: u64,
    pub current_size: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WatchedFolderPoller {
    folder: PathBuf,
    previous_sizes: BTreeMap<PathBuf, u64>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilesystemNotificationKind {
    CreatedOrModified,
    Removed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemNotification {
    pub path: PathBuf,
    pub kind: FilesystemNotificationKind,
    pub current_size: Option<u64>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FilesystemNotificationBuffer {
    observed_sizes: BTreeMap<PathBuf, u64>,
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("unsupported audio format: {0}")]
    UnsupportedFormat(String),
    #[error("file name is required")]
    MissingFilename,
    #[error("library does not exist: {0}")]
    MissingLibrary(Uuid),
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

pub fn discover_ready_watched_files(
    folder: impl AsRef<Path>,
    previous_sizes: &BTreeMap<PathBuf, u64>,
) -> Result<Vec<WatchedImportCandidate>, ImportError> {
    let mut candidates = Vec::new();

    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;

        if !metadata.is_file() {
            continue;
        }

        let filename = match path.file_name().and_then(|file_name| file_name.to_str()) {
            Some(filename) => filename,
            None => continue,
        };
        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default();
        let current_size = metadata.len();
        let previous_size = previous_sizes.get(&path).copied().unwrap_or_default();

        if supported_mvp_format(extension)
            && is_stable_watched_file(filename, previous_size, current_size)
        {
            candidates.push(WatchedImportCandidate {
                path,
                previous_size,
                current_size,
            });
        }
    }

    candidates.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(candidates)
}

impl WatchedFolderPoller {
    pub fn new(folder: impl Into<PathBuf>) -> Self {
        Self {
            folder: folder.into(),
            previous_sizes: BTreeMap::new(),
        }
    }

    pub fn poll(&mut self) -> Result<Vec<WatchedImportCandidate>, ImportError> {
        let candidates = discover_ready_watched_files(&self.folder, &self.previous_sizes)?;
        self.previous_sizes = snapshot_file_sizes(&self.folder)?;
        Ok(candidates)
    }

    pub fn previous_sizes(&self) -> &BTreeMap<PathBuf, u64> {
        &self.previous_sizes
    }
}

impl FilesystemNotificationBuffer {
    pub fn new() -> Self {
        Self {
            observed_sizes: BTreeMap::new(),
        }
    }

    pub fn apply(
        &mut self,
        notification: FilesystemNotification,
    ) -> Option<WatchedImportCandidate> {
        if notification.kind == FilesystemNotificationKind::Removed {
            self.observed_sizes.remove(&notification.path);
            return None;
        }

        let filename = notification.path.file_name()?.to_str()?;
        let extension = notification.path.extension()?.to_str()?;
        let current_size = notification.current_size?;

        if should_ignore_watched_file(filename) || !supported_mvp_format(extension) {
            self.observed_sizes.remove(&notification.path);
            return None;
        }

        let previous_size = self
            .observed_sizes
            .insert(notification.path.clone(), current_size)
            .unwrap_or_default();

        is_stable_watched_file(filename, previous_size, current_size).then_some(
            WatchedImportCandidate {
                path: notification.path,
                previous_size,
                current_size,
            },
        )
    }

    pub fn observed_sizes(&self) -> &BTreeMap<PathBuf, u64> {
        &self.observed_sizes
    }
}

impl Default for FilesystemNotificationBuffer {
    fn default() -> Self {
        Self::new()
    }
}

pub fn import_file(
    catalog: &Catalog,
    library_id: Uuid,
    path: impl AsRef<Path>,
    mode: ImportMode,
) -> Result<AssetRecord, ImportError> {
    import_file_internal(catalog, library_id, path.as_ref(), mode, None)
}

pub fn import_file_with_source_context(
    catalog: &Catalog,
    library_id: Uuid,
    path: impl AsRef<Path>,
    mode: ImportMode,
    source_context: ImportSourceContext,
) -> Result<AssetRecord, ImportError> {
    import_file_internal(
        catalog,
        library_id,
        path.as_ref(),
        mode,
        Some(source_context),
    )
}

fn import_file_internal(
    catalog: &Catalog,
    library_id: Uuid,
    path: &Path,
    mode: ImportMode,
    source_context: Option<ImportSourceContext>,
) -> Result<AssetRecord, ImportError> {
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
    let mut tag_suggestions = search::suggest_tags_from_filename(&original_filename);
    tag_suggestions.extend(metadata_tag_suggestions(path));
    let media_type = infer_media_type_from_suggestions(&tag_suggestions);

    let asset = catalog.register_asset(NewAssetRecord {
        library_id,
        original_filename,
        display_name,
        path: asset_path,
        storage_mode,
        content_hash: Some(content_hash),
        media_type,
        file_size: metadata.file_size,
        availability_state: AvailabilityState::Local,
    })?;

    catalog.enqueue_job(asset.id, JobKind::MetadataExtraction, 10)?;
    // No Hashing job: content_hash above is already a full-file SHA-256, not a partial
    // sample, so there is no further hashing work for a background job to do.
    catalog.enqueue_job(asset.id, JobKind::WaveformGeneration, 30)?;
    suggest_import_tags(catalog, &asset, &tag_suggestions)?;
    if let Some(source_context) = source_context {
        catalog.set_source_record(SourceRecordDraft {
            asset_id: asset.id,
            provider: source_context.provider,
            source_url: source_context.source_url,
            license_type: source_context.license_type,
            license_status: source_context.license_status,
            attribution: source_context.attribution,
            restrictions: source_context.restrictions,
            receipt_path: source_context.receipt_path,
        })?;
    }

    if mode == ImportMode::Managed {
        copy_managed_source(catalog, library_id, path, &asset)?;
    }

    Ok(asset)
}

fn snapshot_file_sizes(folder: &Path) -> Result<BTreeMap<PathBuf, u64>, ImportError> {
    let mut sizes = BTreeMap::new();

    for entry in fs::read_dir(folder)? {
        let entry = entry?;
        let metadata = entry.metadata()?;
        if metadata.is_file() {
            sizes.insert(entry.path(), metadata.len());
        }
    }

    Ok(sizes)
}

fn lightweight_content_hash(path: &Path) -> Result<String, ImportError> {
    let bytes = fs::read(path)?;
    let digest = Sha256::digest(bytes);
    Ok(format!("{digest:x}"))
}

fn infer_media_type_from_suggestions(suggestions: &[search::TagSuggestion]) -> String {
    if let Some(suggestion) = suggestions
        .iter()
        .find(|suggestion| suggestion.facet == "media_type")
    {
        return normalize_media_type(&suggestion.name);
    }

    if suggestions
        .iter()
        .any(|suggestion| matches!(suggestion.facet.as_str(), "action" | "source"))
    {
        return "sound_effect".to_string();
    }

    "other".to_string()
}

fn normalize_media_type(name: &str) -> String {
    name.to_ascii_lowercase()
        .replace(" / ", "_")
        .replace([' ', '-'], "_")
}

fn metadata_tag_suggestions(path: &Path) -> Vec<search::TagSuggestion> {
    let metadata =
        extract_embedded_metadata(path).unwrap_or_else(|_| EmbeddedFileMetadata::default());
    search::suggest_tags_from_embedded_metadata(&search::EmbeddedAudioMetadata {
        title: metadata.title,
        genre: metadata.genre,
        comment: metadata.comment,
    })
}

fn suggest_import_tags(
    catalog: &Catalog,
    asset: &AssetRecord,
    suggestions: &[search::TagSuggestion],
) -> Result<(), ImportError> {
    for suggestion in suggestions {
        let tag = catalog.create_tag(&suggestion.name, &suggestion.facet, true)?;
        let origin = match suggestion.origin {
            search::SuggestionOrigin::Filename => TagOrigin::Filename,
            search::SuggestionOrigin::Metadata => TagOrigin::Metadata,
        };
        catalog.suggest_tag_for_asset(asset.id, tag.id, origin, suggestion.confidence)?;
    }

    Ok(())
}

fn copy_managed_source(
    catalog: &Catalog,
    library_id: Uuid,
    source_path: &Path,
    asset: &AssetRecord,
) -> Result<(), ImportError> {
    let library = catalog
        .get_library(library_id)?
        .ok_or(ImportError::MissingLibrary(library_id))?;
    let relative_path = match &asset.path {
        AssetPath::Managed(relative_path) => relative_path,
        AssetPath::Referenced(_) => return Ok(()),
    };
    let destination = PathBuf::from(library.media_root).join(relative_path);

    if destination.exists() {
        return Ok(());
    }

    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(source_path, destination)?;

    Ok(())
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
    fn watched_folder_discovers_only_stable_supported_audio_files() {
        let folder = unique_watch_folder();
        let ready = folder.join("ready.wav");
        let growing = folder.join("growing.mp3");
        let partial = folder.join("partial.wav.crdownload");
        let unsupported = folder.join("notes.txt");
        fs::write(&ready, b"ready audio").expect("ready");
        fs::write(&growing, b"still growing").expect("growing");
        fs::write(&partial, b"partial").expect("partial");
        fs::write(&unsupported, b"not audio").expect("unsupported");
        fs::create_dir_all(folder.join("nested")).expect("nested");

        let previous_sizes = BTreeMap::from([
            (ready.clone(), 11),
            (growing.clone(), 4),
            (partial.clone(), 7),
            (unsupported.clone(), 9),
        ]);

        let candidates =
            discover_ready_watched_files(&folder, &previous_sizes).expect("discover candidates");

        assert_eq!(
            candidates,
            vec![WatchedImportCandidate {
                path: ready,
                previous_size: 11,
                current_size: 11,
            }]
        );
    }

    #[test]
    fn watched_folder_poller_emits_file_only_after_stable_second_scan() {
        let folder = unique_watch_folder();
        let ready = folder.join("ready.wav");
        fs::write(&ready, b"ready audio").expect("ready");
        let mut poller = WatchedFolderPoller::new(folder.clone());

        let first_scan = poller.poll().expect("first poll");
        let second_scan = poller.poll().expect("second poll");

        assert!(first_scan.is_empty());
        assert_eq!(
            second_scan,
            vec![WatchedImportCandidate {
                path: ready,
                previous_size: 11,
                current_size: 11,
            }]
        );
    }

    #[test]
    fn watched_folder_poller_drops_removed_files_from_size_snapshot() {
        let folder = unique_watch_folder();
        let removed = folder.join("removed.wav");
        fs::write(&removed, b"ready audio").expect("ready");
        let mut poller = WatchedFolderPoller::new(folder.clone());

        assert!(poller.poll().expect("first poll").is_empty());
        fs::remove_file(&removed).expect("remove");

        assert!(poller.poll().expect("second poll").is_empty());
        assert!(!poller.previous_sizes().contains_key(&removed));
    }

    #[test]
    fn filesystem_notifications_emit_candidate_after_stable_repeat_event() {
        let path = PathBuf::from("/watch/impact.wav");
        let mut buffer = FilesystemNotificationBuffer::new();

        let first = buffer.apply(FilesystemNotification {
            path: path.clone(),
            kind: FilesystemNotificationKind::CreatedOrModified,
            current_size: Some(8),
        });
        let second = buffer.apply(FilesystemNotification {
            path: path.clone(),
            kind: FilesystemNotificationKind::CreatedOrModified,
            current_size: Some(8),
        });

        assert_eq!(first, None);
        assert_eq!(
            second,
            Some(WatchedImportCandidate {
                path,
                previous_size: 8,
                current_size: 8,
            })
        );
    }

    #[test]
    fn filesystem_notifications_ignore_partial_unsupported_and_removed_files() {
        let mut buffer = FilesystemNotificationBuffer::new();

        assert_eq!(
            buffer.apply(FilesystemNotification {
                path: PathBuf::from("/watch/partial.wav.crdownload"),
                kind: FilesystemNotificationKind::CreatedOrModified,
                current_size: Some(8),
            }),
            None
        );
        assert_eq!(
            buffer.apply(FilesystemNotification {
                path: PathBuf::from("/watch/notes.txt"),
                kind: FilesystemNotificationKind::CreatedOrModified,
                current_size: Some(8),
            }),
            None
        );

        let path = PathBuf::from("/watch/removed.wav");
        assert_eq!(
            buffer.apply(FilesystemNotification {
                path: path.clone(),
                kind: FilesystemNotificationKind::CreatedOrModified,
                current_size: Some(8),
            }),
            None
        );
        assert_eq!(
            buffer.apply(FilesystemNotification {
                path: path.clone(),
                kind: FilesystemNotificationKind::Removed,
                current_size: None,
            }),
            None
        );
        assert!(!buffer.observed_sizes().contains_key(&path));
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

    #[test]
    fn managed_import_copies_file_into_library_media_root() {
        let catalog_path = unique_catalog_path("managed-import");
        let audio_path = unique_audio_path("managed-impact.wav");
        fs::write(&audio_path, b"managed audio").expect("fixture");
        let media_root = unique_media_root();

        let catalog = Catalog::open(&catalog_path).expect("catalog");
        let library = catalog
            .create_library("Managed Import", media_root.to_string_lossy())
            .expect("library");
        let imported =
            import_file(&catalog, library.id, &audio_path, ImportMode::Managed).expect("import");

        assert_eq!(imported.storage_mode, StorageMode::Managed);
        assert_eq!(
            imported.path,
            AssetPath::Managed("Media/00/managed-impact.wav".to_string())
        );
        assert_eq!(
            fs::read(media_root.join("Media/00/managed-impact.wav")).expect("managed copy"),
            b"managed audio"
        );
        assert_eq!(fs::read(&audio_path).expect("source"), b"managed audio");
    }

    #[test]
    fn import_adds_pending_filename_tag_suggestions() {
        let catalog_path = unique_catalog_path("smart-import-suggestions");
        let audio_path = unique_audio_path("dark-metal-impact.wav");
        fs::write(&audio_path, b"suggestion audio").expect("fixture");

        let catalog = Catalog::open(&catalog_path).expect("catalog");
        catalog.seed_starter_taxonomy().expect("taxonomy");
        let library = catalog
            .create_library("Smart Import", "/library")
            .expect("library");

        let imported =
            import_file(&catalog, library.id, &audio_path, ImportMode::Referenced).expect("import");
        let suggested_tags = catalog
            .pending_suggested_tags(imported.id)
            .expect("suggestions");

        assert!(suggested_tags.iter().any(|tag| tag.name == "Impact"));
        assert!(suggested_tags.iter().any(|tag| tag.name == "Metal"));
    }

    #[test]
    fn import_derives_media_type_from_filename_suggestions() {
        let catalog_path = unique_catalog_path("media-type-import");
        let audio_path = unique_audio_path("dark-metal-impact.wav");
        fs::write(&audio_path, b"classified audio").expect("fixture");

        let catalog = Catalog::open(&catalog_path).expect("catalog");
        let library = catalog
            .create_library("Media Type", "/library")
            .expect("library");

        let imported =
            import_file(&catalog, library.id, &audio_path, ImportMode::Referenced).expect("import");

        assert_eq!(imported.media_type, "sound_effect");
    }

    #[test]
    fn import_uses_embedded_metadata_for_suggestions_and_media_type() {
        let catalog_path = unique_catalog_path("embedded-smart-import");
        let audio_path = unique_audio_path("unknown.wav");
        fs::write(&audio_path, wav_info_fixture()).expect("fixture");

        let catalog = Catalog::open(&catalog_path).expect("catalog");
        let library = catalog
            .create_library("Embedded Smart Import", "/library")
            .expect("library");

        let imported =
            import_file(&catalog, library.id, &audio_path, ImportMode::Referenced).expect("import");
        let suggested_tags = catalog
            .pending_suggested_tags(imported.id)
            .expect("suggestions");

        assert_eq!(imported.media_type, "sound_effect");
        assert!(suggested_tags.iter().any(|tag| tag.name == "Impact"));
        assert!(suggested_tags.iter().any(|tag| tag.name == "Sound Effect"));
    }

    #[test]
    fn import_can_attach_source_license_context() {
        let catalog_path = unique_catalog_path("source-context-import");
        let audio_path = unique_audio_path("licensed-impact.wav");
        fs::write(&audio_path, b"licensed audio").expect("fixture");

        let catalog = Catalog::open(&catalog_path).expect("catalog");
        let library = catalog
            .create_library("Source Context", "/library")
            .expect("library");
        let imported = import_file_with_source_context(
            &catalog,
            library.id,
            &audio_path,
            ImportMode::Referenced,
            ImportSourceContext {
                provider: Some("Boom Library".to_string()),
                source_url: Some("https://example.com/sounds/licensed-impact".to_string()),
                license_type: Some("subscription".to_string()),
                license_status: Some("active".to_string()),
                attribution: Some("Boom Library".to_string()),
                restrictions: Some("client projects allowed".to_string()),
                receipt_path: Some("receipts/boom-2026.pdf".to_string()),
            },
        )
        .expect("import");
        let project = catalog
            .create_collection(library.id, "Trailer", storage::CollectionType::Project)
            .expect("project");
        catalog
            .record_usage_event(
                imported.id,
                Some(project.id),
                storage::UsageEventType::Exported,
                "/project/licensed-impact.wav",
            )
            .expect("usage");

        let report = catalog.project_source_report(project.id).expect("report");

        assert_eq!(report[0].provider.as_deref(), Some("Boom Library"));
        assert_eq!(report[0].license_status.as_deref(), Some("active"));
        assert_eq!(
            report[0].receipt_path.as_deref(),
            Some("receipts/boom-2026.pdf")
        );
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

    fn unique_watch_folder() -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("darkwave-watch-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).expect("watch folder");
        path
    }

    fn unique_media_root() -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("darkwave-media-{}", Uuid::new_v4()));
        fs::create_dir_all(&path).expect("media root");
        path
    }

    fn wav_info_fixture() -> Vec<u8> {
        let mut list_payload = Vec::new();
        list_payload.extend_from_slice(b"INFO");
        append_info_chunk(&mut list_payload, b"INAM", b"Dark Metallic Impact");
        append_info_chunk(&mut list_payload, b"IGNR", b"Sound Effects");

        let mut wav = Vec::new();
        wav.extend_from_slice(b"RIFF");
        wav.extend_from_slice(&((4 + 8 + list_payload.len()) as u32).to_le_bytes());
        wav.extend_from_slice(b"WAVE");
        wav.extend_from_slice(b"LIST");
        wav.extend_from_slice(&(list_payload.len() as u32).to_le_bytes());
        wav.extend_from_slice(&list_payload);
        wav
    }

    fn append_info_chunk(payload: &mut Vec<u8>, id: &[u8; 4], value: &[u8]) {
        payload.extend_from_slice(id);
        payload.extend_from_slice(&(value.len() as u32).to_le_bytes());
        payload.extend_from_slice(value);
        if value.len() % 2 == 1 {
            payload.push(0);
        }
    }
}
