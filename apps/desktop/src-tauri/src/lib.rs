use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

use import_pipeline::{ImportError, ImportMode};
use release_readiness::{ReleaseBlocker, ReleaseReadinessConfig};
use storage::{AssetRecord, Catalog, LibraryRecord, StorageError};
use tauri::Manager;
use uuid::Uuid;

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
fn sample_maintenance_summary() -> (usize, &'static str) {
    let report = maintenance::MaintenanceReport::from_findings(Vec::new());
    let severity = match report.severity {
        maintenance::MaintenanceSeverity::Ok => "ok",
        maintenance::MaintenanceSeverity::Warning => "warning",
    };

    (report.total_findings, severity)
}

#[tauri::command]
fn trash_retention_policy_days() -> u64 {
    30
}

#[tauri::command]
fn backup_restore_requirements() -> Vec<&'static str> {
    vec!["catalog_snapshot", "portable_manifest", "media_root"]
}

#[tauri::command]
fn sample_media_root_status() -> (&'static str, bool) {
    let probe = library_sync::probe_media_root("/Volumes/TrueNAS/SFX", |_| false);
    let status = match probe.status {
        library_sync::MediaRootStatus::Online => "online",
        library_sync::MediaRootStatus::Offline => "offline",
    };

    (status, probe.reconnect_validation_required)
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
    let library_id = parse_library_id(&library_id)?;
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
    let library_id = parse_library_id(&library_id)?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .search_assets(library_id, storage::AssetSearchQuery::text(query))
        .map_err(storage_error_message)
}

#[tauri::command]
fn import_folder(
    state: tauri::State<CatalogState>,
    library_id: String,
    folder_path: String,
    mode: String,
) -> Result<ImportFolderResult, String> {
    let library_id = parse_library_id(&library_id)?;
    let import_mode = match mode.as_str() {
        "managed" => ImportMode::Managed,
        "referenced" => ImportMode::Referenced,
        other => return Err(format!("unknown import mode: {other}")),
    };

    let entries = std::fs::read_dir(&folder_path)
        .map_err(|error| format!("could not read folder {folder_path}: {error}"))?;

    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let mut imported = Vec::new();
    let mut failed = Vec::new();

    let mut paths: Vec<PathBuf> = entries
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    paths.sort();

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

fn parse_library_id(value: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value).map_err(|_| format!("invalid library id: {value}"))
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

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            healthcheck,
            release_blockers,
            release_readiness_items,
            default_preferences,
            supported_drag_targets,
            default_virtualized_range,
            default_command_titles,
            sample_maintenance_summary,
            trash_retention_policy_days,
            backup_restore_requirements,
            sample_media_root_status,
            list_libraries,
            create_library,
            list_assets,
            search_assets,
            import_folder
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
    fn parse_library_id_rejects_non_uuid_input() {
        assert!(super::parse_library_id("not-a-uuid").is_err());
    }

    #[test]
    fn parse_library_id_accepts_uuid_input() {
        let id = uuid::Uuid::new_v4();
        assert_eq!(super::parse_library_id(&id.to_string()), Ok(id));
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
    fn sample_maintenance_summary_reports_clean_state() {
        assert_eq!(super::sample_maintenance_summary(), (0, "ok"));
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

    #[test]
    fn sample_media_root_status_reports_offline_without_reconnect_validation() {
        assert_eq!(super::sample_media_root_status(), ("offline", false));
    }
}
