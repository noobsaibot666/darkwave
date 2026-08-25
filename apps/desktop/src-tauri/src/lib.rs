mod license;
mod security_scoped_bookmark;

use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::Mutex;

use import_pipeline::{ImportError, ImportMode};
use release_readiness::{
    CodecDistributionConfig, ReleaseBlocker, ReleaseReadinessConfig, SigningNotarizationConfig,
    UpdateChannel, UpdateChannelConfig, REQUIRED_PACKAGED_DECODER_EXTENSIONS,
};
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

/// Mac App Store build only: holds each library's live security-scoped
/// access for as long as the app is running (see
/// src/security_scoped_bookmark.rs). Keyed by library id so re-resolving
/// on every list_libraries call is a no-op for libraries already held open.
#[cfg(all(target_os = "macos", not(feature = "direct-dist")))]
struct BookmarkAccessState(Mutex<std::collections::HashMap<Uuid, security_scoped_bookmark::BookmarkAccess>>);

/// In-memory, not persisted — pausing is a "stop for this session" control,
/// not a saved preference. Checked at the top of process_audio_analysis_jobs
/// so a paused queue costs nothing (no claim, no work) rather than churning
/// through claim/reset cycles every background tick.
struct JobControlState {
    audio_analysis_paused: std::sync::atomic::AtomicBool,
}

/// A resident similarity-worker subprocess, spawned once and reused across
/// every analysis job instead of respawning (and re-paying process-start
/// cost) per file. See `run_similarity_worker` for the request/response
/// protocol and why a single `tokio::sync::Mutex` around it is enough to
/// serialize concurrent callers correctly.
struct ResidentSimilarityWorker {
    child: tauri_plugin_shell::process::CommandChild,
    events: tauri::async_runtime::Receiver<tauri_plugin_shell::process::CommandEvent>,
}

struct SimilarityWorkerState(tokio::sync::Mutex<Option<ResidentSimilarityWorker>>);

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
    // Decode coverage for the analysis pipeline (waveform/tempo/pitch/
    // needs-review/similarity) is real and complete via Symphonia — see
    // docs/adr/0025-real-audio-analysis.md — which is what flips codec
    // packaging to Passed below. codec_license_review now has a real
    // reference too: AAC/M4A were removed from the required set entirely
    // (crates/audio-metadata no longer even links Symphonia's AAC decoder)
    // specifically because AAC's patent pool (Via LA) is still active —
    // see docs/adr/0028-defer-aac-decode-pending-patent-question.md for
    // the researched basis. What's left (MP3: patents expired; FLAC/Vorbis:
    // royalty-free by design; AIFF: uncompressed, no codec at all) has no
    // open question, so this is a real closure, not a workaround.
    let codec_distribution = CodecDistributionConfig {
        packaged_decoder_extensions: REQUIRED_PACKAGED_DECODER_EXTENSIONS
            .iter()
            .map(|extension| extension.to_string())
            .collect(),
        license_review_reference: Some(
            "docs/adr/0028-defer-aac-decode-pending-patent-question.md".to_string(),
        ),
    };

    // update_system is now real: web_three/licensing-server's
    // /darkwave/updates/:target/:arch/:currentVersion route is live and
    // verified end to end — a stale-version request returns a real signed
    // manifest (curl'd and confirmed 200 with the actual Ed25519
    // signature), a current-version request correctly returns 204, and
    // the download route serves the real notarized DMG (content-length
    // verified to match the actual file). public_key_id is the minisign
    // key ID from secrets/darkwave-updater.key.pub's own comment line.
    let update_channel = UpdateChannelConfig {
        channel: UpdateChannel::Stable,
        manifest_url:
            "https://alan-design.com/licensing/darkwave/updates/{{target}}/{{arch}}/{{current_version}}"
                .to_string(),
        public_key_id: "4FB33295F15A6FAC".to_string(),
    };

    // macOS signing/notarization is real (see apps/desktop/scripts/
    // deploy_direct_macos.sh and mac_sign_and_package_mas.sh — both
    // produce a real notarized DMG / MAS-signed .pkg with these exact
    // identities). windows_certificate_thumbprint is deliberately left
    // empty: Windows ships unsigned for V1, matching exposeu_wrapkit's
    // (CineFlow Suite) precedent — no EV certificate purchased. Because
    // has_complete_metadata() requires all three fields non-empty,
    // signing_notarization_gate correctly stays Planned despite macOS
    // being fully wired — an accurate reflection of a real, deliberate
    // gap, not a bug to chase (see docs/development/release-readiness.md).
    let signing_notarization = SigningNotarizationConfig {
        macos_developer_id: "Developer ID Application: Nudson Alan Terrinha Alves (RD7UU4Z3D2)"
            .to_string(),
        macos_team_id: "RD7UU4Z3D2".to_string(),
        windows_certificate_thumbprint: String::new(),
    };

    ReleaseReadinessConfig::code_gates_passed()
        .with_codec_distribution(codec_distribution)
        .with_update_channel(update_channel)
        .with_signing_notarization(signing_notarization)
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
    let catalog_snapshot_path = PathBuf::from(&backup_dir)
        .join("catalog.sqlite")
        .to_string_lossy()
        .to_string();
    let manifest_snapshot_path = PathBuf::from(&backup_dir)
        .join("library.darkwave-manifest.json")
        .to_string_lossy()
        .to_string();

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
fn search_commands(query: String) -> Vec<command_palette::PaletteCommand> {
    command_palette::CommandRegistry::default_audio_workspace().search(&query)
}

// (async): iterates every asset in the library plus a duplicate-content
// scan. A plain sync command runs on Tauri's main thread by default and
// would block the whole UI for however long that takes — this used to
// also run one extra query per asset (see asset_ids_with_source_record's
// doc comment), which made it materially worse on every app launch.
#[tauri::command(async)]
fn maintenance_report(
    state: tauri::State<CatalogState>,
    library_id: String,
) -> Result<maintenance::MaintenanceReport, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let assets = catalog
        .list_assets(library_id)
        .map_err(storage_error_message)?;
    let has_source_record = catalog
        .asset_ids_with_source_record(library_id)
        .map_err(storage_error_message)?;

    let mut findings = Vec::new();

    for asset in &assets {
        if asset.availability_state == shared_types::AvailabilityState::Missing {
            findings.push(maintenance::MaintenanceFinding::missing_media(asset.id));
        }

        if !has_source_record.contains(&asset.id) {
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

/// The trash view's "Delete" action — unlike `purge_from_trash` (which only
/// ever removes the catalog row, gated by the retention window), this
/// actually deletes the real file from wherever it lives on disk (the
/// managed library folder or the original referenced location), then
/// finalizes the catalog side. No retention gate: the user explicitly chose
/// this action, on this one item, right now — that confirmation is the
/// safety check, not a time delay. A file that's already missing (moved or
/// deleted outside the app) is treated as success, since the goal state —
/// no file left on disk — already holds.
#[tauri::command]
fn delete_trash_item_permanently(
    state: tauri::State<CatalogState>,
    asset_id: String,
) -> Result<(), String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");

    let asset = catalog
        .get_asset(asset_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "asset not found".to_string())?;

    let absolute_path = resolve_asset_path(&catalog, &asset)?;
    match std::fs::remove_file(&absolute_path) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(format!("failed to delete file: {error}")),
    }

    catalog
        .finalize_permanent_deletion(asset_id)
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

// (async): probes the media root path with a filesystem existence check,
// which for a NAS/SMB path is a network round-trip, not a fast local
// call — a plain sync command would block the main thread for however
// long that takes, on every app launch.
#[tauri::command(async)]
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

    // "not_set" is distinct from "offline": offline implies a root that was
    // reachable before and now isn't (reconnect UI applies), while a library
    // with no root yet has never had anything to probe in the first place.
    if library.media_root.trim().is_empty() {
        return Ok(("not_set".to_string(), false));
    }

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
fn list_libraries(
    app: tauri::AppHandle,
    state: tauri::State<CatalogState>,
) -> Result<Vec<LibraryRecord>, String> {
    let libraries = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        let libraries = catalog.list_libraries().map_err(storage_error_message)?;
        for library in &libraries {
            resolve_library_bookmark_access(&app, &catalog, library);
        }
        libraries
    };
    Ok(libraries)
}

/// Mac App Store build only: regains security-scoped access to a library's
/// media root on this launch, using the bookmark minted the last time its
/// folder was freshly picked (see `store_library_bookmark_for_freshly_picked_folder`).
/// A no-op everywhere else — the direct-sale build is unsandboxed and never
/// needs this.
///
/// Also self-heals `media_root` itself: a security-scoped bookmark tracks a
/// stable file reference, not a path string, so it can still resolve
/// correctly after an SMB/NAS share remounts under a different `/Volumes/…`
/// path than last session. When that happens, every other command in this
/// file that reads `library.media_root` as a plain string would otherwise
/// keep pointing at the stale, now-wrong mount path even though the
/// sandbox access itself is fine — so the resolved path is written back
/// here, once, right after resolution.
#[cfg(all(target_os = "macos", not(feature = "direct-dist")))]
fn resolve_library_bookmark_access(app: &tauri::AppHandle, catalog: &Catalog, library: &LibraryRecord) {
    let Some(bookmark) = library.media_root_bookmark.as_deref() else {
        return;
    };
    let state = app.state::<BookmarkAccessState>();
    let mut held = state.0.lock().expect("bookmark access mutex poisoned");
    if held.contains_key(&library.id) {
        return;
    }
    // A resolution failure (folder moved/deleted, permission revoked) isn't
    // treated as an error here: media_root_status already surfaces
    // "missing media" to the user through the normal offline-detection
    // path, so this fails open rather than erroring the whole library list.
    if let Ok(access) = security_scoped_bookmark::resolve_bookmark(bookmark) {
        if let Some(resolved_path) = access.path() {
            let resolved_path = resolved_path.to_string_lossy();
            if resolved_path != library.media_root {
                let _ = catalog.set_library_media_root(library.id, resolved_path.as_ref());
            }
        }
        held.insert(library.id, access);
    }
}

#[cfg(not(all(target_os = "macos", not(feature = "direct-dist"))))]
fn resolve_library_bookmark_access(
    _app: &tauri::AppHandle,
    _catalog: &Catalog,
    _library: &LibraryRecord,
) {
}

/// Mac App Store build only: mints and persists a security-scoped bookmark
/// for a folder the user just picked (see the call site in import_folder),
/// and immediately holds access open for the rest of this session too —
/// not strictly required (the dialog's own grant already covers this
/// session), but keeps `BookmarkAccessState` consistent with what
/// `resolve_library_bookmark_access` would produce on the next launch.
#[cfg(all(target_os = "macos", not(feature = "direct-dist")))]
fn store_library_bookmark_for_freshly_picked_folder(
    app: &tauri::AppHandle,
    catalog: &Catalog,
    library_id: Uuid,
    folder_path: &str,
) {
    let Ok(bookmark) = security_scoped_bookmark::create_bookmark(std::path::Path::new(folder_path))
    else {
        // Not fatal — media_root is already set from folder_path itself,
        // just without persisted sandbox access across relaunch. Recovery
        // path: media_root_status reports the library unreachable next
        // launch, the user re-picks the folder, which retries this.
        return;
    };
    let _ = catalog.set_library_media_root_bookmark(library_id, Some(&bookmark));

    if let Ok(access) = security_scoped_bookmark::resolve_bookmark(&bookmark) {
        let state = app.state::<BookmarkAccessState>();
        state
            .0
            .lock()
            .expect("bookmark access mutex poisoned")
            .insert(library_id, access);
    }
}

#[cfg(not(all(target_os = "macos", not(feature = "direct-dist"))))]
fn store_library_bookmark_for_freshly_picked_folder(
    _app: &tauri::AppHandle,
    _catalog: &Catalog,
    _library_id: Uuid,
    _folder_path: &str,
) {
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

/// Explicitly sets a library's media root — used by the first-run wizard's
/// "root folder" step, as an alternative to the older implicit path (the
/// first folder someone imports into an empty-media_root library becomes
/// its root, see `import_folder`).
#[tauri::command]
fn set_library_media_root(
    app: tauri::AppHandle,
    state: tauri::State<CatalogState>,
    library_id: String,
    media_root: String,
) -> Result<LibraryRecord, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .set_library_media_root(library_id, &media_root)
        .map_err(storage_error_message)?;
    // The dialog that produced media_root just gave this process sandbox
    // access to it — the one moment a security-scoped bookmark can actually
    // be minted for it (Mac App Store build only; see
    // store_library_bookmark_for_freshly_picked_folder).
    store_library_bookmark_for_freshly_picked_folder(&app, &catalog, library_id, &media_root);
    catalog
        .get_library(library_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "library not found after setting media root".to_string())
}

/// Sets (or clears, with `null`) the folder the app scans for new sounds to
/// auto-import — see `scan_import_folder`. Distinct from `media_root`,
/// which is where the organized library itself lives.
#[tauri::command]
fn set_library_import_root(
    state: tauri::State<CatalogState>,
    library_id: String,
    import_root: Option<String>,
) -> Result<LibraryRecord, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .set_library_import_root(library_id, import_root.as_deref())
        .map_err(storage_error_message)?;
    catalog
        .get_library(library_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "library not found after setting import folder".to_string())
}

/// Removes this library's cached preview files (the local, disposable
/// speed-up copies referenced/NAS assets get, keyed by asset id — see
/// `cached_file_path`), scoped to only this library's assets rather than
/// the whole shared preview-cache directory. Never touches anything under
/// the library's own `media_root`.
#[tauri::command]
fn purge_library_cache(
    app: tauri::AppHandle,
    state: tauri::State<CatalogState>,
    library_id: String,
) -> Result<usize, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let assets = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        catalog.list_assets(library_id).map_err(storage_error_message)?
    };

    let cache_dir = preview_cache_dir(&app)?;
    let mut removed = 0usize;
    for asset in assets {
        let cache_path = cached_file_path(&cache_dir, &asset);
        if cache_path.exists() && std::fs::remove_file(&cache_path).is_ok() {
            removed += 1;
        }
    }

    Ok(removed)
}

/// Permanently removes every currently-trashed asset in this library from
/// the catalog, bypassing the normal retention wait. Same guarantee as the
/// rest of the trash system: only ever deletes catalog rows, never a real
/// file (see `storage::Catalog::empty_trash_for_library`).
#[tauri::command]
fn empty_library_trash(state: tauri::State<CatalogState>, library_id: String) -> Result<usize, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .empty_trash_for_library(library_id)
        .map_err(storage_error_message)
}

/// Counts returned to the UI so "library deleted" can say what it actually
/// cleaned up (cache files off disk, trash rows cascaded away) instead of
/// just confirming the library itself is gone.
#[derive(serde::Serialize)]
struct DeleteLibraryResult {
    cache_files_removed: usize,
    trash_items_cleared: usize,
}

/// Deletes a library and everything the catalog knows about it (assets,
/// tags applied to them, collections, jobs, trash records — see
/// `storage::Catalog::delete_library` for the cascade). Deliberately never
/// touches the filesystem under the library's `media_root`: the source
/// audio a user pointed the library at is never at risk from this action,
/// only Darkwave's own record of it. Also cleans up this library's own
/// cache files (now orphaned) and clears the watched-folder preference if
/// it pointed at the library being removed.
// (async): real file I/O over every asset in the library plus a DB cascade
// — see list_assets/warm_library_cache above for why sync commands doing
// this kind of work can't run on Tauri's main thread.
#[tauri::command(async)]
fn delete_library(
    app: tauri::AppHandle,
    state: tauri::State<CatalogState>,
    library_id: String,
) -> Result<DeleteLibraryResult, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;

    let (assets, trash_items_cleared) = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        let assets = catalog.list_assets(library_id).map_err(storage_error_message)?;
        let trash_items = catalog
            .list_trash_items(library_id)
            .map_err(storage_error_message)?;
        (assets, trash_items.len())
    };

    let mut cache_files_removed = 0usize;
    if let Ok(cache_dir) = preview_cache_dir(&app) {
        for asset in &assets {
            let cache_path = cached_file_path(&cache_dir, asset);
            if cache_path.exists() && std::fs::remove_file(&cache_path).is_ok() {
                cache_files_removed += 1;
            }
        }
    }

    {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        catalog.delete_library(library_id).map_err(storage_error_message)?;
    }

    let preferences_path = preferences_path(&app)?;
    if let Ok(mut preferences) = preferences::load_preferences(&preferences_path) {
        if preferences.watched_folder_library_id.as_deref() == Some(&library_id.to_string()) {
            preferences.watched_folder_path = None;
            preferences.watched_folder_library_id = None;
            let _ = preferences::save_preferences(&preferences_path, &preferences);
        }
    }

    Ok(DeleteLibraryResult {
        cache_files_removed,
        trash_items_cleared,
    })
}

// (async): a plain sync command runs on Tauri's main thread by default,
// and this one (like search_assets/search_assets_advanced below) fires on
// every app launch and every keystroke in the search box — fine at a
// desktop-library scale, but there's no reason to let a large enough
// result set risk stalling the UI when running it off the main thread
// costs nothing.
#[tauri::command(async)]
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

#[tauri::command(async)]
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

/// Faceted-filter shape the frontend sends for both a one-off search and a
/// saved Smart Collection — the two ended up being the same feature, since
/// `create_smart_collection` just stores an `AssetSearchQuery` for later
/// re-evaluation. See docs for the finalization-pass ADR.
#[derive(serde::Deserialize)]
struct AssetSearchFilters {
    text: Option<String>,
    media_type: Option<String>,
    tag_id: Option<String>,
    duration_min_ms: Option<i64>,
    duration_max_ms: Option<i64>,
    bpm_min: Option<f64>,
    bpm_max: Option<f64>,
    peak_db_min: Option<f64>,
    peak_db_max: Option<f64>,
}

fn build_search_query(filters: AssetSearchFilters) -> Result<storage::AssetSearchQuery, String> {
    let tag_id = filters
        .tag_id
        .map(|id| parse_uuid_field(&id, "tag id"))
        .transpose()?;

    Ok(storage::AssetSearchQuery {
        text: filters.text.unwrap_or_default(),
        tag_id,
        media_type: filters.media_type,
        duration_min_ms: filters.duration_min_ms,
        duration_max_ms: filters.duration_max_ms,
        bpm_min: filters.bpm_min,
        bpm_max: filters.bpm_max,
        peak_db_min: filters.peak_db_min,
        peak_db_max: filters.peak_db_max,
    })
}

#[tauri::command(async)]
fn search_assets_advanced(
    state: tauri::State<CatalogState>,
    library_id: String,
    filters: AssetSearchFilters,
) -> Result<Vec<AssetRecord>, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let query = build_search_query(filters)?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .search_assets(library_id, query)
        .map_err(storage_error_message)
}

#[tauri::command]
fn create_smart_collection(
    state: tauri::State<CatalogState>,
    library_id: String,
    name: String,
    filters: AssetSearchFilters,
) -> Result<CollectionRecord, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let query = build_search_query(filters)?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .create_smart_collection(library_id, name, &query)
        .map_err(storage_error_message)
}

#[tauri::command]
fn assets_in_smart_collection(
    state: tauri::State<CatalogState>,
    collection_id: String,
) -> Result<Vec<AssetRecord>, String> {
    let collection_id = parse_uuid_field(&collection_id, "collection id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .assets_in_smart_collection(collection_id)
        .map_err(storage_error_message)
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

// (async): walks the whole folder tree and hashes every file — a plain
// sync command runs on Tauri's main thread by default, so without this the
// window is completely unresponsive (no spinner, no progress, just a
// beachball) for however long the import takes, same class of bug already
// fixed for list_assets/warm_library_cache/etc. above.
#[tauri::command(async)]
fn import_folder(
    app: tauri::AppHandle,
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

    // A library no longer needs a folder picked at creation time — the
    // first folder someone imports into it becomes its media root, which
    // is what turns on Refresh Library and NAS-offline detection from here
    // on. Set unconditionally on the imported folder, not contingent on
    // any file inside it actually matching, since choosing this folder to
    // import from is itself what establishes it as the root.
    {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        let library = catalog
            .get_library(library_id)
            .map_err(storage_error_message)?
            .ok_or_else(|| "library not found".to_string())?;
        if library.media_root.trim().is_empty() {
            catalog
                .set_library_media_root(library_id, &folder_path)
                .map_err(storage_error_message)?;
            // The dialog that produced folder_path just gave this process
            // sandbox access to it — the one moment a security-scoped
            // bookmark can actually be minted for it (Mac App Store build
            // only; see store_library_bookmark_for_freshly_picked_folder).
            store_library_bookmark_for_freshly_picked_folder(
                &app,
                &catalog,
                library_id,
                &folder_path,
            );
        }
    }

    let mut paths = collect_audio_files(std::path::Path::new(&folder_path))
        .map_err(|error| format!("could not read folder {folder_path}: {error}"))?;
    paths.sort();

    let mut imported = Vec::new();
    let mut failed = Vec::new();

    // Lock per file rather than once for the whole scan: hashing each file (and, over a
    // NAS mount, the network read behind it) can take a while, and holding the catalog
    // mutex for the entire loop would block every other command — playback, browsing,
    // tagging — until the whole folder finished importing.
    for path in paths {
        let filename = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();

        let catalog = state.0.lock().expect("catalog mutex poisoned");
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
// (async): walks the entire media root directory tree and hashes every
// new file — a plain sync command would run this on the main thread and
// block the UI (the "sync button" freeze) for however long that scan
// takes, which grows with library size and NAS latency.
#[tauri::command(async)]
fn refresh_library(
    state: tauri::State<CatalogState>,
    library_id: String,
) -> Result<ImportFolderResult, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;

    // Only hold the lock long enough to read the media root and the already-known
    // paths; the (potentially slow, NAS-backed) directory walk and per-file hashing
    // below must not hold it, or every other command blocks until the scan finishes.
    let (media_root, known_paths) = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        let library = catalog
            .get_library(library_id)
            .map_err(storage_error_message)?
            .ok_or_else(|| "library not found".to_string())?;
        let known_paths: HashSet<String> = catalog
            .list_assets(library_id)
            .map_err(storage_error_message)?
            .into_iter()
            .filter_map(|asset| match asset.path {
                AssetPath::Referenced(path) => Some(path),
                AssetPath::Managed(_) => None,
            })
            .collect();
        (library.media_root, known_paths)
    };

    if media_root.trim().is_empty() {
        return Err("This library has no media root yet — import a folder first.".to_string());
    }

    let mut paths = collect_audio_files(std::path::Path::new(&media_root))
        .map_err(|error| format!("could not read {media_root}: {error}"))?;
    paths.sort();

    let mut imported = Vec::new();
    let mut failed = Vec::new();

    for path in paths {
        if known_paths.contains(&path.to_string_lossy().to_string()) {
            continue;
        }

        let filename = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();

        let catalog = state.0.lock().expect("catalog mutex poisoned");
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

/// Scans a library's configured import folder — a staging drop zone,
/// distinct from `media_root` — for audio files and copies new ones into
/// the library as Managed assets. This is the "auto-import" half of the
/// first-run wizard's root-folder/import-folder setup: called once when a
/// library becomes active, and folded into the manual "Refresh Library"
/// action, rather than watched live. A no-op if no import folder is
/// configured. Imported files are left in place in the import folder
/// afterward — `import_file`'s content-hash dedup makes re-scanning
/// already-imported files harmless, and deleting user files automatically
/// is exactly the kind of surprise this app avoids.
// (async): same reasoning as import_folder/refresh_library above — a
// directory walk plus per-file hashing must not block the main thread.
#[tauri::command(async)]
fn scan_import_folder(
    state: tauri::State<CatalogState>,
    library_id: String,
) -> Result<ImportFolderResult, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;

    let import_root = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        catalog
            .get_library(library_id)
            .map_err(storage_error_message)?
            .ok_or_else(|| "library not found".to_string())?
            .import_root
    };
    let Some(import_root) = import_root else {
        return Ok(ImportFolderResult {
            imported: Vec::new(),
            failed: Vec::new(),
        });
    };

    let mut paths = collect_audio_files(std::path::Path::new(&import_root))
        .map_err(|error| format!("could not read {import_root}: {error}"))?;
    paths.sort();

    let mut imported = Vec::new();
    let mut failed = Vec::new();

    for path in paths {
        let filename = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();

        let catalog = state.0.lock().expect("catalog mutex poisoned");
        match import_pipeline::import_file(&catalog, library_id, &path, ImportMode::Managed) {
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
    app: tauri::AppHandle,
    state: tauri::State<CatalogState>,
    asset_id: String,
) -> Result<String, String> {
    let parsed_id = parse_uuid_field(&asset_id, "asset id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let asset = catalog
        .get_asset(parsed_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "asset not found".to_string())?;
    local_asset_path(&app, &catalog, &asset)
}

/// Resolves an asset's path, preferring a locally cached copy over the
/// (possibly NAS-backed) original when one exists.
fn local_asset_path(
    app: &tauri::AppHandle,
    catalog: &Catalog,
    asset: &AssetRecord,
) -> Result<String, String> {
    let resolved = resolve_asset_path(catalog, asset)?;

    if let Ok(cache_dir) = preview_cache_dir(app) {
        let cached_path = cached_file_path(&cache_dir, asset);
        if cached_path.exists() {
            return Ok(cached_path.to_string_lossy().to_string());
        }
    }

    Ok(resolved)
}

fn resolve_asset_path(catalog: &Catalog, asset: &AssetRecord) -> Result<String, String> {
    match &asset.path {
        AssetPath::Referenced(path) => Ok(path.clone()),
        AssetPath::Managed(relative_path) => {
            let library = catalog
                .get_library(asset.library_id)
                .map_err(storage_error_message)?
                .ok_or_else(|| "library not found".to_string())?;
            // relative_path always uses `/` internally (see ImportMode::Managed),
            // independent of host OS — PathBuf::join parses `/` as a separator on
            // every platform including Windows, unlike manual string formatting,
            // which produced double-separator or mixed-separator paths whenever
            // media_root already ended in a native trailing separator.
            Ok(PathBuf::from(&library.media_root)
                .join(relative_path)
                .to_string_lossy()
                .to_string())
        }
    }
}

fn asset_absolute_path(asset: &AssetRecord, media_root: &str) -> String {
    match &asset.path {
        AssetPath::Referenced(path) => path.clone(),
        AssetPath::Managed(relative_path) => PathBuf::from(media_root)
            .join(relative_path)
            .to_string_lossy()
            .to_string(),
    }
}

fn preview_cache_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    let dir = app
        .path()
        .app_data_dir()
        .map_err(|error| format!("resolve app data directory: {error}"))?
        .join("preview-cache");
    std::fs::create_dir_all(&dir)
        .map_err(|error| format!("create preview cache directory: {error}"))?;
    Ok(dir)
}

fn cached_file_path(cache_dir: &std::path::Path, asset: &AssetRecord) -> PathBuf {
    let extension = std::path::Path::new(&asset.original_filename)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("bin");
    cache_dir.join(format!("{}.{}", asset.id, extension))
}

/// Copies referenced (typically NAS-backed) assets into a local cache directory for fast
/// playback, up to the user's configured `preview_cache_limit_mb` budget. Only the initial
/// asset listing holds the catalog mutex; the actual file copies (the slow, network-bound
/// part) happen after it's released, for the same reason `refresh_library` locks per file
/// rather than for the whole operation.
// (async): this runs unconditionally on every app launch (see the
// activeLibraryId effect in App.tsx) and does a filesystem existence
// check plus a possible file copy per asset — a plain sync command would
// run all of that on the main thread, which is exactly what was making
// the app appear to "resync everything and freeze" on every launch,
// worse on Windows/SMB where each check is a real network round-trip.
#[tauri::command(async)]
fn warm_library_cache(app: tauri::AppHandle, state: tauri::State<CatalogState>, library_id: String) -> Result<usize, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let (assets, media_root) = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        let library = catalog
            .get_library(library_id)
            .map_err(storage_error_message)?
            .ok_or_else(|| "library not found".to_string())?;
        let assets = catalog
            .list_assets(library_id)
            .map_err(storage_error_message)?;
        (assets, library.media_root)
    };

    let preferences_path = preferences_path(&app)?;
    let budget_mb = preferences::load_preferences(&preferences_path)
        .map(|preferences| preferences.preview_cache_limit_mb)
        .unwrap_or(2048);
    let budget_bytes = u64::from(budget_mb) * 1024 * 1024;

    let cache_dir = preview_cache_dir(&app)?;
    let mut used_bytes: u64 = std::fs::read_dir(&cache_dir)
        .map(|entries| {
            entries
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| entry.metadata().ok())
                .map(|metadata| metadata.len())
                .sum()
        })
        .unwrap_or(0);

    let mut cached_count = 0usize;
    for asset in assets {
        if used_bytes >= budget_bytes {
            break;
        }

        let cache_path = cached_file_path(&cache_dir, &asset);
        if cache_path.exists() {
            continue;
        }

        let source_path = asset_absolute_path(&asset, &media_root);
        let Ok(metadata) = std::fs::metadata(&source_path) else {
            continue;
        };
        if used_bytes + metadata.len() > budget_bytes {
            continue;
        }

        // Copy to a sibling .partial path first, then rename into place —
        // `std::fs::copy` writes directly to its destination as it goes, so
        // a reader checking `cache_path.exists()` (local_asset_path, used by
        // audio analysis) could previously see the file the instant it's
        // created and start decoding it mid-copy. Large files over a slow
        // NAS/WiFi link take long enough to copy that this is a real risk,
        // not just theoretical. A same-filesystem rename is atomic, so
        // cache_path only ever exists once the copy is actually complete.
        let partial_path = cache_dir.join(format!(
            "{}.partial",
            cache_path.file_name().unwrap_or_default().to_string_lossy()
        ));
        if std::fs::copy(&source_path, &partial_path).is_ok() && std::fs::rename(&partial_path, &cache_path).is_ok() {
            used_bytes += metadata.len();
            cached_count += 1;
        } else {
            let _ = std::fs::remove_file(&partial_path);
        }
    }

    Ok(cached_count)
}

#[tauri::command]
fn purge_preview_cache(app: tauri::AppHandle) -> Result<(), String> {
    let cache_dir = preview_cache_dir(&app)?;
    for entry in std::fs::read_dir(&cache_dir).map_err(|error| format!("read cache directory: {error}"))? {
        let entry = entry.map_err(|error| format!("read cache entry: {error}"))?;
        let _ = std::fs::remove_file(entry.path());
    }
    Ok(())
}

#[tauri::command]
fn mark_waveform_ready(state: tauri::State<CatalogState>, asset_id: String) -> Result<usize, String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .complete_pending_jobs_for_asset(asset_id, JobKind::WaveformGeneration)
        .map_err(storage_error_message)
}

// (async): reads embedded metadata from each pending asset's file (ADR
// 0021) — real file I/O, run on every app launch and every background
// tick, so it shouldn't be on the main thread by default.
#[tauri::command(async)]
fn process_pending_jobs(state: tauri::State<CatalogState>) -> Result<usize, String> {
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let jobs = catalog
        .claim_pending_jobs(JobKind::MetadataExtraction, 50)
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
            Err(error) => {
                catalog.fail_job(job.id, &error).map_err(storage_error_message)?;
            }
        }
        processed += 1;
    }

    Ok(processed)
}

#[derive(Debug, serde::Serialize)]
struct JobStatusEntry {
    kind: String,
    pending: usize,
    failed: usize,
    completed: usize,
}

/// Library-scoped pending/failed/completed counts for the two job kinds the
/// frontend can actually drive to completion (`process_pending_jobs`,
/// `process_audio_analysis_jobs`). WaveformGeneration is deliberately
/// excluded — it completes per-asset when the frontend previews a sound,
/// not via any batch command, so there's no queue to report progress on.
/// failed/completed exist so a finished batch can report what actually
/// happened (and how many failed) instead of just disappearing silently.
#[tauri::command]
fn job_status(
    state: tauri::State<CatalogState>,
    library_id: String,
) -> Result<Vec<JobStatusEntry>, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");

    [
        (JobKind::MetadataExtraction, "metadata_extraction"),
        (JobKind::AudioAnalysis, "audio_analysis"),
    ]
    .into_iter()
    .map(|(kind, label)| {
        catalog
            .job_state_counts_for_library(library_id, kind)
            .map(|counts| JobStatusEntry {
                kind: label.to_string(),
                pending: counts.pending,
                failed: counts.failed,
                completed: counts.completed,
            })
            .map_err(storage_error_message)
    })
    .collect()
}

fn parse_job_kind_field(kind: &str) -> Result<JobKind, String> {
    match kind {
        "metadata_extraction" => Ok(JobKind::MetadataExtraction),
        "audio_analysis" => Ok(JobKind::AudioAnalysis),
        other => Err(format!("unknown job kind: {other}")),
    }
}

/// Explicit, user-initiated "Retry Failed" action — see
/// `Catalog::retry_failed_jobs_for_library` for why this needs to exist
/// separately from the standing worker's attempt-capped auto-requeue: a fix
/// that resolves the actual root cause (e.g. the 24-bit WAV decode bug)
/// makes jobs that already exhausted their 3 automatic attempts worth
/// trying again, and the cap has no way to know that on its own.
#[tauri::command]
fn retry_failed_jobs(
    state: tauri::State<CatalogState>,
    library_id: String,
    kind: String,
) -> Result<usize, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let job_kind = parse_job_kind_field(&kind)?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .retry_failed_jobs_for_library(library_id, job_kind)
        .map_err(storage_error_message)
}

/// Distinct file extensions currently failing for one library + kind — see
/// `Catalog::failed_job_extensions_for_library`. Queried once when building
/// a completion summary, not on every job_status poll, since it's only
/// useful once there's actually something to report.
#[tauri::command]
fn failed_job_extensions(
    state: tauri::State<CatalogState>,
    library_id: String,
    kind: String,
) -> Result<Vec<String>, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let job_kind = parse_job_kind_field(&kind)?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .failed_job_extensions_for_library(library_id, job_kind)
        .map_err(storage_error_message)
}

#[derive(Clone, Copy, serde::Serialize)]
struct AudioAnalysisProgressEvent {
    succeeded: bool,
}

/// Real, content-based needs-review detection, best-effort action-tag
/// suggestions, tempo/pitch estimates, and (via the isolated GPL subprocess)
/// a similarity feature vector — see docs/adr/0025-real-audio-analysis.md.
///
/// The catalog mutex is only ever held for brief, synchronous reads/writes
/// around this — never across decode, DSP, or the subprocess await, which
/// together can take real time per asset. Holding the mutex across slow
/// work is the exact bug this project hit twice before (ADR 0023/0024).
// Claims and fully analyzes up to 20 jobs per call — real decode+DSP+VAD
// work per file, so a batch can easily take minutes. The frontend's
// progress bar only updates once this whole call resolves, which without
// the per-job emit below means it sits frozen at 0% for that entire
// batch despite real work happening (CPU-visible, but invisible in the
// UI) — exactly the "stuck a long time at 0%" symptom this event fixes.
// Each emit lets the frontend decrement its local pending count in near
// real time instead of waiting on the batch as a single unit.
#[tauri::command]
async fn process_audio_analysis_jobs(
    app: tauri::AppHandle,
    state: tauri::State<'_, CatalogState>,
    job_control: tauri::State<'_, JobControlState>,
) -> Result<usize, String> {
    use futures::stream::{self, StreamExt};
    use std::sync::atomic::Ordering;

    if job_control.audio_analysis_paused.load(Ordering::SeqCst) {
        return Ok(0);
    }

    let jobs = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        // Self-heals jobs left 'processing' by an abandoned claim (a paused
        // batch, a dev-rebuild restart, a crash) before claiming more — see
        // reset_stuck_processing_jobs. Without this they're stuck forever:
        // claim only selects 'pending', and the failed-job requeue never
        // touches 'processing' rows.
        catalog
            .reset_stuck_processing_jobs(JobKind::AudioAnalysis)
            .map_err(storage_error_message)?;
        catalog
            .claim_pending_jobs(JobKind::AudioAnalysis, 20)
            .map_err(storage_error_message)?
    };

    let worker_state = app.state::<SimilarityWorkerState>();

    // Bounded rather than "all claimed jobs at once": the CPU-bound
    // decode/DSP/VAD work already gets real OS-thread parallelism via
    // spawn_blocking inside analyze_asset_audio, so this just caps how many
    // files are in flight together at a time — a big batch shouldn't launch
    // 20 concurrent NAS reads simultaneously. Previously this was a strict
    // one-at-a-time loop, which left a 32-thread machine mostly idle during
    // a large import (see docs/.../bug-log.md OPEN-3).
    const AUDIO_ANALYSIS_CONCURRENCY: usize = 4;

    let mut results = stream::iter(jobs)
        .map(|job| process_one_audio_analysis_job(&app, &state, &job_control, worker_state.inner(), job))
        .buffer_unordered(AUDIO_ANALYSIS_CONCURRENCY);

    let mut processed = 0usize;
    while let Some(counted) = results.next().await {
        if counted {
            processed += 1;
        }
    }

    Ok(processed)
}

/// Processes a single already-claimed audio-analysis job: resolves the
/// asset, analyzes it (or defers it if it isn't cached locally yet),
/// persists the result, and emits a progress event. Returns `false` only
/// when the queue was paused before this job's turn came up — the job is
/// left `'processing'` and picked back up by `reset_stuck_processing_jobs`
/// next time, the same "leave it for later, don't lose it" behavior the
/// pre-concurrency version had when it broke out of its loop early on pause.
///
/// A per-job storage error is logged and treated as a skip rather than
/// aborting the whole batch (the previous sequential version's `?` would
/// have failed the entire command on any single job's DB error) — with
/// several jobs now running concurrently, one bad write shouldn't take
/// down every other job already in flight, and the standing background
/// worker retries pending/failed work on its own schedule regardless.
async fn process_one_audio_analysis_job(
    app: &tauri::AppHandle,
    state: &tauri::State<'_, CatalogState>,
    job_control: &tauri::State<'_, JobControlState>,
    worker_state: &SimilarityWorkerState,
    job: storage::JobRecord,
) -> bool {
    use std::sync::atomic::Ordering;
    use tauri::Emitter;

    if job_control.audio_analysis_paused.load(Ordering::SeqCst) {
        return false;
    }

    let asset = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        catalog.get_asset(job.asset_id)
    };
    let asset = match asset {
        Ok(asset) => asset,
        Err(error) => {
            eprintln!("audio-analysis: failed to load asset {}: {error:?}", job.asset_id);
            return false;
        }
    };
    let Some(asset) = asset else {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        if let Err(error) = catalog.fail_job(job.id, "asset not found") {
            eprintln!("audio-analysis: failed to record missing-asset failure: {error:?}");
        }
        let _ = app.emit("audio-analysis-progress", AudioAnalysisProgressEvent { succeeded: false });
        return true;
    };

    let local_path = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        local_asset_path(app, &catalog, &asset)
    };
    // Referenced/NAS assets not yet warmed into the local cache: leave the
    // job pending rather than failing it, so a later warm+retry can pick it
    // up (mirrors how playback already treats an uncached path). Reported
    // as "succeeded" to the UI since this isn't a real failure —
    // reset_stuck_processing_jobs makes it claimable again next round.
    let Ok(local_path) = local_path else {
        let _ = app.emit("audio-analysis-progress", AudioAnalysisProgressEvent { succeeded: true });
        return true;
    };
    if !std::path::Path::new(&local_path).exists() {
        let _ = app.emit("audio-analysis-progress", AudioAnalysisProgressEvent { succeeded: true });
        return true;
    }

    let outcome = analyze_asset_audio(app, worker_state, &local_path).await;
    let succeeded = outcome.is_ok();

    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let saved = match outcome {
        Ok(outcome) => save_audio_analysis_outcome(
            &catalog,
            job.asset_id,
            job.id,
            &asset.media_type,
            asset.review_state,
            outcome,
        ),
        Err(error) => catalog.fail_job(job.id, &error).map_err(storage_error_message),
    };
    if let Err(error) = saved {
        eprintln!("audio-analysis: failed to persist result for job {}: {error}", job.id);
    }

    let _ = app.emit("audio-analysis-progress", AudioAnalysisProgressEvent { succeeded });
    true
}

fn save_audio_analysis_outcome(
    catalog: &Catalog,
    asset_id: Uuid,
    job_id: Uuid,
    current_media_type: &str,
    review_state: storage::ReviewState,
    outcome: AudioAnalysisOutcome,
) -> Result<(), String> {
    let duration_ms = outcome.update.duration_ms;
    let bpm = outcome.update.bpm;
    let bpm_confidence = outcome.update.bpm_confidence;
    let vocal_ratio = outcome.vocal_ratio.map(|ratio| ratio as f64);

    catalog
        .set_audio_analysis(asset_id, outcome.update)
        .map_err(storage_error_message)?;
    catalog
        .set_vocal_ratio(asset_id, vocal_ratio)
        .map_err(storage_error_message)?;

    if outcome.needs_review {
        catalog
            .set_media_type(asset_id, "needs_review")
            .map_err(storage_error_message)?;
    } else if current_media_type == "other"
        || (current_media_type == "sound_effect" && review_state == storage::ReviewState::Unreviewed)
    {
        // "First funnel": import-time classification only has a filename/
        // embedded-metadata keyword guess, or a crude file-size heuristic
        // that assumes uncompressed audio (~28s per 5MB) and can lock a
        // compressed (mp3/aac) long vocal track to "sound_effect" before any
        // real signal is known (bug-log-v2). Now that real duration, tempo,
        // and vocal ratio are known, reclassify from actual acoustic signal
        // — but only when nothing more specific already claimed this asset:
        // an import-time keyword match, any manual media type other than
        // "sound_effect", or a "sound_effect" the user has already reviewed
        // (and therefore explicitly confirmed) are all left untouched.
        if let Some(media_type) =
            audio_analysis::classify_media_type_from_analysis(duration_ms, bpm, bpm_confidence, vocal_ratio)
        {
            catalog
                .set_media_type(asset_id, media_type)
                .map_err(storage_error_message)?;
        }
    }

    for tag_name in outcome.suggested_tags {
        let tag = catalog
            .create_tag(tag_name, "action", true)
            .map_err(storage_error_message)?;
        catalog
            .suggest_tag_for_asset(asset_id, tag.id, TagOrigin::AcousticModel, 0.6)
            .map_err(storage_error_message)?;
    }

    catalog.complete_job(job_id).map_err(storage_error_message)
}

/// Pausing is a session-only control (not a saved preference): stops new
/// audio-analysis batches from being claimed, without discarding queued
/// work — reset_stuck_processing_jobs picks any interrupted batch back up
/// once unpaused. See JobControlState.
#[tauri::command]
fn set_audio_analysis_paused(job_control: tauri::State<JobControlState>, paused: bool) {
    job_control
        .audio_analysis_paused
        .store(paused, std::sync::atomic::Ordering::SeqCst);
}

#[tauri::command]
fn audio_analysis_paused(job_control: tauri::State<JobControlState>) -> bool {
    job_control.audio_analysis_paused.load(std::sync::atomic::Ordering::SeqCst)
}

struct AudioAnalysisOutcome {
    needs_review: bool,
    suggested_tags: Vec<&'static str>,
    vocal_ratio: Option<f32>,
    update: storage::AudioAnalysisUpdate,
}

async fn analyze_asset_audio(
    app: &tauri::AppHandle,
    worker_state: &SimilarityWorkerState,
    path: &str,
) -> Result<AudioAnalysisOutcome, String> {
    // Decode + DSP + VAD inference are all synchronous, CPU-bound Rust —
    // running them inline in this async fn would occupy one of Tauri's
    // async-runtime worker threads for the whole duration. Those same
    // worker threads service every other Tauri command (every UI click,
    // every list/search query), so that previously stalled the entire app
    // for as long as this took, which is what made import look and feel
    // like a freeze rather than "some background work is happening."
    // spawn_blocking moves it onto Tokio's separate blocking-thread pool,
    // where it can run at full CPU cost without starving the UI thread.
    let path_owned = path.to_string();
    let (needs_review, suggested_tags, vocal_ratio, mut update) =
        tauri::async_runtime::spawn_blocking(move || -> Result<_, String> {
            let buffer = audio_metadata::decode_any_supported_audio(&path_owned)
                .map_err(|error| format!("{error:?}"))?;

            let needs_review = audio_analysis::is_likely_silent_or_corrupt(&buffer);
            let measurements = audio_analysis::measure(&buffer);
            let suggested_tags = audio_analysis::suggest_action_tags(&buffer, measurements)
                .into_iter()
                .map(|tag| tag.as_str())
                .collect::<Vec<_>>();
            let tempo = audio_analysis::estimate_tempo(&buffer);
            let pitch = audio_analysis::estimate_pitch(&buffer);
            let vocal_ratio = audio_analysis::detect_vocal_ratio(&buffer);

            let channels = buffer.channels.max(1) as u64;
            let duration_ms = if buffer.sample_rate > 0 {
                Some((buffer.samples.len() as f64 / channels as f64 / buffer.sample_rate as f64 * 1000.0) as i64)
            } else {
                None
            };

            let update = storage::AudioAnalysisUpdate {
                duration_ms,
                sample_rate: Some(buffer.sample_rate as i64),
                bit_depth: None,
                channels: Some(buffer.channels as i64),
                loudness_lufs: None,
                peak_db: Some(measurements.peak_db as f64),
                bpm: tempo.map(|estimate| estimate.bpm as f64),
                bpm_confidence: tempo.map(|estimate| estimate.confidence as f64),
                musical_key: pitch.as_ref().map(|estimate| estimate.note_name.clone()),
                key_confidence: pitch.as_ref().map(|estimate| estimate.clarity as f64),
                perceptual_fingerprint: None,
            };

            Ok((needs_review, suggested_tags, vocal_ratio, update))
        })
        .await
        .map_err(|error| format!("audio analysis task panicked: {error}"))??;

    update.perceptual_fingerprint = run_similarity_worker(app, worker_state, path).await;

    Ok(AudioAnalysisOutcome {
        needs_review,
        suggested_tags,
        vocal_ratio,
        update,
    })
}

/// Sends one file path to the resident similarity-worker subprocess
/// (spawning it on first use, or respawning it if a previous call left it
/// dead) and returns its parsed fingerprint. Returns `None` on any failure
/// (missing sidecar, decode error, malformed output, or timeout) —
/// similarity is a nice-to-have, never a reason to fail or indefinitely
/// stall the whole analysis job.
///
/// A single subprocess, reused across every call, replaces the previous
/// spawn-a-fresh-process-per-file approach (see docs/.../bug-log.md's
/// OPEN-3): every file used to pay a full process-start cost on top of its
/// actual analysis time. Holding `worker_state`'s mutex for the whole
/// "write request, read its one response line" round trip is enough to
/// serialize concurrent callers correctly — the worker processes requests
/// one at a time, in the order it receives them (see
/// `crates/similarity-worker`'s `--stdin-loop` mode), so whoever holds the
/// lock is guaranteed to read back its own response, never someone else's.
/// This does mean concurrent analysis jobs take turns at the fingerprint
/// step specifically, even though their much heavier decode/DSP/VAD work
/// (see `analyze_asset_audio`) runs fully in parallel — a small pool of
/// resident workers instead of one would remove that serialization point
/// too, but isn't implemented here; the fixed per-file spawn/model-start
/// cost this replaces was the actually-measured problem, not fingerprint
/// throughput itself, so this is deliberately the simpler fix until
/// real-world measurement says otherwise.
async fn run_similarity_worker(
    app: &tauri::AppHandle,
    worker_state: &SimilarityWorkerState,
    path: &str,
) -> Option<String> {
    use tauri_plugin_shell::process::CommandEvent;
    use tauri_plugin_shell::ShellExt;

    let mut guard = worker_state.0.lock().await;

    if guard.is_none() {
        let sidecar = app.shell().sidecar("similarity-worker").ok()?;
        let (events, child) = sidecar.args(["--stdin-loop"]).spawn().ok()?;
        *guard = Some(ResidentSimilarityWorker { child, events });
    }

    let worker = guard.as_mut().expect("just ensured it's Some");
    let wrote_request =
        worker.child.write(path.as_bytes()).is_ok() && worker.child.write(b"\n").is_ok();
    if !wrote_request {
        if let Some(dead_worker) = guard.take() {
            let _ = dead_worker.child.kill();
        }
        return None;
    }

    let worker = guard.as_mut().expect("just ensured it's Some");
    // Deliberately generous — a debug build of the sidecar (unoptimized)
    // has been observed taking 20-40s for an ordinary file even before
    // this change, so this is headroom for that, not a tight production
    // budget. Without any bound at all, one pathological file hangs this
    // job's slot forever with nothing to recover it.
    let response = tokio::time::timeout(std::time::Duration::from_secs(90), async {
        loop {
            match worker.events.recv().await {
                Some(CommandEvent::Stdout(bytes)) => return Some(bytes),
                // Surfaced to the terminal rather than silently dropped: a
                // Rust panic in the worker (e.g. bliss-audio choking on a
                // pathological file) prints here, which is the only signal
                // that would otherwise explain an unexpected respawn below.
                Some(CommandEvent::Stderr(bytes)) => {
                    eprintln!("similarity-worker stderr: {}", String::from_utf8_lossy(&bytes));
                    continue;
                }
                // Error/Terminated, any future non-exhaustive variant, or
                // the channel closing (None) all mean "no usable response
                // is coming" — treat them the same as a hard failure.
                _ => return None,
            }
        }
    })
    .await;

    let stdout_bytes = match response {
        Ok(Some(bytes)) => bytes,
        _ => {
            // Timed out, the worker errored/exited, or the event channel
            // closed — drop it so the next call spawns a fresh one instead
            // of writing into a dead or desynced pipe.
            if let Some(dead_worker) = guard.take() {
                let _ = dead_worker.child.kill();
            }
            return None;
        }
    };

    let stdout = String::from_utf8_lossy(&stdout_bytes);
    let parsed: serde_json::Value = serde_json::from_str(stdout.trim()).ok()?;
    parsed
        .get("analysis")
        .filter(|value| value.is_array())
        .map(|value| value.to_string())
}

/// Loads every non-trashed asset in the library with a stored similarity
/// vector, ranks by Euclidean distance from the target asset, and returns
/// the closest matches. Brute-force in memory — fine at desktop-library
/// scale, no vector index needed.
#[tauri::command]
fn similar_assets(
    state: tauri::State<CatalogState>,
    library_id: String,
    asset_id: String,
    limit: usize,
) -> Result<Vec<AssetRecord>, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");

    let fingerprints = catalog
        .perceptual_fingerprints(library_id)
        .map_err(storage_error_message)?;

    let target_vector = fingerprints
        .iter()
        .find(|(id, _)| *id == asset_id)
        .and_then(|(_, json)| serde_json::from_str::<Vec<f32>>(json).ok())
        .ok_or_else(|| "asset has not been analyzed yet".to_string())?;

    let mut ranked: Vec<(Uuid, f32)> = fingerprints
        .into_iter()
        .filter(|(id, _)| *id != asset_id)
        .filter_map(|(id, json)| {
            let vector = serde_json::from_str::<Vec<f32>>(&json).ok()?;
            if vector.len() != target_vector.len() {
                return None;
            }
            let distance = euclidean_distance(&target_vector, &vector);
            Some((id, distance))
        })
        .collect();

    ranked.sort_by(|a, b| a.1.total_cmp(&b.1));
    ranked.truncate(limit);

    Ok(ranked
        .into_iter()
        .filter_map(|(id, _)| catalog.get_asset(id).ok().flatten())
        .collect())
}

fn euclidean_distance(a: &[f32], b: &[f32]) -> f32 {
    a.iter()
        .zip(b.iter())
        .map(|(x, y)| (x - y).powi(2))
        .sum::<f32>()
        .sqrt()
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

/// Manual classification from the inspector's Quick Actions. Restricted to
/// the real, user-facing categories — not `needs_review`, which is a
/// system-set flag for corrupt/silent files, not something a user should be
/// able to assign to a perfectly good file by hand.
#[tauri::command]
fn set_media_type(state: tauri::State<CatalogState>, asset_id: String, media_type: String) -> Result<(), String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    if !matches!(
        media_type.as_str(),
        "music" | "sound_effect" | "ambience" | "voiceover" | "foley" | "other"
    ) {
        return Err(format!("unsupported media type: {media_type}"));
    }
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog.set_media_type(asset_id, media_type).map_err(storage_error_message)
}

/// Points a Missing asset at a new file location the user picked, flipping
/// it back to a referenced/local asset. Wraps `storage::relink_asset`,
/// which already did the whole availability-state flip — this is just the
/// first caller.
#[tauri::command]
fn relink_asset(
    state: tauri::State<CatalogState>,
    asset_id: String,
    new_path: String,
) -> Result<(), String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .relink_asset(asset_id, new_path)
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
fn project_memberships_for_library(
    state: tauri::State<CatalogState>,
    library_id: String,
) -> Result<Vec<storage::AssetProjectMembership>, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .project_memberships_for_library(library_id)
        .map_err(storage_error_message)
}

#[tauri::command]
fn create_project(
    state: tauri::State<CatalogState>,
    library_id: String,
    name: String,
    export_path: Option<String>,
    sfx_export_path: Option<String>,
) -> Result<CollectionRecord, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let project = catalog
        .create_collection(library_id, name, CollectionType::Project)
        .map_err(storage_error_message)?;

    if export_path.is_none() && sfx_export_path.is_none() {
        return Ok(project);
    }
    if export_path.is_some() {
        catalog
            .set_collection_export_path(project.id, export_path.as_deref())
            .map_err(storage_error_message)?;
    }
    if sfx_export_path.is_some() {
        catalog
            .set_collection_sfx_export_path(project.id, sfx_export_path.as_deref())
            .map_err(storage_error_message)?;
    }
    catalog
        .get_collection(project.id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "project not found after creation".to_string())
}

#[tauri::command]
fn set_project_export_path(
    state: tauri::State<CatalogState>,
    project_id: String,
    export_path: Option<String>,
) -> Result<CollectionRecord, String> {
    let project_id = parse_uuid_field(&project_id, "project id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .set_collection_export_path(project_id, export_path.as_deref())
        .map_err(storage_error_message)?;
    catalog
        .get_collection(project_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "project not found".to_string())
}

#[tauri::command]
fn set_project_sfx_export_path(
    state: tauri::State<CatalogState>,
    project_id: String,
    sfx_export_path: Option<String>,
) -> Result<CollectionRecord, String> {
    let project_id = parse_uuid_field(&project_id, "project id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .set_collection_sfx_export_path(project_id, sfx_export_path.as_deref())
        .map_err(storage_error_message)?;
    catalog
        .get_collection(project_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "project not found".to_string())
}

/// The "editor's dream" button: copies one sound straight into a project's
/// configured folder (e.g. an editing app's watch folder) so it can be
/// dragged into a timeline immediately, without an export-destination
/// dialog. Reuses the same editorial export pipeline as
/// `export_selected_asset`, just with the destination pre-resolved from one
/// of the project's two configured folders instead of a user-picked one —
/// music goes to `export_path` (the "sound folder"), everything else goes
/// to `sfx_export_path` (the "sound effects folder").
#[tauri::command]
fn export_asset_to_project(
    state: tauri::State<CatalogState>,
    asset_id: String,
    project_id: String,
) -> Result<String, String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let project_id = parse_uuid_field(&project_id, "project id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");

    let project = catalog
        .get_collection(project_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "project not found".to_string())?;

    let asset = catalog
        .get_asset(asset_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "asset not found".to_string())?;

    let is_music = asset.media_type == "music";
    let destination_folder = if is_music {
        project.export_path
    } else {
        project.sfx_export_path
    }
    .ok_or_else(|| {
        if is_music {
            "this project has no sound folder configured".to_string()
        } else {
            "this project has no sound effects folder configured".to_string()
        }
    })?;

    let source_path = match &asset.path {
        AssetPath::Referenced(path) => path.clone(),
        AssetPath::Managed(relative_path) => {
            let library = catalog
                .get_library(asset.library_id)
                .map_err(storage_error_message)?
                .ok_or_else(|| "library not found".to_string())?;
            PathBuf::from(&library.media_root)
                .join(relative_path)
                .to_string_lossy()
                .to_string()
        }
    };

    let primary_tag_name = catalog
        .tags_for_asset(asset_id)
        .map_err(storage_error_message)?
        .into_iter()
        .next()
        .map(|tag| tag.name);
    let category_subfolder = if is_music {
        None
    } else {
        Some(sfx_export_subfolder(&asset.media_type, primary_tag_name.as_deref()))
    };

    let plan = export_pipeline::plan_editorial_export(export_pipeline::ExportRequest {
        source_path: source_path.clone(),
        project_media_dir: destination_folder,
        asset_display_name: asset.display_name,
        preset: export_pipeline::ExportPreset::Original,
        range: None,
        intent: export_pipeline::default_editorial_export_intent(),
        category_subfolder,
    })
    .map_err(|error| format!("{error:?}"))?;

    let destination_path = export_pipeline::execute_original_copy_export(&plan)
        .map_err(|error| format!("{error:?}"))?
        .destination_path;

    catalog
        .record_usage_event(
            asset_id,
            Some(project_id),
            storage::UsageEventType::Exported,
            &destination_path,
        )
        .map_err(storage_error_message)?;

    Ok(destination_path)
}

/// Chooses the subfolder a non-music asset lands in inside a project's
/// sound effects folder. Tag-named when the asset has a primary tag (e.g.
/// `Foley`, `Whoosh`, `Rise`) — mirrors how editors already hand-organize
/// SFX libraries by category — falling back to a readable name keyed off
/// `media_type` when the asset isn't tagged yet.
fn sfx_export_subfolder(media_type: &str, primary_tag_name: Option<&str>) -> String {
    if let Some(tag) = primary_tag_name.filter(|tag| !tag.trim().is_empty()) {
        return export_pipeline::sanitize_filename(tag);
    }
    match media_type {
        "voiceover" => "Voiceover",
        "foley" => "Foley",
        "ambience" => "Ambience",
        "sound_effect" => "Sound Effects",
        _ => "Other",
    }
    .to_string()
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
    format: Option<String>,
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
            PathBuf::from(&library.media_root)
                .join(relative_path)
                .to_string_lossy()
                .to_string()
        }
    };

    let preset = match format.as_deref() {
        Some("wav24") => export_pipeline::ExportPreset::Wav48k24Bit,
        _ => export_pipeline::ExportPreset::Original,
    };

    let plan = export_pipeline::plan_editorial_export(export_pipeline::ExportRequest {
        source_path: source_path.clone(),
        project_media_dir: destination_folder,
        asset_display_name: asset.display_name,
        preset,
        range: None,
        intent: export_pipeline::default_editorial_export_intent(),
        // A manual "export to any folder" pick, not a project send — the
        // user chose this exact destination, so no auto-subfoldering.
        category_subfolder: None,
    })
    .map_err(|error| format!("{error:?}"))?;

    let destination_path = if preset == export_pipeline::ExportPreset::Wav48k24Bit {
        // decode_any_supported_audio is the same Symphonia-backed seam the
        // audio-analysis job uses (docs/adr/0025) — reused here rather than
        // building a second decode path just for export.
        let decoded = audio_metadata::decode_any_supported_audio(&source_path)
            .map_err(|error| format!("{error:?}"))?;
        let rendered = export_pipeline::render_wav_export(
            &plan,
            &export_pipeline::DecodedPcmBuffer {
                sample_rate: decoded.sample_rate,
                channels: decoded.channels,
                samples: decoded.samples,
            },
        )
        .map_err(|error| format!("{error:?}"))?;
        rendered.destination_path
    } else {
        export_pipeline::execute_original_copy_export(&plan)
            .map_err(|error| format!("{error:?}"))?
            .destination_path
    };

    catalog
        .record_usage_event(
            asset_id,
            None,
            storage::UsageEventType::Exported,
            &destination_path,
        )
        .map_err(storage_error_message)?;

    Ok(destination_path)
}

fn parse_uuid_field(value: &str, label: &str) -> Result<Uuid, String> {
    Uuid::parse_str(value).map_err(|_| format!("invalid {label}: {value}"))
}

fn storage_error_message(error: StorageError) -> String {
    error.to_string()
}

// Both this file and Cargo's default release profile leave panics
// unwinding rather than aborting, but `.setup()` runs on the objc runloop
// thread on macOS — a panic unwinding across that FFI boundary is UB and
// in practice just kills the process with no window and no dialog, which
// is indistinguishable from "app failed to launch" to anyone watching
// (including App Review). This hook exists so a startup failure at least
// leaves a paper trail in a fixed, discoverable location before whatever
// happens next happens.
fn startup_log_path() -> PathBuf {
    std::env::temp_dir().join("darkwave-startup.log")
}

fn log_startup_event(message: &str) {
    use std::io::Write;
    let unix_seconds = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    let line = format!("[{unix_seconds}] {message}\n");
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(startup_log_path())
    {
        let _ = file.write_all(line.as_bytes());
    }
    eprintln!("{line}");
}

fn install_startup_panic_hook() {
    let default_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |panic_info| {
        log_startup_event(&format!("PANIC: {panic_info}"));
        default_hook(panic_info);
    }));
}

// Turns a fatal early-startup failure into a visible native dialog instead
// of a silent crash before any window exists — a real dialog satisfies
// "the app launched a window" even when the app itself can't do anything
// useful past that point, and gives whoever hits it (a reviewer, a user, a
// future debugging session) something concrete to report instead of just
// "it never opened."
fn report_fatal_setup_error<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    step: &str,
    error: impl std::fmt::Display,
) {
    use tauri_plugin_dialog::DialogExt;

    let message =
        format!("Darkwave couldn't finish starting up.\n\nFailed to {step}: {error}\n\nLog: {}", startup_log_path().display());
    log_startup_event(&format!("FATAL during startup ({step}): {error}"));

    let _ = app
        .dialog()
        .message(message)
        .kind(tauri_plugin_dialog::MessageDialogKind::Error)
        .title("Darkwave failed to start")
        .blocking_show();
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    install_startup_panic_hook();
    log_startup_event("run() starting");

    let builder = tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_shell::init());

    // MAS build relies entirely on Apple's own updater (see
    // docs/development/release-readiness.md) — this plugin only makes
    // sense for the direct-sale channel.
    #[cfg(feature = "direct-dist")]
    let builder = builder.plugin(tauri_plugin_updater::Builder::new().build());

    builder
        .setup(|app| {
            log_startup_event("setup() entered");

            let app_data_dir = match app.path().app_data_dir() {
                Ok(dir) => dir,
                Err(error) => {
                    report_fatal_setup_error(app.handle(), "resolve the app data directory", error);
                    return Ok(());
                }
            };
            if let Err(error) = std::fs::create_dir_all(&app_data_dir) {
                report_fatal_setup_error(app.handle(), "create the app data directory", error);
                return Ok(());
            }

            let catalog = match Catalog::open(app_data_dir.join("catalog.sqlite")) {
                Ok(catalog) => catalog,
                Err(error) => {
                    report_fatal_setup_error(app.handle(), "open the local catalog database", error);
                    return Ok(());
                }
            };
            log_startup_event("catalog opened");
            app.manage(CatalogState(Mutex::new(catalog)));
            app.manage(JobControlState {
                audio_analysis_paused: std::sync::atomic::AtomicBool::new(false),
            });
            app.manage(SimilarityWorkerState(tokio::sync::Mutex::new(None)));
            #[cfg(all(target_os = "macos", not(feature = "direct-dist")))]
            app.manage(BookmarkAccessState(Mutex::new(std::collections::HashMap::new())));

            // Standing background worker: requeues jobs that failed with
            // retries left, polls the configured watched folder (if any),
            // then tells the frontend to drain whatever's pending. This is
            // what actually fixes jobs only ever processing right after
            // Import/Refresh — everywhere, not just those two triggers, and
            // it's what makes watched-folder import a live feature instead
            // of tested-but-never-invoked library code. A plain thread +
            // sleep, not async/tokio: each tick's own work (a few SQL
            // statements, one directory read) is fast and synchronous, so
            // there's nothing here that benefits from an async runtime.
            let worker_app_handle = app.handle().clone();
            std::thread::spawn(move || {
                use tauri::Emitter;
                // Owned across ticks so its internal size-stabilization
                // state persists between polls, same as import_pipeline's
                // own design intends. Recreated if the configured path
                // changes; cleared if watching is turned off.
                let mut watched_poller: Option<(String, import_pipeline::WatchedFolderPoller)> =
                    None;

                loop {
                    std::thread::sleep(std::time::Duration::from_secs(20));

                    if let Some(state) = worker_app_handle.try_state::<CatalogState>() {
                        let catalog = state.0.lock().expect("catalog mutex poisoned");
                        const MAX_JOB_ATTEMPTS: i64 = 3;
                        let _ = catalog.requeue_failed_jobs(MAX_JOB_ATTEMPTS);

                        if let Ok(preferences_path) = preferences_path(&worker_app_handle) {
                            if let Ok(preferences) = preferences::load_preferences(&preferences_path)
                            {
                                match (
                                    preferences.watched_folder_path,
                                    preferences
                                        .watched_folder_library_id
                                        .as_deref()
                                        .and_then(|id| Uuid::parse_str(id).ok()),
                                ) {
                                    (Some(folder), Some(library_id)) => {
                                        let poller = match &mut watched_poller {
                                            Some((path, poller)) if *path == folder => poller,
                                            _ => {
                                                watched_poller = Some((
                                                    folder.clone(),
                                                    import_pipeline::WatchedFolderPoller::new(
                                                        folder.clone(),
                                                    ),
                                                ));
                                                &mut watched_poller.as_mut().expect("just set").1
                                            }
                                        };

                                        if let Ok(candidates) = poller.poll() {
                                            for candidate in candidates {
                                                let _ = import_pipeline::import_file(
                                                    &catalog,
                                                    library_id,
                                                    &candidate.path,
                                                    import_pipeline::ImportMode::Referenced,
                                                );
                                            }
                                        }
                                    }
                                    _ => watched_poller = None,
                                }
                            }
                        }
                    }

                    let _ = worker_app_handle.emit("background-tick", ());
                }
            });

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
            let license_report_item = tauri::menu::MenuItem::with_id(
                app,
                "export-license-report",
                "Export License Report…",
                true,
                None::<&str>,
            )?;
            let library_menu = tauri::menu::SubmenuBuilder::new(app, "Library")
                .item(&license_report_item)
                .build()?;
            let shortcuts_item = tauri::menu::MenuItem::with_id(
                app,
                "keyboard-shortcuts",
                "Keyboard Shortcuts",
                true,
                Some("CmdOrCtrl+/"),
            )?;
            let help_menu = tauri::menu::SubmenuBuilder::new(app, "Help")
                .item(&shortcuts_item)
                .build()?;
            let window_menu = tauri::menu::SubmenuBuilder::new(app, "Window")
                .minimize()
                .close_window()
                .build()?;
            let menu = tauri::menu::MenuBuilder::new(app)
                .item(&app_menu)
                .item(&edit_menu)
                .item(&library_menu)
                .item(&window_menu)
                .item(&help_menu)
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
                "export-license-report" => {
                    let _ = app.emit("menu-export-license-report", ());
                }
                "keyboard-shortcuts" => {
                    let _ = app.emit("menu-keyboard-shortcuts", ());
                }
                _ => {}
            }
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { .. } = event {
                if let Ok(cache_dir) = preview_cache_dir(window.app_handle()) {
                    if let Ok(entries) = std::fs::read_dir(&cache_dir) {
                        for entry in entries.filter_map(|entry| entry.ok()) {
                            let _ = std::fs::remove_file(entry.path());
                        }
                    }
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            healthcheck,
            release_blockers,
            release_readiness_items,
            default_preferences,
            supported_drag_targets,
            search_commands,
            maintenance_report,
            trash_retention_policy_days,
            backup_restore_requirements,
            media_root_status,
            list_libraries,
            create_library,
            set_library_media_root,
            set_library_import_root,
            purge_library_cache,
            empty_library_trash,
            delete_library,
            list_assets,
            search_assets,
            import_folder,
            refresh_library,
            scan_import_folder,
            assets_for_tag,
            warm_library_cache,
            purge_preview_cache,
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
            set_media_type,
            relink_asset,
            undo_action,
            redo_action,
            list_collections,
            project_memberships_for_library,
            create_project,
            set_project_export_path,
            set_project_sfx_export_path,
            export_asset_to_project,
            add_to_collection,
            assets_in_collection,
            search_assets_advanced,
            create_smart_collection,
            assets_in_smart_collection,
            get_source_record,
            set_source_record,
            export_selected_asset,
            load_app_preferences,
            save_app_preferences,
            move_to_trash,
            list_trash_items,
            restore_from_trash,
            purge_from_trash,
            delete_trash_item_permanently,
            apply_offline_control,
            backup_library,
            restore_library,
            process_pending_jobs,
            process_audio_analysis_jobs,
            set_audio_analysis_paused,
            audio_analysis_paused,
            job_status,
            retry_failed_jobs,
            failed_job_extensions,
            similar_assets,
            mark_waveform_ready,
            trash_duplicate_group,
            explain_search_query,
            create_browser_state,
            apply_browser_command,
            export_project_license_report,
            validate_reconnect,
            license::get_hwid,
            license::get_license_status,
            license::activate_license,
            license::recover_license_key,
            license::init_trial
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
    fn collect_audio_files_recurses_into_nested_subfolders() {
        let root = std::env::temp_dir().join(format!(
            "darkwave-collect-audio-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let nested = root.join("Pack A").join("Impacts");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(root.join("top-level.wav"), b"x").unwrap();
        std::fs::write(nested.join("buried.wav"), b"x").unwrap();
        std::fs::write(nested.join("not-audio.txt"), b"x").unwrap();
        std::fs::write(root.join(".DS_Store"), b"x").unwrap();

        let files = super::collect_audio_files(&root).unwrap();
        std::fs::remove_dir_all(&root).unwrap();

        assert_eq!(files.len(), 2);
        assert!(files.iter().any(|path| path.ends_with("top-level.wav")));
        assert!(files.iter().any(|path| path.ends_with("buried.wav")));
    }

    #[test]
    fn sfx_export_subfolder_prefers_the_primary_tag_over_media_type() {
        assert_eq!(super::sfx_export_subfolder("foley", Some("Door")), "Door");
        assert_eq!(super::sfx_export_subfolder("sound_effect", Some("Whoosh")), "Whoosh");
    }

    #[test]
    fn sfx_export_subfolder_falls_back_to_media_type_when_untagged() {
        assert_eq!(super::sfx_export_subfolder("voiceover", None), "Voiceover");
        assert_eq!(super::sfx_export_subfolder("foley", None), "Foley");
        assert_eq!(super::sfx_export_subfolder("ambience", Some("   ")), "Ambience");
        assert_eq!(super::sfx_export_subfolder("sound_effect", None), "Sound Effects");
        assert_eq!(super::sfx_export_subfolder("needs_review", None), "Other");
    }

    #[test]
    fn release_blockers_expose_planned_distribution_work() {
        // codec_packaging and codec_license_review both pass now: AAC/M4A
        // (the one format with a real open patent question) were removed
        // from the required set entirely rather than shipped with an open
        // question — see docs/adr/0028. update_system passes too: the
        // manifest/download endpoint on licensing-server is live and
        // verified. signing_notarization is the one genuinely, permanently
        // unconfigured field — Windows ships unsigned by deliberate
        // decision, not an oversight (see release-readiness.md).
        assert_eq!(super::release_blockers(), vec!["signing_notarization"]);
    }

    #[test]
    fn release_readiness_items_reflect_current_blockers() {
        let items = super::release_readiness_items();
        let planned: Vec<_> = items
            .iter()
            .filter(|item| item.state == "Planned")
            .map(|item| item.blocker)
            .collect();

        // codec_packaging, codec_license_review, and update_system are all
        // intentionally absent now — see
        // docs/adr/0028-defer-aac-decode-pending-patent-question.md and
        // release-readiness.md's update-system section.
        assert_eq!(planned, vec!["signing_notarization"]);
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
    fn search_commands_include_import_and_apply_tag_first_for_empty_query() {
        let results = super::search_commands(String::new());
        assert_eq!(
            results[0..2]
                .iter()
                .map(|command| command.title.clone())
                .collect::<Vec<_>>(),
            ["Import Folder".to_string(), "Apply Tag".to_string()]
        );
    }

    #[test]
    fn search_commands_filters_by_query() {
        let results = super::search_commands("tag".to_string());
        assert_eq!(results[0].title, "Apply Tag");
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
