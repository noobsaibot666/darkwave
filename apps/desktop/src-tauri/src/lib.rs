use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

use import_pipeline::{ImportError, ImportMode};
use release_readiness::{ReleaseBlocker, ReleaseReadinessConfig};
use storage::{
    AssetPath, AssetRecord, Catalog, CollectionRecord, CollectionType, JobKind, LibraryRecord,
    SourceRecordDraft, StorageError, TagApprovalState, TagOrigin, TagRecord,
};
use tauri::Manager;
use uuid::Uuid;

const ALL_TAG_ORIGINS: [TagOrigin; 6] = [
    TagOrigin::Filename,
    TagOrigin::Metadata,
    TagOrigin::AcousticModel,
    TagOrigin::UserRule,
    TagOrigin::UserCorrection,
    TagOrigin::Manual,
];

struct CatalogState(Mutex<Catalog>);

#[derive(Debug, serde::Serialize)]
struct ImportFailure {
    filename: String,
    reason: String,
}

#[derive(Debug, serde::Serialize)]
struct ImportFolderResult {
    imported: Vec<AssetRecord>,
    failed: Vec<ImportFailure>,
}

#[derive(Clone, Debug, Eq, PartialEq, serde::Serialize)]
struct ReleaseReadinessItem {
    label: &'static str,
    blocker: &'static str,
    state: &'static str,
}

const RELEASE_ITEM_DEFINITIONS: [(&str, ReleaseBlocker); 8] = [
    ("macOS audit", ReleaseBlocker::MacosAudit),
    ("Windows audit", ReleaseBlocker::WindowsAudit),
    ("Accessibility", ReleaseBlocker::AccessibilityAudit),
    ("Performance", ReleaseBlocker::PerformanceProfile),
    ("Codec packaging", ReleaseBlocker::CodecPackaging),
    ("Codec license", ReleaseBlocker::CodecLicenseReview),
    ("Updates", ReleaseBlocker::UpdateSystem),
    ("Signing", ReleaseBlocker::SigningNotarization),
];

#[tauri::command]
fn healthcheck() -> &'static str {
    library_core::product_codename()
}

fn release_blocker_id(blocker: ReleaseBlocker) -> &'static str {
    match blocker {
        ReleaseBlocker::MacosAudit => "macos_audit",
        ReleaseBlocker::WindowsAudit => "windows_audit",
        ReleaseBlocker::AccessibilityAudit => "accessibility_audit",
        ReleaseBlocker::PerformanceProfile => "performance_profile",
        ReleaseBlocker::CrashRecovery => "crash_recovery",
        ReleaseBlocker::OnboardingDocs => "onboarding_docs",
        ReleaseBlocker::CodecPackaging => "codec_packaging",
        ReleaseBlocker::CodecLicenseReview => "codec_license_review",
        ReleaseBlocker::UpdateSystem => "update_system",
        ReleaseBlocker::SigningNotarization => "signing_notarization",
    }
}

#[tauri::command]
fn release_blockers() -> Vec<&'static str> {
    ReleaseReadinessConfig::code_gates_passed()
        .candidate()
        .blockers()
        .into_iter()
        .map(release_blocker_id)
        .collect()
}

#[tauri::command]
fn release_readiness_items() -> Vec<ReleaseReadinessItem> {
    let current_blockers: HashSet<_> = release_blockers().into_iter().collect();

    RELEASE_ITEM_DEFINITIONS
        .into_iter()
        .map(|(label, blocker)| {
            let blocker = release_blocker_id(blocker);
            let state = if current_blockers.contains(blocker) {
                "Planned"
            } else {
                "Passed"
            };

            ReleaseReadinessItem {
                label,
                blocker,
                state,
            }
        })
        .collect()
}

#[tauri::command]
fn default_preferences() -> preferences::AppPreferences {
    preferences::AppPreferences::default_for_editorial_audio()
}

#[tauri::command]
fn load_app_preferences(app: tauri::AppHandle) -> Result<preferences::AppPreferences, String> {
    let path = preferences_path(&app)?;
    preferences::load_preferences(path).map_err(|error| format!("{error:?}"))
}

#[tauri::command]
fn save_app_preferences(
    app: tauri::AppHandle,
    preferences: preferences::AppPreferences,
) -> Result<(), String> {
    let path = preferences_path(&app)?;
    preferences::save_preferences(path, &preferences).map_err(|error| format!("{error:?}"))
}

fn preferences_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    Ok(dir.join("preferences.json"))
}

#[tauri::command]
fn backup_library(
    app: tauri::AppHandle,
    state: tauri::State<CatalogState>,
    library_id: String,
    backup_dir: String,
) -> Result<backup::BackupPackage, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let library = catalog
        .get_library(library_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "library not found".to_string())?;
    let assets = catalog
        .list_assets(library_id)
        .map_err(storage_error_message)?;
    drop(catalog);

    let manifest = assets
        .iter()
        .filter_map(|asset| {
            let relative_path = match &asset.path {
                AssetPath::Managed(path) | AssetPath::Referenced(path) => path.clone(),
            };
            asset
                .content_hash
                .clone()
                .map(|content_hash| library_sync::ManifestAsset {
                    id: asset.id,
                    relative_path,
                    content_hash,
                })
        })
        .fold(
            library_sync::PortableManifest::new(library_id, 1),
            |manifest, asset| manifest.with_asset(asset),
        );

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    let catalog_path = app_data_dir.join("catalog.sqlite");
    let manifest_path = app_data_dir.join("library.darkwave-manifest.json");
    library_sync::write_manifest_file(&manifest, &manifest_path)
        .map_err(|error| format!("{error:?}"))?;

    std::fs::create_dir_all(&backup_dir)
        .map_err(|error| format!("create backup directory {backup_dir}: {error}"))?;

    let source = backup::BackupSource {
        catalog_path: catalog_path.to_string_lossy().to_string(),
        manifest_path: manifest_path.to_string_lossy().to_string(),
        backup_dir,
    };

    backup::create_backup(
        library_id,
        1,
        library.media_root,
        &source,
        current_time_ms(),
        |from, to| std::fs::copy(from, to).is_ok(),
    )
    .map_err(|error| format!("{error:?}"))
}

/// Restores a catalog from a backup folder produced by `backup_library`. The live
/// SQLite connection is closed (by swapping it out for an in-memory placeholder under
/// the same mutex the rest of the app uses) before the file on disk is touched, and the
/// snapshot is staged next to the live catalog and only `rename`d into place once fully
/// copied, so a failed or partial copy never corrupts the live database.
#[tauri::command]
fn restore_library(app: tauri::AppHandle, state: tauri::State<CatalogState>, backup_dir: String) -> Result<usize, String> {
    let backup_dir = backup_dir.trim_end_matches('/');
    let catalog_snapshot_path = format!("{backup_dir}/catalog.sqlite");
    let manifest_snapshot_path = format!("{backup_dir}/library.darkwave-manifest.json");

    if !std::path::Path::new(&catalog_snapshot_path).exists() {
        return Err("no catalog.sqlite found in the selected backup folder".to_string());
    }
    if !std::path::Path::new(&manifest_snapshot_path).exists() {
        return Err("no manifest found in the selected backup folder".to_string());
    }

    let app_data_dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?;
    let catalog_path = app_data_dir.join("catalog.sqlite");
    let manifest_path = app_data_dir.join("library.darkwave-manifest.json");
    let staged_catalog_path = app_data_dir.join("catalog.sqlite.restoring");

    std::fs::copy(&catalog_snapshot_path, &staged_catalog_path)
        .map_err(|error| format!("stage catalog snapshot: {error}"))?;

    let mut guard = state.0.lock().expect("catalog mutex poisoned");
    let _ = std::fs::copy(&catalog_path, app_data_dir.join("catalog.sqlite.before-restore"));
    drop(std::mem::replace(
        &mut *guard,
        Catalog::open(":memory:").map_err(storage_error_message)?,
    ));

    let swap_result = std::fs::rename(&staged_catalog_path, &catalog_path)
        .map_err(|error| format!("replace live catalog: {error}"))
        .and_then(|_| {
            std::fs::copy(&manifest_snapshot_path, &manifest_path)
                .map(|_| ())
                .map_err(|error| format!("copy manifest snapshot: {error}"))
        });

    let reopened = Catalog::open(&catalog_path).map_err(storage_error_message);

    match (swap_result, reopened) {
        (Ok(()), Ok(catalog)) => {
            let library_count = catalog.list_libraries().map_err(storage_error_message)?.len();
            *guard = catalog;
            Ok(library_count)
        }
        (Err(error), Ok(catalog)) => {
            *guard = catalog;
            Err(error)
        }
        (_, Err(open_error)) => {
            let _ = std::fs::remove_file(&staged_catalog_path);
            *guard = Catalog::open(":memory:").map_err(storage_error_message)?;
            Err(format!("catalog unreadable after restore attempt: {open_error}"))
        }
    }
}

#[tauri::command]
fn supported_drag_targets() -> Vec<&'static str> {
    vec![
        "tag",
        "collection",
        "project",
        "favorite",
        "trash",
        "external_export",
    ]
}

#[tauri::command]
fn default_virtualized_range() -> (usize, usize) {
    let range = viewport::VirtualViewport {
        total_rows: 50_000,
        row_height_px: 52,
        viewport_height_px: 520,
        scroll_top_px: 0,
        overscan_rows: 6,
    }
    .visible_range();

    (range.start, range.end_exclusive)
}

#[tauri::command]
fn default_command_titles() -> Vec<String> {
    command_palette::CommandRegistry::default_audio_workspace()
        .search("")
        .into_iter()
        .map(|command| command.title)
        .collect()
}

#[tauri::command]
fn maintenance_report(
    state: tauri::State<CatalogState>,
    library_id: String,
) -> Result<maintenance::MaintenanceReport, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let assets = catalog
        .list_assets(library_id)
        .map_err(storage_error_message)?;

    let mut findings = Vec::new();

    for asset in &assets {
        if asset.availability_state == shared_types::AvailabilityState::Missing {
            findings.push(maintenance::MaintenanceFinding::missing_media(asset.id));
        }

        let has_license_context = catalog
            .get_source_record(asset.id)
            .map_err(storage_error_message)?
            .is_some();
        if !has_license_context {
            findings.push(maintenance::MaintenanceFinding::license_review_required(
                asset.id,
            ));
        }
    }

    let mut duplicate_groups: std::collections::BTreeMap<String, Vec<Uuid>> =
        std::collections::BTreeMap::new();
    for asset in &assets {
        if let Some(hash) = &asset.content_hash {
            duplicate_groups
                .entry(fingerprint::exact_duplicate_key(hash, asset.file_size))
                .or_default()
                .push(asset.id);
        }
    }
    for (hash, asset_ids) in duplicate_groups {
        if asset_ids.len() > 1 {
            findings.push(maintenance::MaintenanceFinding::duplicate_content(
                hash, asset_ids,
            ));
        }
    }

    let pending_waveforms = catalog
        .pending_job_count(JobKind::WaveformGeneration)
        .map_err(storage_error_message)?;
    for _ in 0..pending_waveforms {
        findings.push(maintenance::MaintenanceFinding {
            kind: maintenance::MaintenanceFindingKind::StaleWaveformCache,
            asset_ids: Vec::new(),
            detail: "Waveform cache should be regenerated".to_string(),
            recommended_action: maintenance::MaintenanceAction::Regenerate,
        });
    }

    Ok(maintenance::MaintenanceReport::from_findings(findings))
}

#[tauri::command]
fn trash_retention_policy_days() -> u64 {
    30
}

fn current_time_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system clock before unix epoch")
        .as_millis() as u64
}

#[tauri::command]
fn move_to_trash(
    state: tauri::State<CatalogState>,
    asset_id: String,
    reason: String,
) -> Result<trash::TrashItem, String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .move_asset_to_trash(asset_id, reason, current_time_ms())
        .map_err(storage_error_message)
}

#[tauri::command]
fn list_trash_items(
    state: tauri::State<CatalogState>,
    library_id: String,
) -> Result<Vec<trash::TrashItem>, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .list_trash_items(library_id)
        .map_err(storage_error_message)
}

#[tauri::command]
fn restore_from_trash(state: tauri::State<CatalogState>, asset_id: String) -> Result<(), String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .restore_asset_from_trash(asset_id)
        .map_err(storage_error_message)
}

#[tauri::command]
fn purge_from_trash(state: tauri::State<CatalogState>, asset_id: String) -> Result<bool, String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let retention_ms = trash_retention_policy_days() * 24 * 60 * 60 * 1000;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .purge_trash_item(asset_id, current_time_ms(), retention_ms)
        .map_err(storage_error_message)
}

#[tauri::command]
fn trash_duplicate_group(
    state: tauri::State<CatalogState>,
    asset_ids: Vec<String>,
) -> Result<usize, String> {
    let asset_ids = asset_ids
        .iter()
        .map(|id| parse_uuid_field(id, "asset id"))
        .collect::<Result<Vec<_>, _>>()?;

    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let mut trashed = 0usize;
    for asset_id in asset_ids.iter().skip(1) {
        catalog
            .move_asset_to_trash(*asset_id, "duplicate content", current_time_ms())
            .map_err(storage_error_message)?;
        trashed += 1;
    }

    Ok(trashed)
}

#[tauri::command]
fn backup_restore_requirements() -> Vec<&'static str> {
    vec!["catalog_snapshot", "portable_manifest", "media_root"]
}

#[tauri::command]
fn media_root_status(
    state: tauri::State<CatalogState>,
    library_id: String,
) -> Result<(String, bool), String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let library = catalog
        .get_library(library_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "library not found".to_string())?;

    let probe = library_sync::probe_media_root(&library.media_root, |path| {
        std::path::Path::new(path).exists()
    });
    let status = match probe.status {
        library_sync::MediaRootStatus::Online => "online",
        library_sync::MediaRootStatus::Offline => "offline",
    }
    .to_string();

    Ok((status, probe.reconnect_validation_required))
}

/// Re-checks every asset's real on-disk availability against the library's media root
/// (updating `availability_state` accordingly) and, when the root is back online, also
/// runs the manifest-based reconnect validation to report exactly which managed paths
/// are still missing after reconnect.
#[tauri::command]
fn validate_reconnect(
    state: tauri::State<CatalogState>,
    library_id: String,
) -> Result<(usize, Option<library_sync::ReconnectValidationReport>), String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let library = catalog
        .get_library(library_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "library not found".to_string())?;
    let assets = catalog
        .list_assets(library_id)
        .map_err(storage_error_message)?;

    let media_root = library.media_root.clone();
    let changed = catalog
        .validate_media_availability(library_id, |path| {
            let candidate = std::path::Path::new(path);
            if candidate.is_absolute() {
                candidate.exists()
            } else {
                std::path::Path::new(&media_root).join(path).exists()
            }
        })
        .map_err(storage_error_message)?;

    let probe = library_sync::probe_media_root(&library.media_root, |path| {
        std::path::Path::new(path).exists()
    });

    let manifest = assets
        .iter()
        .filter_map(|asset| match &asset.path {
            AssetPath::Managed(relative_path) => asset
                .content_hash
                .clone()
                .map(|content_hash| library_sync::ManifestAsset {
                    id: asset.id,
                    relative_path: relative_path.clone(),
                    content_hash,
                }),
            AssetPath::Referenced(_) => None,
        })
        .fold(
            library_sync::PortableManifest::new(library_id, 1),
            |manifest, asset| manifest.with_asset(asset),
        );

    let report = library_sync::plan_reconnect_validation(&manifest, &probe).map(|job| {
        library_sync::validate_reconnect_paths(&job, |path| std::path::Path::new(path).exists())
    });

    Ok((changed, report))
}

#[tauri::command]
fn apply_offline_control(
    mut offline_state: library_sync::OfflineControlState,
    command: library_sync::OfflineControlCommand,
) -> library_sync::OfflineControlState {
    offline_state.apply(command);
    offline_state
}

#[tauri::command]
fn list_libraries(state: tauri::State<CatalogState>) -> Result<Vec<LibraryRecord>, String> {
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog.list_libraries().map_err(storage_error_message)
}

#[tauri::command]
fn create_library(
    state: tauri::State<CatalogState>,
    name: String,
    media_root: String,
) -> Result<LibraryRecord, String> {
    library_core::validate_library_draft(&library_core::LibraryDraft {
        name: name.clone(),
        media_root: media_root.clone(),
    })
    .map_err(|error| error.to_string())?;

    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let library = catalog
        .create_library(name, media_root)
        .map_err(storage_error_message)?;
    catalog
        .seed_starter_taxonomy()
        .map_err(storage_error_message)?;

    Ok(library)
}

#[tauri::command]
fn list_assets(
    state: tauri::State<CatalogState>,
    library_id: String,
) -> Result<Vec<AssetRecord>, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .list_assets(library_id)
        .map_err(storage_error_message)
}

#[tauri::command]
fn search_assets(
    state: tauri::State<CatalogState>,
    library_id: String,
    query: String,
) -> Result<Vec<AssetRecord>, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let parsed = search::parse_natural_language_query(&query);
    let mut search_query = storage::AssetSearchQuery::text(parsed.text);
    if let Some(media_type) = parsed.media_type {
        search_query = search_query.with_media_type(media_type);
    }

    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .search_assets(library_id, search_query)
        .map_err(storage_error_message)
}

#[tauri::command]
fn explain_search_query(query: String) -> Vec<search::VisibleFilter> {
    search::parse_natural_language_query(&query).visible_filters
}

#[tauri::command]
fn export_project_license_report(
    state: tauri::State<CatalogState>,
    project_id: String,
    destination_path: String,
) -> Result<(), String> {
    let project_id = parse_uuid_field(&project_id, "project id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let rows = catalog
        .project_source_report(project_id)
        .map_err(storage_error_message)?
        .into_iter()
        .map(|row| export_pipeline::LicenseReportRow {
            asset_title: row.asset_title,
            original_filename: row.original_filename,
            provider: row.provider,
            source_url: row.source_url,
            license_type: row.license_type,
            license_status: row.license_status,
            attribution: row.attribution,
            restrictions: row.restrictions,
            receipt_path: row.receipt_path,
            usage_status: row.usage_status,
            destination: row.destination,
        })
        .collect::<Vec<_>>();

    let csv = export_pipeline::render_license_report_csv(&rows);
    std::fs::write(&destination_path, csv)
        .map_err(|error| format!("write license report to {destination_path}: {error}"))
}

#[tauri::command]
fn create_browser_state(visible_asset_ids: Vec<String>) -> Result<workspace_state::BrowserState, String> {
    let ids = visible_asset_ids
        .iter()
        .map(|id| parse_uuid_field(id, "asset id"))
        .collect::<Result<Vec<_>, _>>()?;

    Ok(workspace_state::BrowserState::new(ids))
}

#[tauri::command]
fn apply_browser_command(
    mut browser_state: workspace_state::BrowserState,
    command: workspace_state::BrowserCommand,
) -> workspace_state::BrowserState {
    browser_state.apply(command);
    browser_state
}

#[tauri::command]
fn import_folder(
    state: tauri::State<CatalogState>,
    library_id: String,
    folder_path: String,
    mode: String,
) -> Result<ImportFolderResult, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let import_mode = match mode.as_str() {
        "managed" => ImportMode::Managed,
        "referenced" => ImportMode::Referenced,
        other => return Err(format!("unknown import mode: {other}")),
    };

    let mut paths = collect_audio_files(std::path::Path::new(&folder_path))
        .map_err(|error| format!("could not read folder {folder_path}: {error}"))?;
    paths.sort();

    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let mut imported = Vec::new();
    let mut failed = Vec::new();

    for path in paths {
        let filename = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();

        match import_pipeline::import_file(&catalog, library_id, &path, import_mode) {
            Ok(asset) => imported.push(asset),
            Err(ImportError::UnsupportedFormat(_)) => {}
            Err(error) => failed.push(ImportFailure {
                filename,
                reason: error.to_string(),
            }),
        }
    }

    Ok(ImportFolderResult { imported, failed })
}

/// Recursively collects every recognized audio file under `root`, including nested
/// subfolders. Hidden entries (dotfiles/dot-directories, e.g. `.DS_Store`, `.git`) are
/// skipped since real libraries are frequently a mess of nested vendor/pack folders.
fn collect_audio_files(root: &std::path::Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut directories = vec![root.to_path_buf()];

    while let Some(directory) = directories.pop() {
        for entry in std::fs::read_dir(&directory)? {
            let entry = entry?;
            let path = entry.path();
            let is_hidden = path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with('.'));
            if is_hidden {
                continue;
            }

            let file_type = entry.file_type()?;
            if file_type.is_dir() {
                directories.push(path);
            } else if file_type.is_file() {
                let extension = path
                    .extension()
                    .and_then(|extension| extension.to_str())
                    .unwrap_or_default();
                if import_pipeline::is_recognized_audio_extension(extension) {
                    files.push(path);
                }
            }
        }
    }

    Ok(files)
}

/// Re-scans a library's media root for audio files not yet in the catalog (e.g. dropped
/// into a watched NAS folder outside the app) and imports them as referenced assets.
/// Registration is naturally idempotent: `register_asset` matches on content hash and
/// file size, so files already cataloged are returned unchanged rather than duplicated.
#[tauri::command]
fn refresh_library(
    state: tauri::State<CatalogState>,
    library_id: String,
) -> Result<ImportFolderResult, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let library = catalog
        .get_library(library_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "library not found".to_string())?;

    let mut paths = collect_audio_files(std::path::Path::new(&library.media_root))
        .map_err(|error| format!("could not read {}: {error}", library.media_root))?;
    paths.sort();

    let mut imported = Vec::new();
    let mut failed = Vec::new();

    for path in paths {
        let filename = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();

        match import_pipeline::import_file(&catalog, library_id, &path, ImportMode::Referenced) {
            Ok(asset) => imported.push(asset),
            Err(ImportError::UnsupportedFormat(_)) => {}
            Err(error) => failed.push(ImportFailure {
                filename,
                reason: error.to_string(),
            }),
        }
    }

    Ok(ImportFolderResult { imported, failed })
}

#[tauri::command]
fn assets_for_tag(
    state: tauri::State<CatalogState>,
    library_id: String,
    tag_id: String,
) -> Result<Vec<AssetRecord>, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let tag_id = parse_uuid_field(&tag_id, "tag id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .search_assets(
            library_id,
            storage::AssetSearchQuery::text("").with_tag(tag_id),
        )
        .map_err(storage_error_message)
}

#[tauri::command]
fn asset_playback_path(
    state: tauri::State<CatalogState>,
    asset_id: String,
) -> Result<String, String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let asset = catalog
        .get_asset(asset_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "asset not found".to_string())?;

    resolve_asset_path(&catalog, &asset)
}

fn resolve_asset_path(catalog: &Catalog, asset: &AssetRecord) -> Result<String, String> {
    match &asset.path {
        AssetPath::Referenced(path) => Ok(path.clone()),
        AssetPath::Managed(relative_path) => {
            let library = catalog
                .get_library(asset.library_id)
                .map_err(storage_error_message)?
                .ok_or_else(|| "library not found".to_string())?;
            Ok(format!(
                "{}/{}",
                library.media_root.trim_end_matches('/'),
                relative_path
            ))
        }
    }
}

#[tauri::command]
fn mark_waveform_ready(state: tauri::State<CatalogState>, asset_id: String) -> Result<usize, String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .complete_pending_jobs_for_asset(asset_id, JobKind::WaveformGeneration)
        .map_err(storage_error_message)
}

#[tauri::command]
fn process_pending_jobs(state: tauri::State<CatalogState>) -> Result<usize, String> {
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let jobs = catalog
        .pending_jobs_of_kind(JobKind::MetadataExtraction, 50)
        .map_err(storage_error_message)?;

    let mut processed = 0usize;
    for job in jobs {
        let outcome = catalog
            .get_asset(job.asset_id)
            .map_err(storage_error_message)
            .and_then(|asset| asset.ok_or_else(|| "asset not found".to_string()))
            .and_then(|asset| resolve_asset_path(&catalog, &asset))
            .and_then(|path| {
                audio_metadata::extract_embedded_metadata(&path)
                    .map_err(|error| format!("{error:?}"))
            });

        match outcome {
            Ok(embedded) => {
                catalog
                    .set_embedded_metadata(job.asset_id, embedded.title, embedded.genre, embedded.comment)
                    .map_err(storage_error_message)?;
                catalog.complete_job(job.id).map_err(storage_error_message)?;
            }
            Err(_) => {
                catalog.fail_job(job.id).map_err(storage_error_message)?;
            }
        }
        processed += 1;
    }

    Ok(processed)
}

#[tauri::command]
fn list_tags(state: tauri::State<CatalogState>) -> Result<Vec<TagRecord>, String> {
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog.list_tags().map_err(storage_error_message)
}

#[tauri::command]
fn create_tag(
    state: tauri::State<CatalogState>,
    name: String,
    facet: String,
) -> Result<TagRecord, String> {
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .create_tag(name, facet, false)
        .map_err(storage_error_message)
}

#[tauri::command]
fn tags_for_asset(
    state: tauri::State<CatalogState>,
    asset_id: String,
) -> Result<Vec<TagRecord>, String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .tags_for_asset(asset_id)
        .map_err(storage_error_message)
}

#[tauri::command]
fn suggested_tags_for_asset(
    state: tauri::State<CatalogState>,
    asset_id: String,
) -> Result<Vec<TagRecord>, String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .pending_suggested_tags(asset_id)
        .map_err(storage_error_message)
}

#[tauri::command]
fn apply_tag(
    state: tauri::State<CatalogState>,
    asset_ids: Vec<String>,
    tag_id: String,
) -> Result<String, String> {
    let tag_id = parse_uuid_field(&tag_id, "tag id")?;
    let asset_ids = asset_ids
        .iter()
        .map(|id| parse_uuid_field(id, "asset id"))
        .collect::<Result<Vec<_>, _>>()?;

    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let undo_id = catalog
        .apply_tag_to_assets(&asset_ids, tag_id, TagOrigin::Manual)
        .map_err(storage_error_message)?;

    Ok(undo_id.to_string())
}

#[tauri::command]
fn remove_tag(
    state: tauri::State<CatalogState>,
    asset_id: String,
    tag_id: String,
) -> Result<String, String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let tag_id = parse_uuid_field(&tag_id, "tag id")?;

    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let undo_id = catalog
        .remove_tag_from_asset(asset_id, tag_id)
        .map_err(storage_error_message)?;

    Ok(undo_id.to_string())
}

#[tauri::command]
fn accept_suggested_tag(
    state: tauri::State<CatalogState>,
    asset_id: String,
    tag_id: String,
) -> Result<(), String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let tag_id = parse_uuid_field(&tag_id, "tag id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");

    for origin in ALL_TAG_ORIGINS {
        catalog
            .set_tag_approval(asset_id, tag_id, origin, TagApprovalState::Accepted)
            .map_err(storage_error_message)?;
    }

    Ok(())
}

#[tauri::command]
fn reject_suggested_tag(
    state: tauri::State<CatalogState>,
    asset_id: String,
    tag_id: String,
) -> Result<(), String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let tag_id = parse_uuid_field(&tag_id, "tag id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");

    for origin in ALL_TAG_ORIGINS {
        catalog
            .set_tag_approval(asset_id, tag_id, origin, TagApprovalState::Rejected)
            .map_err(storage_error_message)?;
    }

    Ok(())
}

#[tauri::command]
fn set_favorite(
    state: tauri::State<CatalogState>,
    asset_id: String,
    favorite: bool,
) -> Result<(), String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .set_asset_flags(asset_id, Some(favorite), None)
        .map_err(storage_error_message)
}

#[tauri::command]
fn set_reviewed(
    state: tauri::State<CatalogState>,
    asset_id: String,
    reviewed: bool,
) -> Result<(), String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let review_state = if reviewed {
        storage::ReviewState::Reviewed
    } else {
        storage::ReviewState::Unreviewed
    };
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .set_asset_flags(asset_id, None, Some(review_state))
        .map_err(storage_error_message)
}

#[tauri::command]
fn undo_action(state: tauri::State<CatalogState>, undo_id: String) -> Result<(), String> {
    let undo_id = parse_uuid_field(&undo_id, "undo id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog.undo(undo_id).map_err(storage_error_message)
}

#[tauri::command]
fn redo_action(state: tauri::State<CatalogState>, undo_id: String) -> Result<(), String> {
    let undo_id = parse_uuid_field(&undo_id, "undo id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog.redo(undo_id).map_err(storage_error_message)
}

#[tauri::command]
fn list_collections(
    state: tauri::State<CatalogState>,
    library_id: String,
) -> Result<Vec<CollectionRecord>, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .list_collections(library_id)
        .map_err(storage_error_message)
}

#[tauri::command]
fn create_project(
    state: tauri::State<CatalogState>,
    library_id: String,
    name: String,
) -> Result<CollectionRecord, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .create_collection(library_id, name, CollectionType::Project)
        .map_err(storage_error_message)
}

#[tauri::command]
fn add_to_collection(
    state: tauri::State<CatalogState>,
    collection_id: String,
    asset_ids: Vec<String>,
) -> Result<String, String> {
    let collection_id = parse_uuid_field(&collection_id, "collection id")?;
    let asset_ids = asset_ids
        .iter()
        .map(|id| parse_uuid_field(id, "asset id"))
        .collect::<Result<Vec<_>, _>>()?;

    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let undo_id = catalog
        .add_assets_to_collection(collection_id, &asset_ids)
        .map_err(storage_error_message)?;

    Ok(undo_id.to_string())
}

#[tauri::command]
fn assets_in_collection(
    state: tauri::State<CatalogState>,
    collection_id: String,
) -> Result<Vec<AssetRecord>, String> {
    let collection_id = parse_uuid_field(&collection_id, "collection id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .assets_in_collection(collection_id)
        .map_err(storage_error_message)
}

#[tauri::command]
fn get_source_record(
    state: tauri::State<CatalogState>,
    asset_id: String,
) -> Result<Option<SourceRecordDraft>, String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .get_source_record(asset_id)
        .map_err(storage_error_message)
}

#[tauri::command]
fn set_source_record(
    state: tauri::State<CatalogState>,
    draft: SourceRecordDraft,
) -> Result<(), String> {
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .set_source_record(draft)
        .map_err(storage_error_message)
}

#[tauri::command]
fn export_selected_asset(
    state: tauri::State<CatalogState>,
    asset_id: String,
    destination_folder: String,
) -> Result<String, String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let asset = catalog
        .get_asset(asset_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "asset not found".to_string())?;

    let source_path = match &asset.path {
        AssetPath::Referenced(path) => path.clone(),
        AssetPath::Managed(relative_path) => {
            let library = catalog
                .get_library(asset.library_id)
                .map_err(storage_error_message)?
                .ok_or_else(|| "library not found".to_string())?;
            format!(
                "{}/{}",
                library.media_root.trim_end_matches('/'),
                relative_path
            )
        }
    };

    let plan = export_pipeline::plan_editorial_export(export_pipeline::ExportRequest {
        source_path,
        project_media_dir: destination_folder,
        asset_display_name: asset.display_name,
        preset: export_pipeline::ExportPreset::Original,
        range: None,
        intent: export_pipeline::default_editorial_export_intent(),
    })
    .map_err(|error| format!("{error:?}"))?;

    let executed = export_pipeline::execute_original_copy_export(&plan)
        .map_err(|error| format!("{error:?}"))?;

    catalog
        .record_usage_event(
            asset_id,
            None,
            storage::UsageEventType::Exported,
            &executed.destination_path,
        )
        .map_err(storage_error_message)?;

    Ok(executed.destination_path)
}

fn parse_uuid_field(value: &str, label: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value).map_err(|_| format!("invalid {label}: {value}"))
}

fn storage_error_message(error: StorageError) -> String {
    error.to_string()
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let app_data_dir = app
                .path()
                .app_data_dir()
                .expect("resolve app data directory");
            std::fs::create_dir_all(&app_data_dir).expect("create app data directory");

            let catalog = Catalog::open(app_data_dir.join("catalog.sqlite"))
                .expect("open local catalog database");
            app.manage(CatalogState(Mutex::new(catalog)));

            let undo_item =
                tauri::menu::MenuItem::with_id(app, "undo", "Undo", true, Some("CmdOrCtrl+Z"))?;
            let redo_item = tauri::menu::MenuItem::with_id(
                app,
                "redo",
                "Redo",
                true,
                Some("CmdOrCtrl+Shift+Z"),
            )?;
            let edit_menu = tauri::menu::SubmenuBuilder::new(app, "Edit")
                .item(&undo_item)
                .item(&redo_item)
                .separator()
                .cut()
                .copy()
                .paste()
                .select_all()
                .build()?;
            let app_menu = tauri::menu::SubmenuBuilder::new(app, "Darkwave")
                .about(None)
                .separator()
                .quit()
                .build()?;
            let window_menu = tauri::menu::SubmenuBuilder::new(app, "Window")
                .minimize()
                .close_window()
                .build()?;
            let menu = tauri::menu::MenuBuilder::new(app)
                .item(&app_menu)
                .item(&edit_menu)
                .item(&window_menu)
                .build()?;
            app.set_menu(menu)?;

            Ok(())
        })
        .on_menu_event(|app, event| {
            use tauri::Emitter;
            match event.id().as_ref() {
                "undo" => {
                    let _ = app.emit("menu-undo", ());
                }
                "redo" => {
                    let _ = app.emit("menu-redo", ());
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            healthcheck,
            release_blockers,
            release_readiness_items,
            default_preferences,
            supported_drag_targets,
            default_virtualized_range,
            default_command_titles,
            maintenance_report,
            trash_retention_policy_days,
            backup_restore_requirements,
            media_root_status,
            list_libraries,
            create_library,
            list_assets,
            search_assets,
            import_folder,
            refresh_library,
            assets_for_tag,
            asset_playback_path,
            list_tags,
            create_tag,
            tags_for_asset,
            suggested_tags_for_asset,
            apply_tag,
            remove_tag,
            accept_suggested_tag,
            reject_suggested_tag,
            set_favorite,
            set_reviewed,
            undo_action,
            redo_action,
            list_collections,
            create_project,
            add_to_collection,
            assets_in_collection,
            get_source_record,
            set_source_record,
            export_selected_asset,
            load_app_preferences,
            save_app_preferences,
            move_to_trash,
            list_trash_items,
            restore_from_trash,
            purge_from_trash,
            apply_offline_control,
            backup_library,
            restore_library,
            process_pending_jobs,
            mark_waveform_ready,
            trash_duplicate_group,
            explain_search_query,
            create_browser_state,
            apply_browser_command,
            export_project_license_report,
            validate_reconnect
        ])
        .run(tauri::generate_context!())
        .expect("failed to run Darkwave desktop shell");
}

#[cfg(test)]
mod tests {
    #[test]
    fn healthcheck_returns_product_codename() {
        assert_eq!(super::healthcheck(), "Darkwave");
    }

    #[test]
    fn parse_uuid_field_rejects_non_uuid_input() {
        assert!(super::parse_uuid_field("not-a-uuid", "library id").is_err());
    }

    #[test]
    fn parse_uuid_field_accepts_uuid_input() {
        let id = uuid::Uuid::new_v4();
        assert_eq!(super::parse_uuid_field(&id.to_string(), "library id"), Ok(id));
    }

    #[test]
    fn release_blockers_expose_planned_distribution_work() {
        assert_eq!(
            super::release_blockers(),
            vec![
                "codec_packaging",
                "codec_license_review",
                "update_system",
                "signing_notarization"
            ]
        );
    }

    #[test]
    fn release_readiness_items_reflect_current_blockers() {
        let items = super::release_readiness_items();
        let planned: Vec<_> = items
            .iter()
            .filter(|item| item.state == "Planned")
            .map(|item| item.blocker)
            .collect();

        assert_eq!(
            planned,
            vec![
                "codec_packaging",
                "codec_license_review",
                "update_system",
                "signing_notarization"
            ]
        );
        assert_eq!(items[0].label, "macOS audit");
        assert_eq!(items[0].state, "Passed");
    }

    #[test]
    fn default_preferences_expose_audio_workspace_shortcuts() {
        let preferences = super::default_preferences();

        assert_eq!(
            preferences
                .shortcuts
                .binding_for(preferences::CommandId::TogglePlayback),
            Some("Space")
        );
    }

    #[test]
    fn supported_drag_targets_include_classification_and_export() {
        assert_eq!(
            super::supported_drag_targets(),
            vec![
                "tag",
                "collection",
                "project",
                "favorite",
                "trash",
                "external_export"
            ]
        );
    }

    #[test]
    fn default_virtualized_range_keeps_initial_render_bounded() {
        assert_eq!(super::default_virtualized_range(), (0, 16));
    }

    #[test]
    fn default_command_titles_include_import_and_search_first() {
        assert_eq!(
            &super::default_command_titles()[0..2],
            ["Import Folder".to_string(), "Focus Search".to_string()]
        );
    }

    #[test]
    fn trash_retention_policy_defaults_to_30_days() {
        assert_eq!(super::trash_retention_policy_days(), 30);
    }

    #[test]
    fn backup_restore_requirements_include_catalog_manifest_and_media_root() {
        assert_eq!(
            super::backup_restore_requirements(),
            vec!["catalog_snapshot", "portable_manifest", "media_root"]
        );
    }

}
