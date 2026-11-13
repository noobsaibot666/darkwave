mod license;
mod power;
mod security_scoped_bookmark;

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Mutex;

use import_pipeline::{ImportError, ImportMode};
use release_readiness::{
    CodecDistributionConfig, ReleaseBlocker, ReleaseReadinessConfig, SigningNotarizationConfig,
    UpdateChannel, UpdateChannelConfig, REQUIRED_PACKAGED_DECODER_EXTENSIONS,
};
use storage::{
    AssetPath, AssetRecord, Catalog, CollectionRecord, CollectionType, FolderRecord, JobKind,
    LibraryRecord, ProjectExportFolderRecord, ResolveSyncStatus, SourceRecordDraft, StorageError,
    TagApprovalState, TagOrigin, TagRecord,
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

/// The currently open library file's path, if the running catalog came from
/// a real `.darkwave` file (New Setup / Open Library / a double-clicked
/// file). `None` while running against the legacy shared app-data
/// `catalog.sqlite` that predates the library-file model, or before any
/// library file has been created/opened this session — `Catalog` itself has
/// no notion of "its own path," so this is tracked alongside it. Named
/// distinctly from Darkwave's unrelated "Project" concept
/// (`CollectionType::Project`, an editorial export destination).
struct ActiveLibraryFileState(Mutex<Option<String>>);

/// A live handle to the File menu's "Open Recent" submenu, kept around so
/// its contents can be rebuilt in place (see `refresh_open_recent_menu`)
/// whenever the recent-library-files list changes — `Submenu` wraps an
/// `Arc` internally (cheap to clone, safe to hold outside the `.setup()`
/// closure that first builds the menu) and its mutation methods
/// (`append`/`remove`/`items`) take `&self`, unlike `App::set_menu` (only
/// available on `App`, not `AppHandle`, so it can't be called again from an
/// arbitrary command handler) which would require rebuilding the entire
/// menu bar just to reflect one changed submenu.
struct OpenRecentMenuState(tauri::menu::Submenu<tauri::Wry>);

/// Snapshot of which library file is active right now, for comparing across
/// a long `.await` (decode/DSP/model inference) that doesn't hold the
/// catalog mutex. See `active_library_changed_since` for why this matters.
fn active_library_snapshot(active_library_file: &tauri::State<'_, ActiveLibraryFileState>) -> Option<String> {
    active_library_file
        .0
        .lock()
        .expect("active library file mutex poisoned")
        .clone()
}

/// True if `open_library_file`/`open_library_file_and_notify_frontend` swapped
/// the active catalog out from under a job while it was mid-`.await`. The
/// catalog mutex is deliberately released across that await (ADR 0023/0024 —
/// holding it there blocks the whole app), but that means a job can resume
/// holding onto a `job`/`asset_id` that belongs to a `Catalog` instance which
/// has since been replaced (and dropped) by `swap_active_catalog`. Writing
/// the result into the *new* catalog would silently no-op (`UPDATE ... WHERE
/// id = ?` matching zero rows there) while still reporting success, and the
/// stale asset would never reappear in the now-active library's canvas —
/// this check exists to catch that instead of letting it happen quietly.
fn active_library_changed_since(
    snapshot: &Option<String>,
    active_library_file: &tauri::State<'_, ActiveLibraryFileState>,
) -> bool {
    active_library_snapshot(active_library_file) != *snapshot
}

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
    /// Same idea as audio_analysis_paused, for WaveformGeneration. Split
    /// out separately (not folded into one job-analysis pause) because a
    /// library catalogued before waveform caching existed can carry a huge
    /// one-time backlog (thousands of Referenced/NAS assets) that pins CPU
    /// for a long time in a debug build — letting the user pause *that*
    /// specifically, without also losing tempo/key/vocal detection, is what
    /// actually gets the UI responsive again.
    waveform_generation_paused: std::sync::atomic::AtomicBool,
}

/// The YAMNet instrument-classification model, loaded once on first use of
/// `process_instrument_jobs`. `Arc<Mutex<…>>` so a blocking inference task
/// can hold the lock for its whole duration off the async worker threads;
/// `tried` records that a load was attempted so a missing model isn't
/// re-probed from disk every background tick. Model absent ⇒ instrument
/// jobs simply wait (see ADR 0031).
struct InstrumentModelState {
    model: std::sync::Arc<Mutex<Option<audio_analysis::InstrumentModel>>>,
    tried: std::sync::atomic::AtomicBool,
}

#[derive(Debug, serde::Serialize)]
struct ImportFailure {
    filename: String,
    reason: String,
}

#[derive(Debug, serde::Serialize)]
struct ImportFolderResult {
    imported: Vec<AssetRecord>,
    failed: Vec<ImportFailure>,
    /// Filenames skipped because they match a `deleted_asset_fingerprints`
    /// tombstone — a file the user already permanently deleted from this
    /// library, found again by a rescan. Not a failure, so kept separate
    /// from `failed`, but still reported rather than silently vanishing.
    #[serde(default)]
    skipped_deleted: Vec<String>,
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

/// Backs up the currently open library file to a single `.darkwavebak` file
/// at `backup_file_path` (a save dialog's result — the frontend defaults it
/// to `"{library name}.darkwavebak"`). Unlike the old shared-catalog era,
/// there's no separate manifest to write and copy alongside it: the
/// `.darkwave` catalog already contains everything (assets, tags, folders)
/// a restore needs, so checkpointing its WAL (see
/// `storage::Catalog::checkpoint_wal`, the same mechanism
/// `migrate_legacy_library` relies on for a safe plain-file copy) and
/// copying the one file is the whole backup.
#[tauri::command]
fn backup_library(
    state: tauri::State<CatalogState>,
    active_library_file: tauri::State<ActiveLibraryFileState>,
    backup_file_path: String,
) -> Result<backup::BackupPackage, String> {
    let library_file_path = active_library_file
        .0
        .lock()
        .expect("active library file mutex poisoned")
        .clone()
        .ok_or_else(|| "no library file is currently open to back up".to_string())?;

    if std::path::Path::new(&backup_file_path).exists() {
        return Err("a file already exists at that location".to_string());
    }
    if let Some(parent) = std::path::Path::new(&backup_file_path).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create backup folder: {error}"))?;
    }

    let (library_id, media_root) = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        catalog.checkpoint_wal().map_err(storage_error_message)?;
        let library = catalog
            .list_libraries()
            .map_err(storage_error_message)?
            .into_iter()
            .next()
            .ok_or_else(|| "no library found in the open catalog".to_string())?;
        (library.id, library.media_root)
    };

    let source = backup::BackupSource {
        catalog_path: library_file_path,
        backup_file_path,
    };

    backup::create_backup(library_id, media_root, &source, current_time_ms(), |from, to| {
        std::fs::copy(from, to).is_ok()
    })
    .map_err(|error| format!("{error:?}"))
}

/// Restores the currently open library file from a `.darkwavebak` snapshot.
/// The live SQLite connection is closed (swapped out for an in-memory
/// placeholder under the same mutex the rest of the app uses) before the
/// file on disk is touched, and the snapshot is staged next to the live
/// project file and only `rename`d into place once fully copied, so a
/// failed or partial copy never corrupts the live file — the same pattern
/// this command used for the old shared `catalog.sqlite`, just scoped to
/// whichever project file is open now.
#[tauri::command]
fn restore_library(
    state: tauri::State<CatalogState>,
    active_library_file: tauri::State<ActiveLibraryFileState>,
    backup_file_path: String,
) -> Result<LibraryRecord, String> {
    if !std::path::Path::new(&backup_file_path).exists() {
        return Err("that backup file no longer exists".to_string());
    }

    let live_library_file_path = active_library_file
        .0
        .lock()
        .expect("active library file mutex poisoned")
        .clone()
        .ok_or_else(|| "no library file is currently open to restore into".to_string())?;

    let staged_path = format!("{live_library_file_path}.restoring");
    std::fs::copy(&backup_file_path, &staged_path)
        .map_err(|error| format!("stage backup snapshot: {error}"))?;

    let mut guard = state.0.lock().expect("catalog mutex poisoned");
    let _ = std::fs::copy(
        &live_library_file_path,
        format!("{live_library_file_path}.before-restore"),
    );
    drop(std::mem::replace(
        &mut *guard,
        Catalog::open(":memory:").map_err(storage_error_message)?,
    ));

    let swap_result = std::fs::rename(&staged_path, &live_library_file_path)
        .map_err(|error| format!("replace live project file: {error}"));

    let reopened = Catalog::open(&live_library_file_path).map_err(storage_error_message);

    match (swap_result, reopened) {
        (Ok(()), Ok(catalog)) => {
            let library = catalog
                .list_libraries()
                .map_err(storage_error_message)?
                .into_iter()
                .next()
                .ok_or_else(|| "no library found in the restored file".to_string())?;
            *guard = catalog;
            Ok(library)
        }
        (Err(error), Ok(catalog)) => {
            *guard = catalog;
            Err(error)
        }
        (_, Err(open_error)) => {
            let _ = std::fs::remove_file(&staged_path);
            *guard = Catalog::open(":memory:").map_err(storage_error_message)?;
            Err(format!("project file unreadable after restore attempt: {open_error}"))
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

    let expired_licenses = catalog
        .assets_with_expired_license(library_id, &license_documents::today_iso())
        .map_err(storage_error_message)?;
    for (asset_id, expired_on) in expired_licenses {
        findings.push(maintenance::MaintenanceFinding::license_expired(
            asset_id,
            &expired_on,
        ));
    }

    // One finding per pending job, but capped: a whole-library re-sync (ADR
    // 0032) or a fresh bulk import can leave thousands pending for a few
    // minutes, and the maintenance panel only shows a count — thousands of
    // identical finding structs would be a pointless payload.
    let pending_waveforms = catalog
        .pending_job_count(JobKind::WaveformGeneration)
        .map_err(storage_error_message)?
        .min(100);
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
    app: tauri::AppHandle,
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

    // The asset's disposable local preview-cache copy (see cached_file_path)
    // is never coming back into use once this row is gone — this was
    // previously left behind on every single-item permanent delete, an
    // orphan with no future cleanup path short of "Clear Cache" nuking
    // every asset's cache, not just this one.
    if let Ok(cache_dir) = preview_cache_dir(&app) {
        let _ = std::fs::remove_file(cached_file_path(&cache_dir, &asset));
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
    vec!["catalog_snapshot", "media_root"]
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

/// Keeps a library's `kind = "media_root"` folder row (see `add_folder`) in
/// sync whenever `media_root` itself changes — the New Setup wizard's basic
/// step, the Settings "change media root" action, `import_folder`'s
/// implicit first-import-sets-root path, and the MAS bookmark self-heal
/// after an SMB/NAS remount (see `resolve_library_bookmark_access`) all
/// change `media_root`, and the background poller only ever watches what's
/// registered in `folders` — without this, any of those paths would leave
/// the poller either watching a stale path or not watching the root at
/// all. A no-op for an empty `media_root` (nothing to watch yet) and for a
/// path that already matches the existing row. Best-effort: a failure here
/// shouldn't fail whatever caller is setting `media_root`.
fn sync_media_root_folder(catalog: &Catalog, library_id: Uuid, media_root: &str) {
    if media_root.trim().is_empty() {
        return;
    }
    let Ok(folders) = catalog.list_folders(library_id) else {
        return;
    };
    if let Some(existing) = folders.iter().find(|folder| folder.kind == "media_root") {
        if existing.path == media_root {
            return;
        }
        let _ = catalog.remove_folder(existing.id);
    }
    let _ = catalog.add_folder(library_id, media_root, None, "media_root");
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
                sync_media_root_folder(catalog, library.id, resolved_path.as_ref());
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
    sync_media_root_folder(&catalog, library_id, &media_root);
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

/// Registers a folder Darkwave should know about for this library — the
/// basic single-folder setup calls this once for `media_root` itself
/// (`role: null, kind: "media_root"`); the advanced setup calls it once per
/// additional folder the user adds, each with its own `role` (a media-type
/// hint used by import classification — see `import-pipeline`) and `kind`
/// (`"watched"` for an ordinary audio folder, `"documents"` for a
/// licence/PDF folder tracked as inventory only).
#[tauri::command]
fn add_folder(
    state: tauri::State<CatalogState>,
    library_id: String,
    path: String,
    role: Option<String>,
    kind: String,
) -> Result<FolderRecord, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .add_folder(library_id, path, role.as_deref(), kind)
        .map_err(storage_error_message)
}

#[tauri::command]
fn list_folders(state: tauri::State<CatalogState>, library_id: String) -> Result<Vec<FolderRecord>, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog.list_folders(library_id).map_err(storage_error_message)
}

#[tauri::command]
fn remove_folder(state: tauri::State<CatalogState>, folder_id: String) -> Result<(), String> {
    let folder_id = parse_uuid_field(&folder_id, "folder id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog.remove_folder(folder_id).map_err(storage_error_message)
}

#[tauri::command]
fn set_folder_role(
    state: tauri::State<CatalogState>,
    folder_id: String,
    role: Option<String>,
) -> Result<(), String> {
    let folder_id = parse_uuid_field(&folder_id, "folder id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .set_folder_role(folder_id, role.as_deref())
        .map_err(storage_error_message)
}

/// Looks up the configured role for `path` if it exactly matches one of the
/// library's `folders` rows — used by `import_folder`/`refresh_library`/
/// `scan_import_folder` so a manual bulk import of an already-configured
/// folder gets the same classification fallback the background poller
/// would have given the same files (see `resolve_media_type`'s
/// fill-the-gap precedence in `import-pipeline`). `None` for any path that
/// isn't a folder the user has explicitly registered — most manual imports,
/// since the poller already keeps registered folders imported continuously.
fn folder_role_for_path(catalog: &Catalog, library_id: Uuid, path: &str) -> Option<String> {
    catalog
        .list_folders(library_id)
        .ok()?
        .into_iter()
        .find(|folder| folder.path == path)
        .and_then(|folder| folder.role)
}

/// The library-file path the currently open catalog came from, or `None`
/// when running against the legacy shared catalog / no library file yet —
/// the frontend uses this to decide whether to show the first-run screen.
/// Named distinctly from Darkwave's unrelated "Project" concept
/// (`CollectionType::Project`, an editorial export destination) to avoid
/// confusion with it.
#[tauri::command]
fn active_library_file_path(state: tauri::State<ActiveLibraryFileState>) -> Option<String> {
    state
        .0
        .lock()
        .expect("active library file mutex poisoned")
        .clone()
}

#[tauri::command]
fn list_recent_library_files(
    app: tauri::AppHandle,
) -> Result<Vec<preferences::RecentLibraryFileEntry>, String> {
    let path = preferences_path(&app)?;
    let preferences = preferences::load_preferences(&path).map_err(|error| format!("{error:?}"))?;
    Ok(preferences.recent_library_files)
}

/// Whether `path` is safe to treat as a normal local catalog location
/// (see `storage::is_network_tolerant_catalog_path`) — the frontend calls
/// this after a New Setup save dialog or an Open Library pick to decide
/// whether to show the "this is on a network drive" warning.
#[tauri::command]
fn is_path_network_tolerant(path: String) -> Result<bool, String> {
    storage::is_network_tolerant_catalog_path(&path).map_err(storage_error_message)
}

/// Creates a brand-new `.darkwave` library file at `library_file_path` (New
/// Setup) and makes it the active one, replacing whatever catalog was
/// previously open (the fresh in-memory placeholder from a cold start, or
/// another library file the user is switching away from).
#[tauri::command]
fn create_library_file(
    app: tauri::AppHandle,
    state: tauri::State<CatalogState>,
    active_library_file: tauri::State<ActiveLibraryFileState>,
    library_file_path: String,
    name: String,
) -> Result<LibraryRecord, String> {
    if std::path::Path::new(&library_file_path).exists() {
        return Err("a file already exists at that location".to_string());
    }
    if let Some(parent) = std::path::Path::new(&library_file_path).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create library file folder: {error}"))?;
    }

    let (catalog, library) = Catalog::open_or_init_library_file(&library_file_path, &name)
        .map_err(storage_error_message)?;
    catalog
        .seed_starter_taxonomy()
        .map_err(storage_error_message)?;

    swap_active_catalog(
        &app,
        &state,
        &active_library_file,
        catalog,
        &library_file_path,
        &library.name,
    )?;

    Ok(library)
}

/// Opens an existing `.darkwave` library file (Open Library, a
/// recent-libraries click, or a double-clicked file), replacing whatever
/// catalog was previously open.
#[tauri::command]
fn open_library_file(
    app: tauri::AppHandle,
    state: tauri::State<CatalogState>,
    active_library_file: tauri::State<ActiveLibraryFileState>,
    library_file_path: String,
) -> Result<LibraryRecord, String> {
    if !std::path::Path::new(&library_file_path).exists() {
        return Err("that library file no longer exists".to_string());
    }

    let (catalog, library) = Catalog::open_or_init_library_file(&library_file_path, "")
        .map_err(storage_error_message)?;

    swap_active_catalog(
        &app,
        &state,
        &active_library_file,
        catalog,
        &library_file_path,
        &library.name,
    )?;

    Ok(library)
}

/// Rebuilds the File menu's "Open Recent" submenu in place from the current
/// recent-library-files list — clears every existing item, appends a fresh
/// `MenuItem` per entry (newest first, matching the list's own order), and
/// disables the submenu entirely when there's nothing to show rather than
/// leaving stale entries or an empty-but-enabled menu behind. Best-effort:
/// a missing `OpenRecentMenuState` (shouldn't happen once `.setup()` has
/// run) or a failed menu mutation is silently skipped rather than failing
/// whatever caller triggered the refresh — an out-of-date recents submenu
/// is a cosmetic problem, not a reason to fail a library open/create.
fn refresh_open_recent_menu(app: &tauri::AppHandle, recents: &[preferences::RecentLibraryFileEntry]) {
    let Some(state) = app.try_state::<OpenRecentMenuState>() else {
        return;
    };
    let submenu = &state.0;

    if let Ok(items) = submenu.items() {
        for item in items {
            let _ = submenu.remove(&item);
        }
    }

    let _ = submenu.set_enabled(!recents.is_empty());
    for entry in recents {
        if let Ok(item) = tauri::menu::MenuItem::with_id(
            app,
            format!("open-recent:{}", entry.path),
            &entry.name,
            true,
            None::<&str>,
        ) {
            let _ = submenu.append(&item);
        }
    }
}

/// Shared tail end of `create_library_file`/`open_library_file`: swaps the
/// new catalog into the same mutex `restore_library` already uses for a
/// live catalog replacement, records the active library-file path, and
/// updates the recent-libraries list so the first-run screen reflects it
/// next launch.
fn swap_active_catalog(
    app: &tauri::AppHandle,
    state: &tauri::State<CatalogState>,
    active_library_file: &tauri::State<ActiveLibraryFileState>,
    catalog: Catalog,
    library_file_path: &str,
    library_name: &str,
) -> Result<(), String> {
    // Persist first, swap the live catalog only once that succeeds — the
    // other order left a window where a failed preferences write returned
    // Err to the frontend (which then leaves its own activeLibraryId/
    // activeLibraryFilePath state untouched) while the backend had already
    // swapped to the new catalog, desyncing frontend and backend state with
    // no way to roll back.
    let preferences_file = preferences_path(app)?;
    let mut prefs = preferences::load_preferences(&preferences_file)
        .map_err(|error| format!("{error:?}"))?;
    prefs.record_library_file_opened(library_file_path, library_name, current_time_ms());
    preferences::save_preferences(&preferences_file, &prefs)
        .map_err(|error| format!("{error:?}"))?;
    refresh_open_recent_menu(app, &prefs.recent_library_files);

    {
        let mut guard = state.0.lock().expect("catalog mutex poisoned");
        drop(std::mem::replace(&mut *guard, catalog));
    }
    *active_library_file
        .0
        .lock()
        .expect("active library file mutex poisoned") = Some(library_file_path.to_string());

    Ok(())
}

/// Picks the first `.darkwave` path out of a process's CLI args (skipping
/// argv[0], the executable path itself) — how a library file's path
/// arrives on Windows, whether from the initial launch or a second launch
/// forwarded here by `tauri_plugin_single_instance`. Deliberately does not
/// match `.darkwavebak` — a backup file isn't a live catalog to open
/// directly; double-clicking one only brings Darkwave to the front for now
/// (see the backup-format phase for an eventual restore flow).
fn darkwave_library_path_from_args(args: &[String]) -> Option<String> {
    args.iter()
        .skip(1)
        .find(|arg| arg.to_ascii_lowercase().ends_with(".darkwave"))
        .cloned()
}

/// Opens `library_file_path` on an already-running app instance (the
/// second-launch-forwarded-here path on Windows, or a `RunEvent::Opened`
/// file on macOS) and tells the already-rendered frontend about it via an
/// event, since — unlike `open_library_file` — nothing in the UI called
/// `invoke` to trigger this and get the result back directly.
fn open_library_file_and_notify_frontend(app: &tauri::AppHandle, library_file_path: &str) {
    if !std::path::Path::new(library_file_path).is_file() {
        return;
    }
    let (Some(state), Some(active_library_file)) = (
        app.try_state::<CatalogState>(),
        app.try_state::<ActiveLibraryFileState>(),
    ) else {
        return;
    };

    // A repeat double-click of the file already open, or a second launch
    // pointed at it (both land here — see the single-instance callback and
    // RunEvent::Opened), would otherwise pay for closing and reopening the
    // live connection, a preferences.json rewrite, and a frontend event
    // that resets onboarding/UI state, all for no actual change.
    let already_active = active_library_file
        .0
        .lock()
        .expect("active library file mutex poisoned")
        .as_deref()
        == Some(library_file_path);
    if already_active {
        return;
    }

    let opened = Catalog::open_or_init_library_file(library_file_path, "")
        .map_err(storage_error_message)
        .and_then(|(catalog, library)| {
            swap_active_catalog(
                app,
                &state,
                &active_library_file,
                catalog,
                library_file_path,
                &library.name,
            )?;
            Ok(library)
        });

    use tauri::Emitter;
    match opened {
        Ok(library) => {
            let _ = app.emit(
                "library-file-opened",
                serde_json::json!({ "library": library, "path": library_file_path }),
            );
        }
        // Previously silently dropped: a double-click or second-launch open
        // that failed (moved/deleted file, corrupt catalog, permissions)
        // left the app exactly as it was with no indication anything was
        // even attempted. Surface it so the frontend can show an error.
        Err(error) => {
            let _ = app.emit(
                "library-file-open-failed",
                serde_json::json!({ "path": library_file_path, "error": error }),
            );
        }
    }
}

fn legacy_catalog_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .app_data_dir()
        .map(|dir| dir.join("catalog.sqlite"))
        .map_err(|error| format!("resolve app data directory: {error}"))
}

/// Migrates one library out of the legacy shared `catalog.sqlite` into its
/// own standalone `.darkwave` file at `library_file_path`, which must not
/// already exist. Does not touch the live legacy catalog beyond checkpointing
/// its WAL (see `Catalog::checkpoint_wal`) — the whole legacy file is
/// cloned via a plain filesystem copy, then every *other* library's data is
/// stripped out of that copy via the existing cascade-deleting
/// `delete_library`, which already removes a library's assets, tags,
/// collections and everything else that hangs off them (see its own doc
/// comment) without touching sibling libraries. Cloning the whole file
/// first — rather than re-inserting rows table by table into a fresh file
/// — means nothing can be silently missed by an incomplete list of tables
/// to copy.
///
/// Deliberately does not touch `ActiveLibraryFileState`/the running
/// `CatalogState`, and does not mark `library_file_path` as the active
/// project to reopen next launch — it only becomes a recent-libraries
/// entry (see `AppPreferences::add_recent_library_file`), so the user picks
/// when to actually open it rather than having the app silently switch
/// underneath them mid-migration.
#[tauri::command]
fn migrate_legacy_library(
    app: tauri::AppHandle,
    state: tauri::State<CatalogState>,
    library_id: String,
    library_file_path: String,
) -> Result<LibraryRecord, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;

    if std::path::Path::new(&library_file_path).exists() {
        return Err("a file already exists at that location".to_string());
    }
    if let Some(parent) = std::path::Path::new(&library_file_path).parent() {
        std::fs::create_dir_all(parent)
            .map_err(|error| format!("create library file folder: {error}"))?;
    }

    {
        let legacy_catalog = state.0.lock().expect("catalog mutex poisoned");
        legacy_catalog
            .checkpoint_wal()
            .map_err(storage_error_message)?;
    }

    let source_path = legacy_catalog_path(&app)?;
    std::fs::copy(&source_path, &library_file_path)
        .map_err(|error| format!("copy legacy catalog: {error}"))?;

    let migrated = Catalog::open(&library_file_path).map_err(storage_error_message)?;
    let other_library_ids: Vec<Uuid> = migrated
        .list_libraries()
        .map_err(storage_error_message)?
        .into_iter()
        .map(|library| library.id)
        .filter(|id| *id != library_id)
        .collect();
    for other_id in other_library_ids {
        migrated
            .delete_library(other_id)
            .map_err(storage_error_message)?;
    }

    let library = migrated
        .get_library(library_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "library not found in migrated file".to_string())?;

    // The background poller now watches only what's registered in
    // `folders` — a legacy library predates that table entirely, so
    // without this it would silently stop being watched at all once
    // migrated, even though its media_root was being watched before
    // (implicitly, via the old media_root-wide refresh/import behavior).
    // Same sync every other media_root-changing path uses.
    sync_media_root_folder(&migrated, library_id, &library.media_root);

    drop(migrated);

    let preferences_file = preferences_path(&app)?;
    let mut prefs = preferences::load_preferences(&preferences_file)
        .map_err(|error| format!("{error:?}"))?;
    prefs.add_recent_library_file(&library_file_path, &library.name, current_time_ms());
    // Durable record that this legacy library id is done — without this the
    // migration screen's status is only ever session-local React state, so
    // relaunching mid-migration made an already-migrated library look
    // pending again and re-offered "Choose location…" for it.
    prefs.record_legacy_library_migrated(library_id.to_string(), &library_file_path, &library.name);
    preferences::save_preferences(&preferences_file, &prefs)
        .map_err(|error| format!("{error:?}"))?;
    refresh_open_recent_menu(&app, &prefs.recent_library_files);

    Ok(library)
}

/// Called once every legacy library has been migrated (see
/// `migrate_legacy_library`): closes the live connection to the legacy
/// shared catalog, replaces it with an empty placeholder (the same state a
/// fresh install starts in), and renames the legacy file out of the way —
/// never deletes it outright — so it stops being treated as a live catalog
/// on future launches (`active_library_file_path` returning `None` with
/// zero libraries no longer looks like "migration needed") while staying
/// recoverable if a migration bug is later discovered.
#[tauri::command]
fn finish_legacy_migration(
    app: tauri::AppHandle,
    state: tauri::State<CatalogState>,
    active_library_file: tauri::State<ActiveLibraryFileState>,
) -> Result<(), String> {
    {
        let mut guard = state.0.lock().expect("catalog mutex poisoned");
        let placeholder = Catalog::open(":memory:").map_err(storage_error_message)?;
        drop(std::mem::replace(&mut *guard, placeholder));
    }
    *active_library_file
        .0
        .lock()
        .expect("active library file mutex poisoned") = None;

    let legacy_path = legacy_catalog_path(&app)?;
    if legacy_path.exists() {
        let mut retired_path = legacy_path.with_file_name("catalog.sqlite.pre-migration-backup");
        let mut attempt = 1;
        while retired_path.exists() {
            retired_path =
                legacy_path.with_file_name(format!("catalog.sqlite.pre-migration-backup.{attempt}"));
            attempt += 1;
        }
        // Best-effort: a failed rename here (e.g. a stray file lock) still
        // leaves the legacy file readable and inert for us, just not
        // renamed — worth surfacing, not worth failing the whole flow over,
        // since every library has already been safely copied out by this
        // point.
        if let Err(error) = std::fs::rename(&legacy_path, &retired_path) {
            log_startup_event(&format!(
                "could not retire legacy catalog {}: {error}",
                legacy_path.display()
            ));
        }
    }

    Ok(())
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

/// Runs SQLite's own consistency check against the currently open catalog
/// file (see `storage::Catalog::integrity_check`) — the "Check & Repair"
/// action's check half. `library_id` only confirms the library is one this
/// catalog actually knows about; the check itself runs against the whole
/// file, same as `optimize_library` below, since a `.darkwave` file holds
/// exactly one library in normal use. An empty result means healthy;
/// anything else is a case for restoring from a `.darkwavebak` backup, not
/// something this command can fix on its own.
// (async): PRAGMA integrity_check walks every page of the database file,
// real I/O that can take a while on a large catalog — same reasoning as
// delete_library below for not running this on Tauri's main thread.
#[tauri::command(async)]
fn check_library_integrity(state: tauri::State<CatalogState>, library_id: String) -> Result<Vec<String>, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .get_library(library_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "library not found".to_string())?;
    catalog.integrity_check().map_err(storage_error_message)
}

/// Reclaims space and defragments the currently open catalog file (see
/// `storage::Catalog::optimize`) — the equivalent of Lightroom's "Optimize
/// Catalog". Same one-library-per-file reasoning as `check_library_integrity`
/// for why `library_id` only gates access rather than scoping the operation.
// (async): VACUUM rewrites the entire database file — real, potentially
// slow I/O, same reasoning as delete_library below for not running this on
// Tauri's main thread.
#[tauri::command(async)]
fn optimize_library(state: tauri::State<CatalogState>, library_id: String) -> Result<(), String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .get_library(library_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "library not found".to_string())?;
    catalog.optimize().map_err(storage_error_message)
}

/// Clears this library's "don't re-import this" tombstones (see
/// `storage::Catalog::record_deleted_fingerprint`) — the escape hatch for
/// anyone who deliberately wants a previously-deleted file to come back
/// the next time its folder is scanned, since that block is otherwise
/// permanent and invisible. Returns how many were cleared.
#[tauri::command]
fn clear_deleted_import_history(state: tauri::State<CatalogState>, library_id: String) -> Result<usize, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .get_library(library_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "library not found".to_string())?;
    catalog
        .clear_deleted_fingerprints(library_id)
        .map_err(storage_error_message)
}

/// Permanently removes every currently-trashed asset in this library from
/// the catalog, bypassing the normal retention wait. Same guarantee as the
/// rest of the trash system for the *real* file: only ever deletes catalog
/// rows, never a real file (see `storage::Catalog::empty_trash_for_library`).
/// The app's own disposable local preview-cache copy (see
/// `cached_file_path`) is a different story — nothing else was ever going
/// to clean those up once these rows are gone, so this does that too.
#[tauri::command]
fn empty_library_trash(
    app: tauri::AppHandle,
    state: tauri::State<CatalogState>,
    library_id: String,
) -> Result<usize, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");

    let trashed = catalog.list_trash_items(library_id).map_err(storage_error_message)?;
    let removed = catalog
        .empty_trash_for_library(library_id)
        .map_err(storage_error_message)?;

    if let Ok(cache_dir) = preview_cache_dir(&app) {
        for item in trashed {
            let _ = std::fs::remove_file(cached_file_path_for(&cache_dir, item.asset_id, &item.original_path));
        }
    }

    Ok(removed)
}

#[derive(serde::Serialize)]
struct OrganizeMember {
    asset_id: String,
    filename: String,
    display_name: String,
    /// The folder the file sits in now — shown in the review so the user
    /// can see where its category folder will be created.
    folder: String,
    reason: search::sfx_categories::CategoryReason,
    matched: String,
    series: Option<String>,
    /// `Some(current media type)` when the check found a sound effect filed
    /// as something else (music, ambience…); applying re-files it.
    reclassify_from: Option<String>,
}

#[derive(serde::Serialize)]
struct OrganizeCategory {
    name: String,
    existing: bool,
    members: Vec<OrganizeMember>,
}

#[derive(serde::Serialize)]
struct OrganizeProposal {
    categories: Vec<OrganizeCategory>,
    uncategorized: Vec<OrganizeMember>,
    /// Sound effects that already had a category (listed under it, not re-guessed).
    already_categorized: usize,
    /// Sound effects found filed under another media type.
    misfiled: usize,
    /// Every track the check looked at.
    total_considered: usize,
}

/// Media types the library check considers. Voiceover and Foley are
/// deliberate, separate categories in the export taxonomy (02_VO,
/// 04_FOLEY), so they're never second-guessed into sound effects.
const SFX_CHECK_MEDIA_TYPES: &[&str] = &["sound_effect", "music", "ambience", "other", "needs_review"];

/// Folder names around a file, nearest first, for the engine's folder
/// signal (and the SFX/music folder markers). Stops at the volume root.
fn enclosing_folders(path: &std::path::Path) -> Vec<String> {
    path.parent()
        .map(|parent| {
            parent
                .components()
                .filter_map(|component| match component {
                    std::path::Component::Normal(name) => Some(name.to_string_lossy().to_string()),
                    _ => None,
                })
                .collect::<Vec<_>>()
        })
        .map(|mut folders| {
            folders.reverse();
            folders.truncate(8);
            folders
        })
        .unwrap_or_default()
}

/// The "Organize" button, step one: a read-only check of the whole
/// library. Every imported track (except voiceover/foley, see
/// SFX_CHECK_MEDIA_TYPES) goes through `search::sfx_categories`, which
/// decides whether it's a sound effect — including ones filed as music or
/// ambience — and which category it belongs in, from its name, UCS code,
/// folders, numbering and duration. The user reviews, edits and confirms
/// before `apply_sfx_organization` writes anything.
///
/// Sound effects that already have a category aren't re-guessed unless
/// `include_categorized` (that would override a choice the user made), but
/// they're listed under their category so "move files" can file them.
/// `media_type` is accepted for compatibility and must be `sound_effect`.
#[tauri::command(async)]
fn propose_sfx_organization(
    state: tauri::State<CatalogState>,
    library_id: String,
    media_type: String,
    include_categorized: bool,
) -> Result<OrganizeProposal, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    if media_type != "sound_effect" {
        return Err(format!("{media_type} can't be organized into categories"));
    }
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let assets: Vec<AssetRecord> = catalog
        .list_assets(library_id)
        .map_err(storage_error_message)?
        .into_iter()
        .filter(|asset| SFX_CHECK_MEDIA_TYPES.contains(&asset.media_type.as_str()))
        .filter(|asset| import_pipeline::junk_file_kind(&asset.original_filename).is_none())
        .collect();
    let current = catalog
        .asset_categories_for_library(library_id)
        .map_err(storage_error_message)?;
    // Only categories that actually hold tracks: an emptied (renamed-away)
    // category must not attract new files back under its old name, and
    // the seeded Impact/Whoosh/Rise tags are not categories until used.
    let live_categories: Vec<String> = catalog
        .category_counts_for_library(library_id)
        .map_err(storage_error_message)?
        .into_iter()
        .map(|(tag, _)| tag.name)
        .collect();
    let all_category_names: Vec<String> = catalog
        .list_tags()
        .map_err(storage_error_message)?
        .into_iter()
        .filter(|tag| tag.facet.as_deref() == Some(storage::CATEGORY_FACET))
        .map(|tag| tag.name)
        .collect();

    let total_considered = assets.len();
    let paths: HashMap<Uuid, String> = assets
        .iter()
        .filter_map(|asset| resolve_asset_path(&catalog, asset).ok().map(|path| (asset.id, path)))
        .collect();
    let by_id: HashMap<String, &AssetRecord> = assets.iter().map(|asset| (asset.id.to_string(), asset)).collect();
    let (skipped, candidates): (Vec<&AssetRecord>, Vec<&AssetRecord>) = assets
        .iter()
        .partition(|asset| !include_categorized && current.contains_key(&asset.id));
    let inputs: Vec<search::sfx_categories::OrganizeInput> = candidates
        .iter()
        .map(|asset| search::sfx_categories::OrganizeInput {
            id: asset.id.to_string(),
            filename: asset.original_filename.clone(),
            folders: paths
                .get(&asset.id)
                .map(|path| enclosing_folders(std::path::Path::new(path)))
                .unwrap_or_default(),
            media_type: asset.media_type.clone(),
            duration_ms: asset.duration_ms,
        })
        .collect();
    let plan = search::sfx_categories::propose_sfx_categories(&inputs, &live_categories);

    let member = |id: &str, reason, matched: String, series: Option<String>, reclassify_from: Option<String>| {
        let asset = by_id[id];
        let folder = paths
            .get(&asset.id)
            .map(|path| category_folder_base(std::path::Path::new(path), &all_category_names).display().to_string())
            .unwrap_or_default();
        OrganizeMember {
            asset_id: id.to_string(),
            filename: asset.original_filename.clone(),
            display_name: asset.display_name.clone(),
            folder,
            reason,
            matched,
            series,
            reclassify_from,
        }
    };
    let mut misfiled = 0;
    let mut categories: Vec<OrganizeCategory> = plan
        .categories
        .into_iter()
        .map(|category| OrganizeCategory {
            name: category.name,
            existing: category.existing,
            members: category
                .members
                .into_iter()
                .map(|m| {
                    if m.reclassify_from.is_some() {
                        misfiled += 1;
                    }
                    member(&m.id, m.reason, m.matched, m.series, m.reclassify_from)
                })
                .collect(),
        })
        .collect();

    // Already-categorized sound effects: listed under their category with
    // an empty `matched` ("already in this category"), never re-guessed.
    for asset in &skipped {
        let Some((_, name)) = current.get(&asset.id) else { continue };
        let entry = member(
            &asset.id.to_string(),
            search::sfx_categories::CategoryReason::Existing,
            String::new(),
            None,
            None,
        );
        match categories.iter_mut().find(|category| category.name.eq_ignore_ascii_case(name)) {
            Some(category) => {
                category.existing = true;
                category.members.push(entry);
            }
            None => categories.push(OrganizeCategory { name: name.clone(), existing: true, members: vec![entry] }),
        }
    }
    categories.sort_by(|left, right| right.members.len().cmp(&left.members.len()).then(left.name.cmp(&right.name)));

    Ok(OrganizeProposal {
        categories,
        uncategorized: plan
            .uncategorized
            .iter()
            .map(|id| member(id, search::sfx_categories::CategoryReason::Keyword, String::new(), None, None))
            .collect(),
        already_categorized: skipped.len(),
        misfiled,
        total_considered,
    })
}

#[derive(serde::Deserialize)]
struct CategoryAssignment {
    asset_id: String,
    /// `None` / empty clears the asset's category.
    category: Option<String>,
    /// Re-file the track under this media type too (the library check's
    /// "this is really a sound effect" finds). `None` leaves it alone.
    #[serde(default)]
    media_type: Option<String>,
}

#[derive(serde::Serialize)]
struct ApplyOrganizationResult {
    undo_id: String,
    assigned: usize,
    /// Files actually moved into a category folder on disk.
    moved: usize,
    /// Category folders that didn't exist before and were created.
    folders_created: usize,
    /// Files that kept their category but couldn't be moved, with why.
    move_failures: Vec<String>,
}

/// One media-type re-filing Organize did, as recorded for undo/redo.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct MediaTypeChange {
    asset: Uuid,
    from: String,
    to: String,
}

/// One file Organize moved, as recorded for undo/redo.
#[derive(serde::Serialize, serde::Deserialize, Clone)]
struct FileMove {
    asset: Uuid,
    from: PathBuf,
    to: PathBuf,
}

/// Undo payload for an Organize run that moved files: the catalog-side
/// category change is its own (`set_asset_categories`) undo entry, nested
/// here so one Undo reverts both together.
#[derive(serde::Serialize, serde::Deserialize)]
struct OrganizeMovesUndo {
    category_undo: Uuid,
    moves: Vec<FileMove>,
    created_dirs: Vec<PathBuf>,
    #[serde(default)]
    media_types: Vec<MediaTypeChange>,
}

const ORGANIZE_MOVES_UNDO_KIND: &str = "organize_file_moves";

/// The folder a file's category folder is created in: the folder the file
/// is in now — unless that folder is itself already a category folder
/// (a re-sort, or a move from the inspector), in which case its parent, so
/// `_SoundFX/Doors/x.wav` moving to Impacts becomes `_SoundFX/Impacts/x.wav`
/// rather than `_SoundFX/Doors/Impacts/x.wav`.
fn category_folder_base(file: &std::path::Path, category_names: &[String]) -> PathBuf {
    let parent = file.parent().map(PathBuf::from).unwrap_or_default();
    let parent_name = parent.file_name().and_then(|name| name.to_str()).unwrap_or_default();
    let is_category_folder = category_names.iter().any(|name| {
        export_pipeline::sanitize_filename(name).eq_ignore_ascii_case(parent_name)
    });
    if is_category_folder {
        parent.parent().map(PathBuf::from).unwrap_or(parent)
    } else {
        parent
    }
}

/// `dir/filename`, or `dir/stem (2).ext`, `(3)`… — never an existing path,
/// so a move can never overwrite another file.
fn unique_destination(dir: &std::path::Path, filename: &str) -> PathBuf {
    let candidate = dir.join(filename);
    if !candidate.exists() {
        return candidate;
    }
    let path = std::path::Path::new(filename);
    let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or(filename);
    let extension = path.extension().and_then(|extension| extension.to_str());
    for counter in 2.. {
        let name = match extension {
            Some(extension) => format!("{stem} ({counter}).{extension}"),
            None => format!("{stem} ({counter})"),
        };
        let candidate = dir.join(name);
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!()
}

/// Moves one file without ever overwriting: `rename` when it can (same
/// volume, the normal case for a folder inside the file's own folder),
/// otherwise copy → verify size → remove the original. The macOS
/// AppleDouble sidecar (`._name`), if present on a NAS/exFAT share, moves
/// along with it so the file's Finder metadata isn't orphaned as junk.
fn move_file_safely(from: &std::path::Path, to: &std::path::Path) -> std::io::Result<()> {
    if to.exists() {
        return Err(std::io::Error::new(std::io::ErrorKind::AlreadyExists, "destination already exists"));
    }
    if std::fs::rename(from, to).is_err() {
        let expected = std::fs::metadata(from)?.len();
        std::fs::copy(from, to)?;
        let copied = std::fs::metadata(to)?.len();
        if copied != expected {
            let _ = std::fs::remove_file(to);
            return Err(std::io::Error::other("copy was incomplete; original left in place"));
        }
        std::fs::remove_file(from)?;
    }
    if let (Some(from_name), Some(to_name)) = (from.file_name(), to.file_name()) {
        let sidecar = from.with_file_name(format!("._{}", from_name.to_string_lossy()));
        if sidecar.exists() {
            let _ = std::fs::rename(&sidecar, to.with_file_name(format!("._{}", to_name.to_string_lossy())));
        }
    }
    Ok(())
}

/// After a move, points the catalog row at the file's new location,
/// keeping its storage mode (see `Catalog::set_asset_location`).
fn record_moved_location(catalog: &Catalog, asset: &AssetRecord, to: &std::path::Path) -> Result<(), String> {
    let path = match &asset.path {
        AssetPath::Managed(_) => {
            let library = catalog
                .get_library(asset.library_id)
                .map_err(storage_error_message)?
                .ok_or_else(|| "library not found".to_string())?;
            match to.strip_prefix(&library.media_root) {
                Ok(relative) => AssetPath::Managed(relative.to_string_lossy().to_string()),
                Err(_) => AssetPath::Referenced(to.to_string_lossy().to_string()),
            }
        }
        AssetPath::Referenced(_) => AssetPath::Referenced(to.to_string_lossy().to_string()),
    };
    catalog.set_asset_location(asset.id, &path).map_err(storage_error_message)
}

/// Removes a folder Organize created if nothing real is left in it (Finder
/// may have dropped a `.DS_Store` in the meantime — that doesn't count).
fn remove_dir_if_effectively_empty(dir: &std::path::Path) {
    let Ok(entries) = std::fs::read_dir(dir) else { return };
    let mut junk = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if import_pipeline::junk_file_kind(&name).is_some() {
            junk.push(entry.path());
        } else {
            return;
        }
    }
    for path in junk {
        let _ = std::fs::remove_file(path);
    }
    let _ = std::fs::remove_dir(dir);
}

/// Step two of Organize (and the inspector's per-track Category field):
/// writes the reviewed assignments as one undoable batch — each asset ends
/// up in exactly one category (or none), see
/// `Catalog::set_asset_categories`.
///
/// With `move_files`, each categorized file is also moved on disk into a
/// folder named after its category, created next to it (see
/// `category_folder_base`). Never overwrites (`unique_destination`), skips
/// files that aren't reachable right now (they keep their category and are
/// reported), and records one undo entry covering both the categories and
/// every move, so a single Undo puts every file back where it was.
// (async): renames on a NAS share are network round-trips.
#[tauri::command(async)]
fn apply_sfx_organization(
    state: tauri::State<CatalogState>,
    assignments: Vec<CategoryAssignment>,
    move_files: Option<bool>,
) -> Result<ApplyOrganizationResult, String> {
    let mut refile = Vec::new();
    let parsed = assignments
        .into_iter()
        .map(|assignment| {
            let asset_id = parse_uuid_field(&assignment.asset_id, "asset id")?;
            if let Some(media_type) = assignment.media_type.filter(|media_type| media_type == "sound_effect") {
                refile.push((asset_id, media_type));
            }
            Ok((
                asset_id,
                assignment.category.map(|name| name.trim().to_string()).filter(|name| !name.is_empty()),
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    organize_assets_refiling(&catalog, &parsed, &refile, move_files.unwrap_or(false))
}

#[cfg(test)]
fn organize_assets(
    catalog: &Catalog,
    parsed: &[(Uuid, Option<String>)],
    move_files: bool,
) -> Result<ApplyOrganizationResult, String> {
    organize_assets_refiling(catalog, parsed, &[], move_files)
}

/// Category folders emptied by moves (a re-sort or a rename) are removed;
/// undo recreates any parent it needs when moving files back.
fn remove_emptied_category_folders(moves: &[FileMove], category_names: &[String]) {
    let mut seen = std::collections::HashSet::new();
    for file_move in moves {
        let Some(parent) = file_move.from.parent() else { continue };
        if !seen.insert(parent.to_path_buf()) {
            continue;
        }
        let name = parent.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        let is_category_folder = category_names
            .iter()
            .any(|category| export_pipeline::sanitize_filename(category).eq_ignore_ascii_case(name));
        if is_category_folder {
            remove_dir_if_effectively_empty(parent);
        }
    }
}

fn organize_assets_refiling(
    catalog: &Catalog,
    parsed: &[(Uuid, Option<String>)],
    refile: &[(Uuid, String)],
    move_files: bool,
) -> Result<ApplyOrganizationResult, String> {
    let mut media_types = Vec::new();
    for (asset_id, media_type) in refile {
        let Some(asset) = catalog.get_asset(*asset_id).map_err(storage_error_message)? else { continue };
        if asset.media_type != *media_type {
            catalog.set_media_type(*asset_id, media_type).map_err(storage_error_message)?;
            media_types.push(MediaTypeChange { asset: *asset_id, from: asset.media_type, to: media_type.clone() });
        }
    }
    let category_undo = catalog.set_asset_categories(parsed).map_err(storage_error_message)?;

    let mut result = ApplyOrganizationResult {
        undo_id: category_undo.to_string(),
        assigned: parsed.len(),
        moved: 0,
        folders_created: 0,
        move_failures: Vec::new(),
    };
    if !move_files {
        if !media_types.is_empty() {
            let payload = serde_json::to_string(&OrganizeMovesUndo {
                category_undo,
                moves: Vec::new(),
                created_dirs: Vec::new(),
                media_types,
            })
            .map_err(|error| error.to_string())?;
            result.undo_id = catalog
                .record_external_undo(ORGANIZE_MOVES_UNDO_KIND, &payload)
                .map_err(storage_error_message)?
                .to_string();
        }
        return Ok(result);
    }

    let category_names: Vec<String> = catalog
        .list_tags()
        .map_err(storage_error_message)?
        .into_iter()
        .filter(|tag| tag.facet.as_deref() == Some(storage::CATEGORY_FACET))
        .map(|tag| tag.name)
        .collect();
    let mut moves = Vec::new();
    let mut created_dirs: Vec<PathBuf> = Vec::new();

    for (asset_id, category) in parsed {
        let Some(category) = category else { continue };
        let Some(asset) = catalog.get_asset(*asset_id).map_err(storage_error_message)? else { continue };
        let Ok(current) = resolve_asset_path(catalog, &asset) else { continue };
        let from = PathBuf::from(&current);
        if !from.is_file() {
            result.move_failures.push(format!("{}: file not reachable right now", asset.original_filename));
            continue;
        }
        let folder = category_folder_base(&from, &category_names).join(export_pipeline::sanitize_filename(category));
        if from.parent() == Some(folder.as_path()) {
            continue; // already in its category folder
        }
        if !folder.exists() {
            if let Err(error) = std::fs::create_dir_all(&folder) {
                result.move_failures.push(format!("{}: couldn't create {}: {error}", asset.original_filename, folder.display()));
                continue;
            }
            created_dirs.push(folder.clone());
            result.folders_created += 1;
        }
        let file_name = from.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default();
        let to = unique_destination(&folder, &file_name);
        if let Err(error) = move_file_safely(&from, &to) {
            result.move_failures.push(format!("{}: {error}", asset.original_filename));
            continue;
        }
        if let Err(error) = record_moved_location(catalog, &asset, &to) {
            // Put it back rather than leave the catalog pointing at nothing.
            let _ = move_file_safely(&to, &from);
            result.move_failures.push(format!("{}: {error}", asset.original_filename));
            continue;
        }
        moves.push(FileMove { asset: asset.id, from, to });
    }

    result.moved = moves.len();
    remove_emptied_category_folders(&moves, &category_names);
    if !moves.is_empty() || !created_dirs.is_empty() || !media_types.is_empty() {
        let payload = serde_json::to_string(&OrganizeMovesUndo { category_undo, moves, created_dirs, media_types })
            .map_err(|error| error.to_string())?;
        let undo_id = catalog
            .record_external_undo(ORGANIZE_MOVES_UNDO_KIND, &payload)
            .map_err(storage_error_message)?;
        result.undo_id = undo_id.to_string();
    }
    Ok(result)
}

/// Reverses (`forward = false`) or replays (`forward = true`) the file
/// moves of one Organize run. A file is only moved if it's still exactly
/// where the run left it and its destination is free — anything the user
/// has since moved or replaced by hand is left alone, never overwritten.
fn replay_organize_moves(catalog: &Catalog, undo: &OrganizeMovesUndo, forward: bool) {
    if forward {
        for dir in &undo.created_dirs {
            let _ = std::fs::create_dir_all(dir);
        }
    }
    let ordered: Box<dyn Iterator<Item = &FileMove>> =
        if forward { Box::new(undo.moves.iter()) } else { Box::new(undo.moves.iter().rev()) };
    for file_move in ordered {
        let (source, destination) = if forward { (&file_move.from, &file_move.to) } else { (&file_move.to, &file_move.from) };
        if !source.is_file() || destination.exists() {
            continue;
        }
        if let Some(parent) = destination.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        if move_file_safely(source, destination).is_ok() {
            if let Ok(Some(asset)) = catalog.get_asset(file_move.asset) {
                if record_moved_location(catalog, &asset, destination).is_err() {
                    let _ = move_file_safely(destination, source);
                }
            }
        }
    }
    for change in &undo.media_types {
        let _ = catalog.set_media_type(change.asset, if forward { &change.to } else { &change.from });
    }
    if forward {
        let names: Vec<String> = catalog
            .list_tags()
            .map(|tags| {
                tags.into_iter()
                    .filter(|tag| tag.facet.as_deref() == Some(storage::CATEGORY_FACET))
                    .map(|tag| tag.name)
                    .collect()
            })
            .unwrap_or_default();
        remove_emptied_category_folders(&undo.moves, &names);
    } else {
        for dir in undo.created_dirs.iter().rev() {
            remove_dir_if_effectively_empty(dir);
        }
    }
}

/// Renames a category (the sidebar's double-click). Renaming onto another
/// existing category's name merges the two. Implemented as "move every
/// track of this category to the new name" through the same path as
/// Organize, so with `move_files` the files move into the new folder
/// (emptied old folders are removed), and one Undo reverts all of it.
#[tauri::command(async)]
fn rename_sfx_category(
    state: tauri::State<CatalogState>,
    library_id: String,
    tag_id: String,
    new_name: String,
    move_files: Option<bool>,
) -> Result<ApplyOrganizationResult, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let tag_id = parse_uuid_field(&tag_id, "tag id")?;
    let new_name = new_name.trim().to_string();
    if new_name.is_empty() {
        return Err("a category needs a name".into());
    }
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    rename_category(&catalog, library_id, tag_id, &new_name, move_files.unwrap_or(false))
}

fn rename_category(
    catalog: &Catalog,
    library_id: Uuid,
    tag_id: Uuid,
    new_name: &str,
    move_files: bool,
) -> Result<ApplyOrganizationResult, String> {
    let members = catalog.assets_in_category(library_id, tag_id).map_err(storage_error_message)?;
    // Same category key ("whooshes" → "Whooshes"): only the display name
    // changes. Not undoable on its own, and nothing on disk to move.
    if catalog.find_category_tag(new_name).map_err(storage_error_message)?.is_some_and(|tag| tag.id == tag_id) {
        catalog.set_category_display_name(tag_id, new_name).map_err(storage_error_message)?;
        return Ok(ApplyOrganizationResult {
            undo_id: String::new(),
            assigned: members.len(),
            moved: 0,
            folders_created: 0,
            move_failures: Vec::new(),
        });
    }
    let parsed: Vec<(Uuid, Option<String>)> = members.into_iter().map(|asset| (asset, Some(new_name.to_string()))).collect();
    organize_assets_refiling(catalog, &parsed, &[], move_files)
}

#[derive(serde::Serialize)]
struct SfxCategorySummary {
    tag_id: String,
    name: String,
    count: usize,
}

/// The sidebar's Sound Effects › By category list (and the inspector's
/// Category picker): every category with at least one track in this library.
#[tauri::command]
fn list_sfx_categories(state: tauri::State<CatalogState>, library_id: String) -> Result<Vec<SfxCategorySummary>, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    Ok(catalog
        .category_counts_for_library(library_id)
        .map_err(storage_error_message)?
        .into_iter()
        .map(|(tag, count)| SfxCategorySummary { tag_id: tag.id.to_string(), name: tag.name, count })
        .collect())
}

/// Each asset's current category in this library (`asset id → name`), for
/// showing it on browser rows and in the inspector.
#[tauri::command]
fn asset_categories(state: tauri::State<CatalogState>, library_id: String) -> Result<HashMap<String, String>, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    Ok(catalog
        .asset_categories_for_library(library_id)
        .map_err(storage_error_message)?
        .into_iter()
        .map(|(asset, (_, name))| (asset.to_string(), name))
        .collect())
}

/// One asset the library cleanup flagged as junk. `kind` is one of
/// `apple_double` / `system_file` / `empty` / `unreadable` — see
/// `junk_kind_for_asset` for exactly what each means and how cleanup
/// treats it.
#[derive(serde::Serialize)]
struct JunkAsset {
    asset_id: String,
    filename: String,
    path: String,
    file_size: u64,
    kind: &'static str,
    detail: Option<String>,
}

/// Classifies one catalogued asset for the cleanup. Order matters: an
/// AppleDouble sidecar also fails to decode, but it's reported as the more
/// specific (and safely file-deletable) `apple_double`, not `unreadable`.
///
/// `unreadable` is only ever claimed for a format this build can actually
/// decode (`supported_mvp_format`) — a cataloged-but-not-playable format
/// like AAC/M4A (see ADR 0028) fails decode by design and is real audio
/// the user wants to keep.
fn junk_kind_for_asset(asset: &AssetRecord, decode_error: Option<&String>) -> Option<&'static str> {
    match import_pipeline::junk_file_kind(&asset.original_filename) {
        Some(import_pipeline::JunkFileKind::AppleDouble) => return Some("apple_double"),
        Some(import_pipeline::JunkFileKind::SystemFile) => return Some("system_file"),
        None => {}
    }
    if asset.file_size == 0 {
        return Some("empty");
    }
    let extension = std::path::Path::new(&asset.original_filename)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default();
    if decode_error.is_some() && audio_metadata::supported_mvp_format(extension) {
        return Some("unreadable");
    }
    None
}

/// Settings → "Clean Up Junk Files", step one: lists every asset in this
/// library that holds no real audio. Read-only — nothing is changed until
/// the user reviews this list and confirms `clean_up_junk_files`.
#[tauri::command]
fn scan_junk_files(state: tauri::State<CatalogState>, library_id: String) -> Result<Vec<JunkAsset>, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let assets = catalog.list_assets(library_id).map_err(storage_error_message)?;
    let decode_failures: HashMap<Uuid, String> = catalog
        .decode_failures_for_library(library_id)
        .map_err(storage_error_message)?
        .into_iter()
        .collect();

    let mut junk = Vec::new();
    for asset in &assets {
        let decode_error = decode_failures.get(&asset.id);
        let Some(kind) = junk_kind_for_asset(asset, decode_error) else {
            continue;
        };
        let path = resolve_asset_path(&catalog, asset).unwrap_or_default();
        junk.push(JunkAsset {
            asset_id: asset.id.to_string(),
            filename: asset.original_filename.clone(),
            path,
            file_size: asset.file_size,
            kind,
            detail: if kind == "unreadable" { decode_error.cloned() } else { None },
        });
    }
    junk.sort_by(|left, right| left.kind.cmp(right.kind).then_with(|| left.filename.cmp(&right.filename)));
    Ok(junk)
}

#[derive(serde::Serialize, Default)]
struct JunkCleanupResult {
    /// Removed from the library for good (with a re-import block).
    removed: usize,
    /// Moved to Darkwave's Trash instead (unreadable audio — restorable).
    trashed: usize,
    /// Real files deleted from the drive (AppleDouble/system files only).
    files_deleted: usize,
    /// Ids the frontend sent that no longer classify as junk, or failed.
    skipped: usize,
    errors: Vec<String>,
}

/// Settings → "Clean Up Junk Files", step two. Every id is re-classified
/// here rather than trusting the list the UI sent, so a stale or tampered
/// selection can never remove a real track. What happens per kind:
///
/// - `apple_double` / `system_file`: removed from the library permanently
///   (`finalize_permanent_deletion`, which also leaves a re-import
///   tombstone). With `delete_files`, the file itself is also deleted from
///   the drive — safe, since by name it's an OS metadata file, never audio.
/// - `empty`: removed from the library, but the file is **never** deleted:
///   a 0-byte file can be a cloud-sync placeholder (iCloud/Dropbox/NAS
///   sync stub) whose deletion would propagate and delete the real file.
/// - `unreadable`: moved to Darkwave's Trash (restorable) and the file is
///   left alone. It has a real audio name, so the call to destroy it stays
///   with the Trash view's own explicit per-item "Delete".
#[tauri::command]
fn clean_up_junk_files(
    app: tauri::AppHandle,
    state: tauri::State<CatalogState>,
    library_id: String,
    asset_ids: Vec<String>,
    delete_files: bool,
) -> Result<JunkCleanupResult, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let decode_failures: HashMap<Uuid, String> = catalog
        .decode_failures_for_library(library_id)
        .map_err(storage_error_message)?
        .into_iter()
        .collect();
    let cache_dir = preview_cache_dir(&app).ok();
    let mut result = JunkCleanupResult::default();

    for raw_id in asset_ids {
        let asset_id = parse_uuid_field(&raw_id, "asset id")?;
        let Some(asset) = catalog.get_asset(asset_id).map_err(storage_error_message)? else {
            result.skipped += 1;
            continue;
        };
        let kind = if asset.library_id == library_id {
            junk_kind_for_asset(&asset, decode_failures.get(&asset.id))
        } else {
            None
        };
        let Some(kind) = kind else {
            result.skipped += 1;
            continue;
        };

        if kind == "unreadable" {
            match catalog.move_asset_to_trash(asset_id, "unreadable (cleanup)", current_time_ms()) {
                Ok(_) => result.trashed += 1,
                Err(error) => {
                    result.skipped += 1;
                    result.errors.push(format!("{}: {}", asset.original_filename, storage_error_message(error)));
                }
            }
            continue;
        }

        if delete_files && matches!(kind, "apple_double" | "system_file") {
            if let Ok(absolute_path) = resolve_asset_path(&catalog, &asset) {
                match std::fs::remove_file(&absolute_path) {
                    Ok(()) => result.files_deleted += 1,
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => {
                        // Leave the row in place so the user can see it's
                        // still there and retry (e.g. a read-only share).
                        result.skipped += 1;
                        result.errors.push(format!("{}: {error}", asset.original_filename));
                        continue;
                    }
                }
            }
        }

        if let Some(cache_dir) = &cache_dir {
            let _ = std::fs::remove_file(cached_file_path(cache_dir, &asset));
        }
        match catalog.finalize_permanent_deletion(asset_id) {
            Ok(()) => result.removed += 1,
            Err(error) => {
                result.skipped += 1;
                result.errors.push(format!("{}: {}", asset.original_filename, storage_error_message(error)));
            }
        }
    }

    Ok(result)
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
        // Cascades to this library's folders (and everything else scoped to
        // it) automatically — see delete_library's own doc comment — so
        // there's no separate watched-folder preference to clean up here
        // any more; folders are per-library catalog rows now, not a global
        // preference.
        catalog.delete_library(library_id).map_err(storage_error_message)?;
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

/// Backs the browser's "Recently Added" view: the newest `limit` imports,
/// newest first. `limit` comes from `AppPreferences::recent_imports_limit`
/// and is re-clamped here rather than trusted, since the frontend's number
/// input is not the only possible caller.
#[tauri::command(async)]
fn list_recent_assets(
    state: tauri::State<CatalogState>,
    library_id: String,
    limit: u32,
) -> Result<Vec<AssetRecord>, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let limit = preferences::normalize_recent_imports_limit(limit);
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .list_recent_assets(library_id, limit)
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
    format: Option<String>,
}

fn build_search_query(filters: AssetSearchFilters) -> Result<storage::AssetSearchQuery, String> {
    let tag_id = filters
        .tag_id
        .map(|id| parse_uuid_field(&id, "tag id"))
        .transpose()?;

    // search_assets (plain text search) runs the query through
    // parse_natural_language_query, so typing "sound effect explosion"
    // narrows to media_type=sound_effect + text "explosion". This command
    // (and create_smart_collection, which shares this builder) used to skip
    // that entirely and pass the raw text straight through — so the moment
    // a user also set a duration/BPM range, or saved a Smart Collection,
    // that same typed query silently stopped being media-type-aware. Only
    // parse when the caller hasn't already supplied an explicit media_type
    // (nothing does today, but a future caller reasonably could).
    let (text, media_type) = match filters.media_type {
        Some(media_type) => (filters.text.unwrap_or_default(), Some(media_type)),
        None => {
            let parsed = search::parse_natural_language_query(&filters.text.unwrap_or_default());
            (parsed.text, parsed.media_type)
        }
    };

    Ok(storage::AssetSearchQuery {
        text,
        tag_id,
        media_type,
        duration_min_ms: filters.duration_min_ms,
        duration_max_ms: filters.duration_max_ms,
        bpm_min: filters.bpm_min,
        bpm_max: filters.bpm_max,
        peak_db_min: filters.peak_db_min,
        peak_db_max: filters.peak_db_max,
        format: filters.format,
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
            license_valid_until: row.license_valid_until,
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
async fn import_folder(
    app: tauri::AppHandle,
    state: tauri::State<'_, CatalogState>,
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
            sync_media_root_folder(&catalog, library_id, &folder_path);
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

    let paths = collect_audio_files(std::path::Path::new(&folder_path))
        .map_err(|error| format!("could not read folder {folder_path}: {error}"))?;

    let folder_role = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        folder_role_for_path(&catalog, library_id, &folder_path)
    };
    let (imported, failed, skipped_deleted) =
        run_batch_import(&state, library_id, import_mode, paths, folder_role).await;

    {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        let _ = detect_and_persist_stem_groups(&catalog, library_id);
    }

    Ok(ImportFolderResult { imported, failed, skipped_deleted })
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

/// Shared import driver for `import_folder` / `refresh_library` /
/// `scan_import_folder`. Runs the metadata-probe + streamed-hash stage
/// (`import_pipeline::prepare_import`) across `paths` with bounded
/// parallelism and no catalog lock — the slow disk/NAS-bound part of an
/// import — then commits the results one file at a time, taking the catalog
/// lock per file so browsing/playback/tagging stay responsive while a big
/// library imports (ADR 0024's mutex lesson) and a first import of an
/// SSD/NVMe library isn't bottlenecked on hashing one file per core (ADR
/// 0029). `UnsupportedFormat` is a silent skip in both stages, exactly as
/// the old per-file `import_file` path treated it.
async fn run_batch_import(
    state: &tauri::State<'_, CatalogState>,
    library_id: Uuid,
    mode: ImportMode,
    paths: Vec<PathBuf>,
    folder_role: Option<String>,
) -> (Vec<AssetRecord>, Vec<ImportFailure>, Vec<String>) {
    use futures::stream::{self, StreamExt};

    // Matches AUDIO_ANALYSIS_CONCURRENCY: enough to use several cores on
    // local storage, not so much that it fires a swarm of simultaneous
    // whole-file reads at a single NAS mount or spinning disk.
    const IMPORT_PREPARE_CONCURRENCY: usize = 4;

    let mut prepared: Vec<(PathBuf, Result<import_pipeline::PreparedImport, ImportError>)> =
        stream::iter(paths)
            .map(|path| {
                let folder_role = folder_role.clone();
                async move {
                    let probe_path = path.clone();
                    let result = tauri::async_runtime::spawn_blocking(move || {
                        import_pipeline::prepare_import(&probe_path, mode, folder_role.as_deref())
                    })
                    .await
                    .unwrap_or_else(|join_error| {
                        Err(ImportError::Io(std::io::Error::other(join_error.to_string())))
                    });
                    (path, result)
                }
            })
            .buffer_unordered(IMPORT_PREPARE_CONCURRENCY)
            .collect()
            .await;

    // Commit in stable path order regardless of which prepares finished first.
    prepared.sort_by(|(a, _), (b, _)| a.cmp(b));

    let mut imported = Vec::new();
    let mut failed = Vec::new();
    let mut skipped_deleted = Vec::new();
    for (path, result) in prepared {
        let filename = path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();

        let prepared = match result {
            Ok(prepared) => prepared,
            Err(ImportError::UnsupportedFormat(_)) => continue,
            Err(error) => {
                failed.push(ImportFailure {
                    filename,
                    reason: error.to_string(),
                });
                continue;
            }
        };

        let catalog = state.0.lock().expect("catalog mutex poisoned");
        match import_pipeline::commit_prepared_import(&catalog, library_id, prepared, None) {
            Ok(import_pipeline::CommitOutcome::Imported(asset)) => imported.push(asset),
            // Not a failure — a rescan finding the exact file (by content
            // hash + size) a user already permanently deleted from this
            // library. See storage::Catalog::record_deleted_fingerprint.
            // Reported separately so it doesn't read as an error, but
            // still visibly, rather than the file just silently not
            // appearing with no explanation.
            Ok(import_pipeline::CommitOutcome::PreviouslyDeleted) => skipped_deleted.push(filename),
            Err(ImportError::UnsupportedFormat(_)) => {}
            Err(error) => failed.push(ImportFailure {
                filename,
                reason: error.to_string(),
            }),
        }
    }

    (imported, failed, skipped_deleted)
}

#[derive(Debug, serde::Serialize)]
struct DroppedImportResult {
    imported: Vec<AssetRecord>,
    failed: Vec<ImportFailure>,
    /// See `ImportFolderResult::skipped_deleted` — same meaning. For a
    /// drop, the file was already copied into the library's own folder
    /// before this is known (Managed mode has no in-place option), so that
    /// orphaned copy is removed again rather than left behind uncataloged.
    #[serde(default)]
    skipped_deleted: Vec<String>,
    /// How many stem groups (this drop's own STEMS-pattern files, plus any
    /// previously-ungrouped ones elsewhere in the library) got linked as a
    /// side effect — surfaced so the frontend can say something like "3
    /// tracks, 1 with stems" instead of staying silent about it.
    stem_groups_detected: usize,
}

/// Handles files/folders dropped onto the app window from outside it (see
/// the frontend's `onDragDropEvent` listener). Unlike `import_folder`/
/// `import_file` — which catalog a file wherever it already lives, in
/// Managed or Referenced mode — a drop's whole point is "make this the
/// library's own copy": each path is copied into `media_root` (or one of
/// its configured `import_subfolders`, when `target_subfolder` is
/// `Some`) *before* being cataloged as Referenced at that new, permanent
/// location. The original — typically sitting in Downloads — is never
/// touched and never becomes the canonical file.
///
/// `paths` can mix individual files and whole folders (a dropped "Song
/// Stems" folder, say) — folders are expanded recursively the same way
/// `import_folder`'s own folder argument already is.
// (async): copies real file bytes (possibly many, possibly large) and
// then runs the same disk/hash-bound prepare stage import_folder does —
// exactly the kind of work ADR 0024 says can't run on the main thread.
#[tauri::command(async)]
async fn import_dropped_paths(
    state: tauri::State<'_, CatalogState>,
    library_id: String,
    paths: Vec<String>,
    target_subfolder: Option<String>,
) -> Result<DroppedImportResult, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;

    let media_root = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        catalog
            .get_library(library_id)
            .map_err(storage_error_message)?
            .ok_or_else(|| "library not found".to_string())?
            .media_root
    };
    if media_root.trim().is_empty() {
        return Err("this library has no media root set yet — import a folder first".to_string());
    }

    let mut destination_dir = PathBuf::from(&media_root);
    if let Some(subfolder) = target_subfolder.as_deref().map(str::trim).filter(|name| !name.is_empty()) {
        destination_dir.push(subfolder);
    }
    std::fs::create_dir_all(&destination_dir)
        .map_err(|error| format!("could not create {}: {error}", destination_dir.display()))?;

    // Expand any dropped folders to the audio files inside them, exactly
    // like import_folder's own folder argument.
    let mut source_files: Vec<PathBuf> = Vec::new();
    for path_str in &paths {
        let path = std::path::Path::new(&path_str);
        if path.is_dir() {
            let mut nested = collect_audio_files(path)
                .map_err(|error| format!("could not read folder {path_str}: {error}"))?;
            source_files.append(&mut nested);
        } else {
            let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or_default();
            if import_pipeline::is_recognized_audio_extension(extension) {
                source_files.push(path.to_path_buf());
            }
        }
    }

    // Copy each into the library's own folder before cataloging anything
    // — a copy failure for one file just drops it from the batch (it
    // never reaches run_batch_import, so it won't appear in `imported`
    // either) rather than sinking the whole drop.
    let mut copied_paths = Vec::with_capacity(source_files.len());
    for source in &source_files {
        let Some(file_name) = source.file_name() else { continue };
        let destination = unique_destination_path(&destination_dir, file_name);
        if std::fs::copy(source, &destination).is_ok() {
            copied_paths.push(destination);
        }
    }

    // No folder-role signal here: a drop onto the app window has no
    // associated `folders` row to attribute it to (unlike a folder the
    // user explicitly imports or that the background poller watches).
    let (imported, failed, skipped_deleted) =
        run_batch_import(&state, library_id, ImportMode::Referenced, copied_paths, None).await;

    // A skipped-deleted file was already copied into the library's own
    // folder above (Managed mode's copy-first step, before
    // run_batch_import — and therefore commit_prepared_import — ever gets
    // to decide it shouldn't be cataloged). Left in place, it would sit in
    // the media folder as an uncataloged duplicate of a file the user
    // deliberately removed — remove the copy again rather than leave that
    // behind.
    for filename in &skipped_deleted {
        let _ = std::fs::remove_file(destination_dir.join(filename));
    }

    let stem_groups_detected = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        detect_and_persist_stem_groups(&catalog, library_id).unwrap_or(0)
    };

    Ok(DroppedImportResult { imported, failed, skipped_deleted, stem_groups_detected })
}

/// Appends " (2)", " (3)"... before the extension until the result doesn't
/// already exist under `dir` — the same collision convention Finder/
/// Explorer use, so a file dropped twice (or one that happens to share a
/// name with something already in the library's folder) never silently
/// overwrites or gets skipped.
fn unique_destination_path(dir: &std::path::Path, file_name: &std::ffi::OsStr) -> PathBuf {
    let candidate = dir.join(file_name);
    if !candidate.exists() {
        return candidate;
    }

    let name_path = std::path::Path::new(file_name);
    let stem = name_path.file_stem().and_then(|s| s.to_str()).unwrap_or("file");
    let extension = name_path.extension().and_then(|ext| ext.to_str());
    let mut attempt = 2u32;
    loop {
        let numbered = match extension {
            Some(ext) => format!("{stem} ({attempt}).{ext}"),
            None => format!("{stem} ({attempt})"),
        };
        let candidate = dir.join(numbered);
        if !candidate.exists() {
            return candidate;
        }
        attempt += 1;
    }
}

/// Runs stem-group detection (`import_pipeline::detect_stem_groups`) over
/// every currently un-grouped asset in a library and persists whatever it
/// finds. Called automatically after any import — a regular folder
/// import, and a drop's own `import_dropped_paths` — so a real stem set
/// gets organized without the user ever having to ask, and separately
/// exposed as the `detect_stem_groups` command for an explicit
/// retroactive scan over a library that already has scattered,
/// never-grouped stems from before this feature existed.
fn detect_and_persist_stem_groups(catalog: &Catalog, library_id: Uuid) -> Result<usize, String> {
    let candidates = catalog
        .asset_filenames_for_library(library_id)
        .map_err(storage_error_message)?;
    let groups = import_pipeline::detect_stem_groups(&candidates);
    for members in &groups {
        let group_id = Uuid::new_v4();
        catalog.set_stem_group(group_id, members).map_err(storage_error_message)?;
    }
    Ok(groups.len())
}

/// Explicit "go organize whatever's already scattered" action — for a
/// library that had stem sets imported before this feature existed (or
/// one where the automatic post-import pass, for whatever reason, missed
/// something). Returns how many new groups it formed.
#[tauri::command]
fn detect_stem_groups(state: tauri::State<CatalogState>, library_id: String) -> Result<usize, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    detect_and_persist_stem_groups(&catalog, library_id)
}

/// One stem group's full member list (primary/full-mix first) — what the
/// inspector's Stems panel shows for a selected asset that has
/// `stem_group_id` set.
#[tauri::command]
fn stem_group_members(state: tauri::State<CatalogState>, group_id: String) -> Result<Vec<AssetRecord>, String> {
    let group_id = parse_uuid_field(&group_id, "group id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog.stem_group_members(group_id).map_err(storage_error_message)
}

/// Sets (or, with an empty list, clears) the named subfolders under a
/// library's `media_root` a dropped file can be routed into — see
/// `LibraryRecord::import_subfolders`.
#[tauri::command]
fn set_library_import_subfolders(
    state: tauri::State<CatalogState>,
    library_id: String,
    names: Vec<String>,
) -> Result<LibraryRecord, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .set_library_import_subfolders(library_id, &names)
        .map_err(storage_error_message)?;
    catalog
        .get_library(library_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "library not found after setting import subfolders".to_string())
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
async fn refresh_library(
    state: tauri::State<'_, CatalogState>,
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

    // Filter out already-catalogued files before the prepare stage so a
    // rescan never re-hashes what it already knows.
    let paths: Vec<PathBuf> = collect_audio_files(std::path::Path::new(&media_root))
        .map_err(|error| format!("could not read {media_root}: {error}"))?
        .into_iter()
        .filter(|path| !known_paths.contains(&path.to_string_lossy().to_string()))
        .collect();

    let folder_role = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        folder_role_for_path(&catalog, library_id, &media_root)
    };
    let (imported, failed, skipped_deleted) =
        run_batch_import(&state, library_id, ImportMode::Referenced, paths, folder_role).await;

    {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        let _ = detect_and_persist_stem_groups(&catalog, library_id);
    }

    Ok(ImportFolderResult { imported, failed, skipped_deleted })
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
async fn scan_import_folder(
    state: tauri::State<'_, CatalogState>,
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
            skipped_deleted: Vec::new(),
        });
    };

    let paths = collect_audio_files(std::path::Path::new(&import_root))
        .map_err(|error| format!("could not read {import_root}: {error}"))?;

    let folder_role = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        folder_role_for_path(&catalog, library_id, &import_root)
    };
    let (imported, failed, skipped_deleted) =
        run_batch_import(&state, library_id, ImportMode::Managed, paths, folder_role).await;

    {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        let _ = detect_and_persist_stem_groups(&catalog, library_id);
    }

    Ok(ImportFolderResult { imported, failed, skipped_deleted })
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

/// Ceiling on how many times `defer_unavailable_job` will silently retry a
/// job whose asset file isn't reachable locally before finally calling
/// `fail_job` on it. Paired with `reset_stuck_processing_jobs`'s existing
/// 3-minute age floor (which paces how often a deferred job can even be
/// reclaimed — see that function's doc comment), this caps *silent*
/// retrying at roughly an hour of session time. Below the cap this is
/// invisible to the user by design (the common, expected case: a NAS share
/// or external drive that isn't mounted yet, or hasn't been warmed into the
/// local cache). At the cap it becomes a real, visible failure — recoverable
/// at any time afterward via the existing "Retry Failed Jobs" action, which
/// resets `attempts` back to 0 — rather than retrying forever with no way
/// for the user to know a file may simply be gone for good (a deleted
/// external drive, a renamed NAS share, a broken asset row).
const MAX_SILENT_AVAILABILITY_ATTEMPTS: i64 = 20;

/// Handles a job whose asset file couldn't be resolved or found locally
/// this attempt. Leaves the job `'processing'` and just counts the
/// attempt — *not* `requeue_job_as_pending`, which puts the row straight
/// back to `'pending'` and lets the very next claim cycle pick it right
/// back up, with no backoff at all. Since the standing background worker
/// (see `.setup()`) calls `process_{audio_analysis,waveform,instrument}_jobs`
/// on a roughly 1-second cycle for as long as the app is open, an
/// immediate requeue turned "file not reachable yet" into a tight,
/// CPU-and-power-assertion-burning loop that never stopped for the
/// lifetime of the session — the bug this replaced. Leaving the row
/// `'processing'` means it can't be reclaimed until
/// `reset_stuck_processing_jobs`'s own age-gated threshold judges the claim
/// abandoned, which is exactly the pacing this relies on.
///
/// Returns `true` if this call pushed the job over
/// `MAX_SILENT_AVAILABILITY_ATTEMPTS` and it was failed outright — the
/// caller should treat that like any other real failure (emit a progress
/// event, etc.); a `false` return means the job was deferred silently and
/// nothing user-visible happened.
fn defer_unavailable_job(catalog: &Catalog, job_id: Uuid, context: &str) -> bool {
    match catalog.mark_job_attempt(job_id) {
        Ok(attempts) if attempts >= MAX_SILENT_AVAILABILITY_ATTEMPTS => {
            let message = format!(
                "file still not reachable locally after {attempts} attempts — reconnect the drive/NAS share, then use Retry Failed Jobs"
            );
            if let Err(error) = catalog.fail_job_exhausted(job_id, &message) {
                eprintln!("{context}: failed to record unavailable-file failure for job {job_id}: {error:?}");
            }
            true
        }
        Ok(_) => false,
        Err(error) => {
            eprintln!("{context}: failed to record availability attempt for job {job_id}: {error:?}");
            false
        }
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
    cached_file_path_for(cache_dir, asset.id, &asset.original_filename)
}

/// Shared by `cached_file_path` (has a full `AssetRecord` on hand) and
/// callers that only have a trashed item's id + its original path/filename
/// (a `TrashItem` doesn't carry a whole `AssetRecord`) — same naming scheme
/// either way, so a cache entry written under one path is always found
/// under the other.
fn cached_file_path_for(cache_dir: &std::path::Path, asset_id: Uuid, filename_or_path: &str) -> PathBuf {
    let extension = std::path::Path::new(filename_or_path)
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or("bin");
    cache_dir.join(format!("{asset_id}.{extension}"))
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

/// One asset's persisted waveform: the flat transport-strip magnitudes the
/// UI draws plus the multi-resolution min/max peaks (ADR 0004) kept for
/// richer future renderers. Stored as JSON in the `waveform_peaks` table by
/// either the `WaveformGeneration` job or the analysis job (whichever
/// decodes the file first), or by the client fallback below.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
struct StoredWaveform {
    peaks: Vec<f32>,
    sample_rate: u32,
    cache: waveform::WaveformCache,
}

/// What `get_waveform` hands the frontend — just what it renders.
#[derive(Clone, serde::Serialize)]
struct WaveformResponse {
    peaks: Vec<f32>,
    sample_rate: u32,
}

fn build_waveform_payload(buffer: &audio_metadata::DecodedAudioBuffer) -> StoredWaveform {
    let cache = waveform::WaveformCache::from_samples(&buffer.samples, buffer.sample_rate);
    StoredWaveform {
        peaks: cache.transport_strip(),
        sample_rate: buffer.sample_rate,
        cache,
    }
}

fn persist_waveform_payload(
    catalog: &Catalog,
    asset_id: Uuid,
    payload: &StoredWaveform,
) -> Result<(), String> {
    let json = serde_json::to_string(payload).map_err(|error| error.to_string())?;
    catalog
        .set_waveform_cache(asset_id, i64::from(payload.sample_rate), &json)
        .map_err(storage_error_message)?;
    catalog
        .complete_pending_jobs_for_asset(asset_id, JobKind::WaveformGeneration)
        .map_err(storage_error_message)?;
    Ok(())
}

/// Returns an asset's cached waveform, or `None` if it hasn't been
/// generated yet (the frontend then computes one itself and calls
/// `store_waveform_peaks` so the miss only happens once). A corrupt stored
/// payload is treated as a miss rather than an error, for the same reason.
#[tauri::command]
fn get_waveform(
    state: tauri::State<CatalogState>,
    asset_id: String,
) -> Result<Option<WaveformResponse>, String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let Some(row) = catalog
        .get_waveform_cache(asset_id)
        .map_err(storage_error_message)?
    else {
        return Ok(None);
    };
    match serde_json::from_str::<StoredWaveform>(&row.payload) {
        Ok(stored) => Ok(Some(WaveformResponse {
            peaks: stored.peaks,
            sample_rate: stored.sample_rate,
        })),
        Err(error) => {
            eprintln!("waveform: discarding corrupt payload for {asset_id}: {error}");
            Ok(None)
        }
    }
}

/// Persists a waveform the frontend computed itself (the fallback path when
/// `get_waveform` missed) so no asset is ever decoded in the WebView more
/// than once. The array is clamped and length-capped so a bad client value
/// can't poison the cache; the multi-resolution layers are left empty since
/// the client only produces the flat strip.
#[tauri::command]
fn store_waveform_peaks(
    state: tauri::State<CatalogState>,
    asset_id: String,
    peaks: Vec<f32>,
    sample_rate: u32,
) -> Result<(), String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let peaks: Vec<f32> = peaks
        .into_iter()
        .take(4096)
        .map(|value| if value.is_finite() { value.clamp(0.0, 1.0) } else { 0.0 })
        .collect();
    let payload = StoredWaveform {
        peaks,
        sample_rate,
        cache: waveform::WaveformCache {
            sample_rate,
            row: Vec::new(),
            inspector: Vec::new(),
            transport: Vec::new(),
        },
    };
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    persist_waveform_payload(&catalog, asset_id, &payload)
}

/// Drains pending `WaveformGeneration` jobs: decode each asset's source
/// file once on a blocking thread, reduce it to a peak payload, and persist
/// it so browsing that asset never re-reads the file. Mirrors
/// `process_audio_analysis_jobs` — bounded batch, self-healing stuck
/// claims, bounded concurrency, catalog mutex held only for the short
/// reads/writes around the decode. An asset that isn't available locally
/// yet is left pending (picked back up by `reset_stuck_processing_jobs`),
/// not failed. The common case during import is that the analysis job
/// decodes first and fills this cache as a side effect, so this usually
/// finds nothing to do — it's the path for relinks, an analysis-paused
/// queue, and libraries catalogued before waveform caching existed.
#[tauri::command(async)]
async fn process_waveform_jobs(
    app: tauri::AppHandle,
    state: tauri::State<'_, CatalogState>,
    job_control: tauri::State<'_, JobControlState>,
    active_library_file: tauri::State<'_, ActiveLibraryFileState>,
) -> Result<usize, String> {
    use futures::stream::{self, StreamExt};
    use std::sync::atomic::Ordering;

    if job_control.waveform_generation_paused.load(Ordering::SeqCst) {
        return Ok(0);
    }

    let jobs = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        catalog
            .reset_stuck_processing_jobs(JobKind::WaveformGeneration)
            .map_err(storage_error_message)?;
        // Not the generic claim_pending_jobs: this skips any asset whose
        // AudioAnalysis job is also still pending/processing, since that
        // pass decodes the file once and fills the waveform cache as a side
        // effect anyway — see claim_pending_waveform_jobs's doc comment.
        catalog
            .claim_pending_waveform_jobs(24)
            .map_err(storage_error_message)?
    };

    // Lower than AUDIO_ANALYSIS_CONCURRENCY on purpose: a library catalogued
    // before waveform caching existed can carry a backlog thousands deep
    // (every pre-existing Referenced/NAS asset needs one), so this is the
    // kind most likely to be mid-backlog for a long stretch. At 4-way it
    // was pinning CPU hard enough to starve interactive work (a manual
    // Sonic Radar Analyze click, even simple track selection) of any real
    // scheduling turn for a long time — not hung, just never getting a
    // chance to run. 2-way still makes real progress without dominating
    // every core; pause it entirely (waveform_generation_paused) for
    // latency-sensitive work like a focused instrument-detection pass.
    const WAVEFORM_CONCURRENCY: usize = 2;

    let mut results = stream::iter(jobs)
        .map(|job| process_one_waveform_job(&app, &state, &active_library_file, job))
        .buffer_unordered(WAVEFORM_CONCURRENCY);

    let mut processed = 0usize;
    while let Some(counted) = results.next().await {
        if counted {
            processed += 1;
        }
    }

    Ok(processed)
}

async fn process_one_waveform_job(
    app: &tauri::AppHandle,
    state: &tauri::State<'_, CatalogState>,
    active_library_file: &tauri::State<'_, ActiveLibraryFileState>,
    job: storage::JobRecord,
) -> bool {
    use tauri::Emitter;

    let asset = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        catalog.get_asset(job.asset_id)
    };
    let asset = match asset {
        Ok(Some(asset)) => asset,
        Ok(None) => {
            let catalog = state.0.lock().expect("catalog mutex poisoned");
            if let Err(error) = catalog.fail_job(job.id, "asset not found") {
                eprintln!("waveform: failed to record missing-asset failure: {error:?}");
            }
            let _ = app.emit(
                "waveform-progress",
                JobFileProgressEvent { succeeded: false, asset_name: format!("asset {}", job.asset_id) },
            );
            return true;
        }
        Err(error) => {
            eprintln!("waveform: failed to load asset {}: {error:?}", job.asset_id);
            return false;
        }
    };

    // Already generated — complete the job without touching the file at
    // all. Covers a job that's legitimately claimable again despite the
    // asset already having a valid cache: an abandoned claim from before
    // this app restarted (reset_stuck_processing_jobs reclaims it) whose
    // prior run actually finished writing the cache but never got to mark
    // the job itself completed, or simply a stale duplicate job row from
    // before complete_pending_jobs_for_asset's 'processing' fix above.
    // clear_waveform_cache (a relink invalidation) deletes the row outright,
    // so this can't mask a genuinely-needed regeneration.
    let already_cached = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        catalog.get_waveform_cache(job.asset_id)
    };
    if matches!(already_cached, Ok(Some(_))) {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        if let Err(error) = catalog.complete_pending_jobs_for_asset(job.asset_id, JobKind::WaveformGeneration) {
            eprintln!("waveform: failed to complete already-cached job {}: {error:?}", job.id);
        }
        let _ = app.emit(
            "waveform-progress",
            JobFileProgressEvent { succeeded: true, asset_name: asset.display_name.clone() },
        );
        return true;
    }

    let local_path = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        local_asset_path(app, &catalog, &asset)
    };
    // Not warmed into the local cache yet — count the attempt and leave it
    // 'processing' (same as analysis does) so a later warm+retry, or the
    // standing worker's own next self-heal, picks it up. See
    // defer_unavailable_job's doc comment for why this must not be an
    // immediate requeue-to-pending: that turned a merely-unreachable NAS
    // path into a tight retry loop that never stopped.
    let Ok(local_path) = local_path else {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        let failed = defer_unavailable_job(&catalog, job.id, "waveform");
        if failed {
            let _ = app.emit(
                "waveform-progress",
                JobFileProgressEvent { succeeded: false, asset_name: asset.display_name.clone() },
            );
        }
        return failed;
    };
    if !std::path::Path::new(&local_path).exists() {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        let failed = defer_unavailable_job(&catalog, job.id, "waveform");
        if failed {
            let _ = app.emit(
                "waveform-progress",
                JobFileProgressEvent { succeeded: false, asset_name: asset.display_name.clone() },
            );
        }
        return failed;
    }

    let decode_path = local_path.clone();
    let library_before_decode = active_library_snapshot(active_library_file);
    let decoded = tauri::async_runtime::spawn_blocking(move || {
        audio_metadata::decode_any_supported_audio(&decode_path).map_err(|error| format!("{error:?}"))
    })
    .await;

    if active_library_changed_since(&library_before_decode, active_library_file) {
        eprintln!(
            "waveform: active library changed mid-job for asset {}, discarding result",
            job.asset_id
        );
        return false;
    }

    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let succeeded = match decoded {
        Ok(Ok(buffer)) => {
            let payload = build_waveform_payload(&buffer);
            if let Err(error) = persist_waveform_payload(&catalog, job.asset_id, &payload) {
                eprintln!("waveform: failed to persist payload for job {}: {error}", job.id);
            }
            true
        }
        Ok(Err(error)) => {
            if let Err(fail_error) = catalog.fail_job(job.id, &error) {
                eprintln!("waveform: failed to record decode failure: {fail_error:?}");
            }
            false
        }
        Err(join_error) => {
            eprintln!("waveform: decode task panicked: {join_error}");
            return false;
        }
    };
    let _ = app.emit(
        "waveform-progress",
        JobFileProgressEvent { succeeded, asset_name: asset.display_name.clone() },
    );
    true
}

/// Where the instrument model may live: the app-data `models/` dir (a
/// drop-in that needs no rebuild) takes priority over the bundled
/// `resources/models/`. Returns the pair only if BOTH files are present.
fn instrument_model_paths(app: &tauri::AppHandle) -> Option<(PathBuf, PathBuf)> {
    let dirs = [
        app.path().app_data_dir().ok().map(|dir| dir.join("models")),
        app.path().resource_dir().ok().map(|dir| dir.join("models")),
    ];
    for dir in dirs.into_iter().flatten() {
        let model = dir.join("yamnet.onnx");
        let class_map = dir.join("yamnet_class_map.csv");
        if model.is_file() && class_map.is_file() {
            return Some((model, class_map));
        }
    }
    None
}

/// Drains pending `InstrumentDetection` jobs: decode each asset once and run
/// the YAMNet model over it, persisting the folded instrument labels. If no
/// model is installed this claims nothing and returns 0 — the jobs stay
/// pending and start processing once a model is dropped in and the app
/// restarts. Inference is serialised on the model mutex (the ONNX session
/// isn't `Sync`, and it already parallelises internally), so unlike the
/// waveform/analysis workers this runs one job at a time.
#[tauri::command(async)]
async fn process_instrument_jobs(
    app: tauri::AppHandle,
    state: tauri::State<'_, CatalogState>,
    model_state: tauri::State<'_, InstrumentModelState>,
    active_library_file: tauri::State<'_, ActiveLibraryFileState>,
) -> Result<usize, String> {
    use std::sync::atomic::Ordering;

    if !model_state.tried.swap(true, Ordering::SeqCst) {
        match instrument_model_paths(&app) {
            Some((model_path, class_map_path)) => {
                let loaded = tauri::async_runtime::spawn_blocking(move || {
                    audio_analysis::load_instrument_model(&model_path, &class_map_path)
                })
                .await
                .ok()
                .flatten();
                if loaded.is_some() {
                    *model_state.model.lock().expect("instrument model mutex poisoned") = loaded;
                } else {
                    eprintln!("instrument-detection: model files found but failed to load");
                }
            }
            None => eprintln!(
                "instrument-detection: no model installed (models/yamnet.onnx + \
                 yamnet_class_map.csv) — instrument jobs will wait"
            ),
        }
    }
    if model_state
        .model
        .lock()
        .expect("instrument model mutex poisoned")
        .is_none()
    {
        return Ok(0);
    }

    let jobs = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        catalog
            .reset_stuck_processing_jobs(JobKind::InstrumentDetection)
            .map_err(storage_error_message)?;
        catalog
            .claim_pending_jobs(JobKind::InstrumentDetection, 12)
            .map_err(storage_error_message)?
    };

    let mut processed = 0usize;
    for job in jobs {
        if process_one_instrument_job(&app, &state, &model_state, &active_library_file, job).await {
            processed += 1;
        }
    }
    Ok(processed)
}

async fn process_one_instrument_job(
    app: &tauri::AppHandle,
    state: &tauri::State<'_, CatalogState>,
    model_state: &tauri::State<'_, InstrumentModelState>,
    active_library_file: &tauri::State<'_, ActiveLibraryFileState>,
    job: storage::JobRecord,
) -> bool {
    use tauri::Emitter;

    let asset = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        catalog.get_asset(job.asset_id)
    };
    let asset = match asset {
        Ok(Some(asset)) => asset,
        Ok(None) => {
            let catalog = state.0.lock().expect("catalog mutex poisoned");
            let _ = catalog.fail_job(job.id, "asset not found");
            let _ = app.emit(
                "instrument-detection-progress",
                JobFileProgressEvent { succeeded: false, asset_name: format!("asset {}", job.asset_id) },
            );
            return true;
        }
        Err(error) => {
            eprintln!("instrument-detection: failed to load asset {}: {error:?}", job.asset_id);
            return false;
        }
    };

    let local_path = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        local_asset_path(app, &catalog, &asset)
    };
    // See defer_unavailable_job's doc comment: an immediate requeue-to-
    // pending here is what turned an unreachable NAS/Referenced path into a
    // tight retry loop that never stopped.
    let Ok(local_path) = local_path else {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        let failed = defer_unavailable_job(&catalog, job.id, "instrument-detection");
        if failed {
            let _ = app.emit(
                "instrument-detection-progress",
                JobFileProgressEvent { succeeded: false, asset_name: asset.display_name.clone() },
            );
        }
        return failed;
    };
    if !std::path::Path::new(&local_path).exists() {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        let failed = defer_unavailable_job(&catalog, job.id, "instrument-detection");
        if failed {
            let _ = app.emit(
                "instrument-detection-progress",
                JobFileProgressEvent { succeeded: false, asset_name: asset.display_name.clone() },
            );
        }
        return failed;
    }

    let model = model_state.model.clone();
    let library_before_detection = active_library_snapshot(active_library_file);
    let detected = tauri::async_runtime::spawn_blocking(move || -> Result<Vec<(String, f64)>, String> {
        let buffer = audio_metadata::decode_any_supported_audio(&local_path)
            .map_err(|error| format!("{error:?}"))?;
        detect_instruments_with_timeout(model, buffer)
    })
    .await;

    if active_library_changed_since(&library_before_detection, active_library_file) {
        eprintln!(
            "instrument-detection: active library changed mid-job for asset {}, discarding result",
            job.asset_id
        );
        return false;
    }

    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let succeeded = match detected {
        Ok(Ok(instruments)) => match catalog.set_asset_instruments(job.asset_id, &instruments) {
            Ok(()) => {
                let _ = catalog.complete_job(job.id);
                true
            }
            Err(error) => {
                // Persist failed (transient DB error) — leave it failed so
                // the standing worker retries, rather than completing a job
                // that stored nothing.
                let message = format!("could not persist instruments: {error}");
                if let Err(fail_error) = catalog.fail_job(job.id, &message) {
                    eprintln!("instrument-detection: failed to record persist failure: {fail_error:?}");
                }
                false
            }
        },
        Ok(Err(error)) => {
            if let Err(fail_error) = catalog.fail_job(job.id, &error) {
                eprintln!("instrument-detection: failed to record failure: {fail_error:?}");
            }
            false
        }
        Err(join_error) => {
            eprintln!("instrument-detection: task panicked: {join_error}");
            return false;
        }
    };
    let _ = app.emit(
        "instrument-detection-progress",
        JobFileProgressEvent { succeeded, asset_name: asset.display_name.clone() },
    );
    true
}

/// Job priority resync_analysis queues at for each kind — kept alongside
/// resync_analysis rather than in `JobKind` itself since it's a UI-driven
/// "how urgent is a manual re-analysis" choice, not an intrinsic property of
/// the kind.
fn resync_priority_for_job_kind(kind: &JobKind) -> i64 {
    match kind {
        JobKind::MetadataExtraction => 10,
        JobKind::Hashing => 20,
        JobKind::WaveformGeneration => 30,
        JobKind::AudioAnalysis => 40,
        JobKind::InstrumentDetection => 50,
    }
}

/// The Sonic Radar "sync" button, and the inspector's per-track Analyze
/// buttons: re-queue analysis for a selection — or the whole library when
/// nothing is selected — so assets whose original jobs already completed
/// pick up newer detection (ADR 0032). Returns the total number of jobs
/// queued; the frontend then drives the normal drain.
///
/// `kinds` (job-kind strings, e.g. "audio_analysis", "instrument_detection")
/// lets a caller target just one pass — the inspector's Sonic Radar section
/// uses this so re-analyzing one asset doesn't also re-run every other pass
/// on it. Omitted or empty defaults to all three passes (waveform, audio
/// analysis, instrument detection), which is what the sidebar's whole-
/// selection sync button relies on. Note tempo/key/pitch/vocals are *not*
/// independently selectable: `analyze_asset_audio` decodes the file once and
/// fills all four from that single pass, so every one of them maps to the
/// same "audio_analysis" kind.
// (async): the whole-library form runs bulk INSERT…SELECT over the entire
// assets table up to three times; on a large library that's enough work
// that it must not sit on the main thread (ADR 0024's recurring lesson).
#[tauri::command(async)]
fn resync_analysis(
    state: tauri::State<'_, CatalogState>,
    library_id: String,
    asset_ids: Vec<String>,
    kinds: Option<Vec<String>>,
) -> Result<usize, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let asset_ids = asset_ids
        .iter()
        .map(|id| parse_uuid_field(id, "asset id"))
        .collect::<Result<Vec<_>, _>>()?;

    let selected_kinds = match kinds {
        Some(names) if !names.is_empty() => names
            .iter()
            .map(|name| parse_job_kind_field(name))
            .collect::<Result<Vec<_>, _>>()?,
        _ => vec![JobKind::WaveformGeneration, JobKind::AudioAnalysis, JobKind::InstrumentDetection],
    };

    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let mut queued = 0usize;
    for kind in selected_kinds {
        let priority = resync_priority_for_job_kind(&kind);
        queued += if asset_ids.is_empty() {
            catalog
                .requeue_analysis_for_library(library_id, kind, priority)
                .map_err(storage_error_message)?
        } else {
            catalog
                .requeue_analysis_for_assets(&asset_ids, kind, priority)
                .map_err(storage_error_message)?
        };
    }
    Ok(queued)
}

#[derive(serde::Serialize)]
struct AssetJobStateResponse {
    state: String,
    error: Option<String>,
}

/// The Sonic Radar inspector's per-track Analyze buttons poll this
/// directly after queuing (see resync_analysis) instead of the shared,
/// library-wide `job_status` counters — this is scoped to one asset's own
/// job row, so it can't be confused by some *other* in-flight job of the
/// same kind, which was the actual bug behind an Analyze click sometimes
/// reverting with no error and no result: the shared tracker occasionally
/// never even saw this asset's job.
#[tauri::command]
fn asset_job_state(
    state: tauri::State<CatalogState>,
    asset_id: String,
    kind: String,
) -> Result<Option<AssetJobStateResponse>, String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let kind = parse_job_kind_field(&kind)?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .latest_job_state_for_asset(asset_id, kind)
        .map(|maybe| maybe.map(|(state, error)| AssetJobStateResponse { state, error }))
        .map_err(storage_error_message)
}

/// Whether instrument detection can actually run — the model is loaded, or
/// its files are present and will load on the next job. The frontend uses
/// this to drop `instrument_detection` from the job-drain loop when there's
/// no model, so a library full of pending detection jobs doesn't flash a
/// "Detecting instruments 0%" bar on every background tick forever.
#[tauri::command]
fn instrument_detection_available(
    app: tauri::AppHandle,
    model_state: tauri::State<'_, InstrumentModelState>,
) -> bool {
    if model_state
        .model
        .lock()
        .expect("instrument model mutex poisoned")
        .is_some()
    {
        return true;
    }
    instrument_model_paths(&app).is_some()
}

#[derive(serde::Serialize)]
struct InstrumentFacet {
    name: String,
    count: i64,
}

/// Every distinct detected instrument in a library with its asset count —
/// the data behind the Instrument Detection page's pill grid.
#[tauri::command]
fn instruments_for_library(
    state: tauri::State<CatalogState>,
    library_id: String,
) -> Result<Vec<InstrumentFacet>, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    Ok(catalog
        .instrument_counts_for_library(library_id)
        .map_err(storage_error_message)?
        .into_iter()
        .map(|(name, count)| InstrumentFacet { name, count })
        .collect())
}

/// Assets that have ANY of the given instruments (OR match) — the filtered
/// list once one or more instrument pills are selected.
#[tauri::command]
fn assets_by_instruments(
    state: tauri::State<CatalogState>,
    library_id: String,
    instruments: Vec<String>,
) -> Result<Vec<AssetRecord>, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .assets_with_any_instrument(library_id, &instruments)
        .map_err(storage_error_message)
}

#[derive(serde::Serialize)]
struct InstrumentTag {
    name: String,
    confidence: f64,
}

/// One asset's detected instruments (for the inspector), strongest first.
#[tauri::command]
fn asset_instruments(
    state: tauri::State<CatalogState>,
    asset_id: String,
) -> Result<Vec<InstrumentTag>, String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    Ok(catalog
        .instruments_for_asset(asset_id)
        .map_err(storage_error_message)?
        .into_iter()
        .map(|(name, confidence)| InstrumentTag { name, confidence })
        .collect())
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

/// Library-scoped pending/failed/completed counts for the three job kinds
/// the frontend drives to completion: `process_pending_jobs`
/// (metadata_extraction), `process_waveform_jobs` (waveform_generation),
/// and `process_audio_analysis_jobs` (audio_analysis). WaveformGeneration
/// used to complete only when the frontend previewed a sound; it is now a
/// real backend queue (ADR 0029), so it reports progress like the others.
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
        (JobKind::WaveformGeneration, "waveform_generation"),
        (JobKind::InstrumentDetection, "instrument_detection"),
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
        "waveform_generation" => Ok(JobKind::WaveformGeneration),
        "instrument_detection" => Ok(JobKind::InstrumentDetection),
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

#[derive(Debug, serde::Serialize)]
struct FailedJobAsset {
    asset_id: String,
    name: String,
    path: String,
    error: Option<String>,
}

/// Name, resolved absolute path, and last error message for every
/// currently-failed job of one kind — lets the Background Activity panel
/// point at the exact files (and derive per-extension counts client-side)
/// so a failure can actually be investigated, instead of only reporting a
/// count. Supersedes the old extensions-only `failed_job_extensions`
/// command. Queried once when building a completion summary, not on every
/// job_status poll, since it's only useful once there's actually something
/// to report.
#[tauri::command]
fn failed_job_assets(
    state: tauri::State<CatalogState>,
    library_id: String,
    kind: String,
) -> Result<Vec<FailedJobAsset>, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let job_kind = parse_job_kind_field(&kind)?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let details = catalog
        .failed_job_details_for_library(library_id, job_kind)
        .map_err(storage_error_message)?;

    Ok(details
        .into_iter()
        .filter_map(|(asset_id, error)| {
            let asset = catalog.get_asset(asset_id).ok().flatten()?;
            let path = resolve_asset_path(&catalog, &asset).unwrap_or_else(|_| asset.original_filename.clone());
            Some(FailedJobAsset {
                asset_id: asset_id.to_string(),
                name: asset.original_filename,
                path,
                error,
            })
        })
        .collect())
}

/// Emitted per-job (not per-batch) by the audio-analysis, waveform, and
/// instrument-detection processors under their own event names
/// ("audio-analysis-progress", "waveform-progress",
/// "instrument-detection-progress") — shared shape since all three just
/// need "which file, did it work" so the Background Activity panel can show
/// what's actually running under each kind's progress bar, not just its
/// count. Only emitted for a job that reached a real, definitive outcome
/// (succeeded, genuinely failed, or finally gave up after
/// MAX_SILENT_AVAILABILITY_ATTEMPTS) — a silently-deferred retry for an
/// unreachable file emits nothing, same as before, since that's not
/// user-visible progress.
#[derive(Clone, serde::Serialize)]
struct JobFileProgressEvent {
    succeeded: bool,
    asset_name: String,
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
    active_library_file: tauri::State<'_, ActiveLibraryFileState>,
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

    // Bounded rather than "all claimed jobs at once": the CPU-bound
    // decode/DSP/VAD work already gets real OS-thread parallelism via
    // spawn_blocking inside analyze_asset_audio, so this just caps how many
    // files are in flight together at a time — a big batch shouldn't launch
    // 20 concurrent NAS reads simultaneously. Previously this was a strict
    // one-at-a-time loop, which left a 32-thread machine mostly idle during
    // a large import (see docs/.../bug-log.md OPEN-3).
    const AUDIO_ANALYSIS_CONCURRENCY: usize = 4;

    let mut results = stream::iter(jobs)
        .map(|job| process_one_audio_analysis_job(&app, &state, &job_control, &active_library_file, job))
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
/// when the queue was paused before this job's turn came up.
///
/// That pause case explicitly requeues the job back to `'pending'` rather
/// than leaving it `'processing'` for `reset_stuck_processing_jobs` to find
/// later — this used to rely on that reset, back when it ran unconditionally
/// on every claim cycle and so recovered an abandoned claim almost
/// instantly. Once reset gained an age floor (so it stops yanking back
/// claims that are still genuinely in flight — see its doc comment), a job
/// abandoned here by a pause toggled mid-batch would otherwise sit
/// `'processing'` and invisible to `claim_pending_jobs` for however long is
/// left of that floor before it's claimable again. An explicit requeue on
/// the *known* reason (paused, not abandoned) needs no such wait.
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
    active_library_file: &tauri::State<'_, ActiveLibraryFileState>,
    job: storage::JobRecord,
) -> bool {
    use std::sync::atomic::Ordering;
    use tauri::Emitter;

    if job_control.audio_analysis_paused.load(Ordering::SeqCst) {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        if let Err(error) = catalog.requeue_job_as_pending(job.id) {
            eprintln!("audio-analysis: failed to requeue paused job {}: {error:?}", job.id);
        }
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
        let _ = app.emit(
            "audio-analysis-progress",
            JobFileProgressEvent { succeeded: false, asset_name: format!("asset {}", job.asset_id) },
        );
        return true;
    };

    let local_path = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        local_asset_path(app, &catalog, &asset)
    };
    // Referenced/NAS assets not yet warmed into the local cache: count the
    // attempt and leave the job 'processing' rather than failing it (or
    // bouncing it straight back to 'pending', which is what used to turn
    // this into a tight, never-ending retry loop — see
    // defer_unavailable_job's doc comment), so a later warm+retry, or the
    // standing worker's own next self-heal, can pick it up. Only once
    // MAX_SILENT_AVAILABILITY_ATTEMPTS is exceeded does this become a real,
    // reported failure.
    let Ok(local_path) = local_path else {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        let failed = defer_unavailable_job(&catalog, job.id, "audio-analysis");
        if failed {
            let _ = app.emit(
                "audio-analysis-progress",
                JobFileProgressEvent { succeeded: false, asset_name: asset.display_name.clone() },
            );
        }
        return failed;
    };
    if !std::path::Path::new(&local_path).exists() {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        let failed = defer_unavailable_job(&catalog, job.id, "audio-analysis");
        if failed {
            let _ = app.emit(
                "audio-analysis-progress",
                JobFileProgressEvent { succeeded: false, asset_name: asset.display_name.clone() },
            );
        }
        return failed;
    }

    let library_before_analysis = active_library_snapshot(active_library_file);
    let outcome = analyze_asset_audio(&local_path).await;
    let succeeded = outcome.is_ok();

    if active_library_changed_since(&library_before_analysis, active_library_file) {
        // The active library was swapped mid-analysis (e.g. File > Open
        // Library / Last Open while a batch was running) — the catalog this
        // job was claimed from is gone, and the now-active one has no row
        // for job.id/asset_id. Drop the result instead of writing it into
        // the wrong catalog or reporting a false success; the abandoned
        // 'processing' row self-heals via reset_stuck_processing_jobs once
        // its own library is reopened.
        eprintln!(
            "audio-analysis: active library changed mid-job for asset {}, discarding result",
            job.asset_id
        );
        return false;
    }

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

    let _ = app.emit(
        "audio-analysis-progress",
        JobFileProgressEvent { succeeded, asset_name: asset.display_name.clone() },
    );
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
    // The decode this analysis just did is also everything the waveform
    // cache needs — fill it now and mark the WaveformGeneration job done,
    // so process_waveform_jobs won't decode the same file again. A failure
    // here is logged, not fatal: the standalone job is still pending and
    // will regenerate it.
    if let Err(error) = persist_waveform_payload(catalog, asset_id, &outcome.waveform) {
        eprintln!("waveform: failed to persist alongside analysis for {asset_id}: {error}");
    }
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

/// Same contract as set_audio_analysis_paused, for WaveformGeneration.
#[tauri::command]
fn set_waveform_generation_paused(job_control: tauri::State<JobControlState>, paused: bool) {
    job_control
        .waveform_generation_paused
        .store(paused, std::sync::atomic::Ordering::SeqCst);
}

#[tauri::command]
fn waveform_generation_paused(job_control: tauri::State<JobControlState>) -> bool {
    job_control
        .waveform_generation_paused
        .load(std::sync::atomic::Ordering::SeqCst)
}

/// Ceiling on how long `detect_vocal_ratio_with_timeout` waits for Silero
/// VAD (via the `ort`/ONNX Runtime crate) before giving up and reporting no
/// vocal data. Real VAD inference over even a full-length track finishes in
/// well under a second (chunked at 512 samples, CPU-only, one inference
/// engine thread — see `voice_activity_detector`'s session config); this
/// exists for the failure mode actually observed on one real Windows
/// machine, not the ordinary case.
const VOCAL_DETECTION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Runs Silero VAD on a background thread and gives up after
/// `VOCAL_DETECTION_TIMEOUT` instead of waiting forever.
///
/// `voice_activity_detector`'s ONNX session is a single process-wide
/// `LazyLock`, built lazily on whichever thread first calls into it; if that
/// first build (or a later `session.run`) ever wedges — an unresponsive
/// ONNX Runtime/execution-provider init has been observed for real on one
/// Windows machine, no error, no panic, it just never returns — every
/// thread that ever reaches this code queues up behind the same stuck
/// `LazyLock`/mutex forever, one at a time, and audio-analysis jobs pile up
/// as permanently `'processing'` without ever completing or failing (this
/// is the bug: not the retry-loop this whole job-queue design already
/// guards against, but the analysis step itself never returning at all).
/// Bounding the wait on a detached thread can't un-stick that thread, but it
/// stops one wedged native call from wedging every future job too — the
/// caller gets `None` (already the documented graceful-degradation case for
/// vocal detection, see `docs/development/windows-setup.md`) and moves on.
fn detect_vocal_ratio_with_timeout(mono: Vec<f32>, sample_rate: u32) -> Option<f32> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // Errors are expected here if the receiving end already gave up
        // and dropped `rx` after timing out — this thread may still be
        // stuck in ONNX Runtime long after its job has moved on.
        let _ = tx.send(audio_analysis::detect_vocal_ratio_from_mono(&mono, sample_rate));
    });
    match rx.recv_timeout(VOCAL_DETECTION_TIMEOUT) {
        Ok(vocal_ratio) => vocal_ratio,
        Err(_) => {
            eprintln!(
                "audio-analysis: vocal-ratio detection did not return within {VOCAL_DETECTION_TIMEOUT:?} — \
                 treating as no vocal data (see detect_vocal_ratio_with_timeout's doc comment)"
            );
            None
        }
    }
}

/// Same ceiling as `VOCAL_DETECTION_TIMEOUT`, for the same `ort`/ONNX
/// Runtime failure mode in the instrument-detection model instead of Silero
/// VAD — see `detect_instruments_with_timeout`'s doc comment.
const INSTRUMENT_DETECTION_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(15);

/// Runs YAMNet instrument detection on a detached thread and gives up after
/// `INSTRUMENT_DETECTION_TIMEOUT` instead of waiting forever.
///
/// Instrument detection already runs its jobs one at a time, not
/// concurrently (see `process_instrument_jobs`'s doc comment — the ONNX
/// session isn't `Sync`) — which makes an unbounded hang here *worse* than
/// the vocal-ratio case `detect_vocal_ratio_with_timeout` guards against,
/// not just as bad: a single wedged `Session::run` wouldn't just queue
/// concurrent work behind it, it would stop every instrument-detection job
/// after it, forever, for the rest of the session. Same fix, same
/// reasoning: bounding the wait on a detached thread can't un-stick that
/// thread, but it stops one wedged native call from wedging every future
/// job too.
///
/// A timeout degrades to `Ok(vec![])` — "nothing detected" is already a
/// real, expected outcome this job kind handles, so a single flaky file
/// shouldn't turn into a hard failure the ordinary retry path treats any
/// differently. The model-unloaded case stays a real `Err`, though — that's
/// an immediate, deterministic condition (not a flaky hang) worth
/// surfacing distinctly, same as before this change.
fn detect_instruments_with_timeout(
    model: std::sync::Arc<Mutex<Option<audio_analysis::InstrumentModel>>>,
    buffer: audio_metadata::DecodedAudioBuffer,
) -> Result<Vec<(String, f64)>, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        // Errors are expected here if the receiving end already gave up
        // and dropped `rx` after timing out — this thread may still be
        // stuck in ONNX Runtime long after its job has moved on.
        let mut guard = model.lock().expect("instrument model mutex poisoned");
        let result = match guard.as_mut() {
            Some(model) => Ok(audio_analysis::detect_instruments(model, &buffer)
                .into_iter()
                .map(|prediction| (prediction.instrument, prediction.confidence as f64))
                .collect()),
            None => Err("instrument model unloaded".to_string()),
        };
        let _ = tx.send(result);
    });
    match rx.recv_timeout(INSTRUMENT_DETECTION_TIMEOUT) {
        Ok(result) => result,
        Err(_) => {
            eprintln!(
                "instrument-detection: model inference did not return within \
                 {INSTRUMENT_DETECTION_TIMEOUT:?} — treating as no instruments detected \
                 (see detect_instruments_with_timeout's doc comment)"
            );
            Ok(Vec::new())
        }
    }
}

struct AudioAnalysisOutcome {
    needs_review: bool,
    suggested_tags: Vec<&'static str>,
    vocal_ratio: Option<f32>,
    update: storage::AudioAnalysisUpdate,
    /// Built from the same decoded buffer as the analysis, so the common
    /// import path decodes each file once and fills the waveform cache as a
    /// side effect rather than making `process_waveform_jobs` decode it a
    /// second time.
    waveform: StoredWaveform,
}

async fn analyze_asset_audio(path: &str) -> Result<AudioAnalysisOutcome, String> {
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
    let (needs_review, suggested_tags, vocal_ratio, update, waveform) =
        tauri::async_runtime::spawn_blocking(move || -> Result<_, String> {
            let buffer = audio_metadata::decode_any_supported_audio(&path_owned)
                .map_err(|error| format!("{error:?}"))?;

            let waveform = build_waveform_payload(&buffer);
            let needs_review = audio_analysis::is_likely_silent_or_corrupt(&buffer);

            // Every one of tempo/pitch/key/vocal-ratio/measure independently
            // downmixes the same decoded buffer to mono internally — for a
            // multi-minute file that's the same O(n) pass paid six times
            // over. Downmix once here and hand every pass the same slice
            // instead (see audio_analysis::measure_from_mono's doc comment).
            let mono = audio_analysis::mono_samples(&buffer);
            let sample_rate = buffer.sample_rate;

            let measurements = audio_analysis::measure_from_mono(&mono, sample_rate);
            let suggested_tags =
                audio_analysis::suggest_action_tags_from_mono(&mono, sample_rate, measurements)
                    .into_iter()
                    .map(|tag| tag.as_str())
                    .collect::<Vec<_>>();
            let tempo = audio_analysis::estimate_tempo_from_mono(&mono, sample_rate);
            let pitch = audio_analysis::estimate_pitch_from_mono(&mono, sample_rate);
            let key = audio_analysis::estimate_key_from_mono(&mono, sample_rate);
            let vocal_ratio = detect_vocal_ratio_with_timeout(mono.clone(), sample_rate);
            // In-process DSP (see crates/audio-analysis's "Perceptual
            // fingerprint" section and docs/adr/0033 for why this replaced
            // a bliss-rs sidecar subprocess) — cheap enough, and reuses
            // `mono`/`measurements` already computed above, to just be part
            // of this same synchronous pass instead of needing its own
            // detached task and follow-up database write the way the old
            // subprocess-based version did.
            let fingerprint = audio_analysis::compute_fingerprint_from_mono(&mono, sample_rate, measurements, tempo);
            let fingerprint_json = serde_json::to_string(&fingerprint).ok();

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
                perceptual_fingerprint: fingerprint_json,
                detected_key: key.as_ref().map(|estimate| estimate.key.clone()),
                key_strength: key.as_ref().map(|estimate| estimate.strength as f64),
            };

            Ok((needs_review, suggested_tags, vocal_ratio, update, waveform))
        })
        .await
        .map_err(|error| format!("audio analysis task panicked: {error}"))??;

    Ok(AudioAnalysisOutcome {
        needs_review,
        suggested_tags,
        vocal_ratio,
        update,
        waveform,
    })
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
            let distance = audio_analysis::fingerprint_distance(&target_vector, &vector);
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
        .map_err(storage_error_message)?;
    // relink_asset drops the now-wrong waveform cache; queue a fresh
    // generation pass for the new file so the strip repopulates without
    // waiting for the next preview.
    let _ = catalog.enqueue_job(asset_id, JobKind::WaveformGeneration, 30);
    // The old file's instruments no longer describe the new one — clear
    // them and queue a re-detect.
    let _ = catalog.set_asset_instruments(asset_id, &[]);
    let _ = catalog.enqueue_job(asset_id, JobKind::InstrumentDetection, 50);
    Ok(())
}

#[tauri::command]
fn undo_action(state: tauri::State<CatalogState>, undo_id: String) -> Result<(), String> {
    let undo_id = parse_uuid_field(&undo_id, "undo id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    undo_with_files(&catalog, undo_id)
}

/// `Catalog::undo`, plus the on-disk half of an Organize run that moved files.
fn undo_with_files(catalog: &Catalog, undo_id: Uuid) -> Result<(), String> {
    if let Some((kind, payload, false)) = catalog.undo_entry(undo_id).map_err(storage_error_message)? {
        if kind == ORGANIZE_MOVES_UNDO_KIND {
            let undo: OrganizeMovesUndo = serde_json::from_str(&payload).map_err(|error| error.to_string())?;
            replay_organize_moves(catalog, &undo, false);
            catalog.undo(undo.category_undo).map_err(storage_error_message)?;
        }
    }
    catalog.undo(undo_id).map_err(storage_error_message)
}

#[tauri::command]
fn redo_action(state: tauri::State<CatalogState>, undo_id: String) -> Result<(), String> {
    let undo_id = parse_uuid_field(&undo_id, "undo id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    redo_with_files(&catalog, undo_id)
}

fn redo_with_files(catalog: &Catalog, undo_id: Uuid) -> Result<(), String> {
    if let Some((kind, payload, true)) = catalog.undo_entry(undo_id).map_err(storage_error_message)? {
        if kind == ORGANIZE_MOVES_UNDO_KIND {
            let undo: OrganizeMovesUndo = serde_json::from_str(&payload).map_err(|error| error.to_string())?;
            catalog.redo(undo.category_undo).map_err(storage_error_message)?;
            replay_organize_moves(catalog, &undo, true);
        }
    }
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

// --- Resolve Live Bridge ---------------------------------------------------
//
// Structure sync + one-click send-to-timeline for DaVinci Resolve, via the
// on-demand Python bridge at apps/resolve-bridge/ (see its README — a fresh
// process per call, not a resident worker like the similarity-worker
// sidecar: Resolve's connection state can go stale the instant Resolve is
// closed/reopened or its current project switches, and call volume here is
// low/user-triggered, so residency's complexity buys nothing). See
// docs/development/editor-workflow-section.md for where this fits.

/// Same role vocabulary `project_export_folders.role` already uses
/// (music/sound_effect/voiceover/foley/ambience/documents) — `documents` is
/// skipped, same as everywhere else audio export routing happens.
const RESOLVE_SYNC_ROLES: &[&str] = &["music", "sound_effect", "voiceover", "foley", "ambience"];

/// Locates `resolve_bridge.py` — the bundled copy first (production, via
/// the `resources` entry in tauri.conf.json), falling back to the source
/// tree's own copy for `cargo run`/`tauri dev`. Mirrors
/// `instrument_model_paths`'s app-data-then-bundled fallback shape, adapted
/// since this file has no equivalent of a user-droppable override.
/// Locates the bundled "Open Source Licenses" page (Help menu) — same
/// bundled-resource-first, dev-workspace-fallback pattern as
/// `resolve_bridge_script_path` below. Regenerated by
/// `scripts/generate-third-party-notices.sh`, not written by hand — see
/// docs/adr/0034-third-party-notices.md.
fn third_party_notices_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    if let Ok(resource_dir) = app.path().resource_dir() {
        let bundled = resource_dir.join("third-party-notices.html");
        if bundled.is_file() {
            return Ok(bundled);
        }
    }
    let dev_path = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/resources/third-party-notices.html"
    ));
    if dev_path.is_file() {
        return Ok(dev_path);
    }
    Err("could not locate third-party-notices.html (checked bundled resources and the dev workspace path — run scripts/generate-third-party-notices.sh)".to_string())
}

fn resolve_bridge_script_path(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    if let Ok(resource_dir) = app.path().resource_dir() {
        let bundled = resource_dir.join("resolve-bridge").join("resolve_bridge.py");
        if bundled.is_file() {
            return Ok(bundled);
        }
    }
    let dev_path = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../resolve-bridge/resolve_bridge.py"
    ));
    if dev_path.is_file() {
        return Ok(dev_path);
    }
    Err("could not locate resolve_bridge.py (checked bundled resources and the dev workspace path)".to_string())
}

/// Runs one action against `resolve_bridge.py`: spawns `python3 <script>`,
/// writes the JSON request to its stdin, closes stdin (so the script's
/// `json.load(sys.stdin)` sees EOF instead of blocking forever), then reads
/// its JSON response from stdout. Plain `std::process::Command` inside
/// `spawn_blocking`, not `tauri_plugin_shell` — the shell plugin's
/// `CommandChild` has no way to close stdin without dropping the whole
/// child, which this one-shot write-then-read-to-completion flow needs.
async fn run_resolve_bridge_action(
    app: &tauri::AppHandle,
    request: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let script_path = resolve_bridge_script_path(app)?;
    let request_json = serde_json::to_string(&request)
        .map_err(|error| format!("could not serialize resolve-bridge request: {error}"))?;

    match tauri::async_runtime::spawn_blocking(move || -> Result<serde_json::Value, String> {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut child = Command::new("python3")
            .arg(&script_path)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("could not start python3 — is Python 3 installed and on PATH? ({error})"))?;

        let mut stdin = child.stdin.take().expect("stdin was piped");
        stdin
            .write_all(request_json.as_bytes())
            .map_err(|error| format!("failed writing to resolve-bridge stdin: {error}"))?;
        drop(stdin); // closes the pipe — signals EOF to the script's json.load(sys.stdin)

        let output = child
            .wait_with_output()
            .map_err(|error| format!("resolve-bridge process error: {error}"))?;

        let stdout = String::from_utf8_lossy(&output.stdout);
        let stdout = stdout.trim();
        if stdout.is_empty() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(format!("resolve-bridge produced no output (stderr: {stderr})"));
        }
        serde_json::from_str(stdout).map_err(|error| {
            format!("could not parse resolve-bridge output as JSON: {error} (raw: {stdout})")
        })
    })
    .await
    {
        Ok(result) => result,
        Err(join_error) => Err(format!("resolve-bridge task panicked: {join_error}")),
    }
}

/// Builds the role→present mapping for a project's current export
/// configuration: every `project_export_folders` role, plus `music` if
/// `export_path` is set and `sound_effect` if `sfx_export_path` is set (the
/// same two built-in slots `export_asset_to_project` already falls back to)
/// — a reasonable bin-structure approximation of that same destination
/// logic, not a byte-for-byte reproduction of it (this only decides which
/// *organizational bins* exist, not where a real export actually lands).
fn resolve_sync_roles_for_project(catalog: &Catalog, project: &CollectionRecord) -> Vec<String> {
    let mut roles: Vec<String> = catalog
        .list_project_export_folders(project.id)
        .unwrap_or_default()
        .into_iter()
        .map(|folder| folder.role)
        .filter(|role| RESOLVE_SYNC_ROLES.contains(&role.as_str()))
        .collect();
    if project.export_path.is_some() && !roles.iter().any(|role| role == "music") {
        roles.push("music".to_string());
    }
    if project.sfx_export_path.is_some() && !roles.iter().any(|role| role == "sound_effect") {
        roles.push("sound_effect".to_string());
    }
    roles.sort();
    roles.dedup();
    roles
}

/// Fire-and-forget: marks the project `'syncing'`, runs `sync_structure`
/// against the bridge, records the outcome. Called from `create_project`
/// and every export-folder mutation command below — never awaited by its
/// caller, so a slow or hung Resolve/Python call can't block the UI action
/// that triggered it (matches the "smooth and invisible" design intent —
/// see docs/development/editor-workflow-section.md).
fn spawn_resolve_structure_sync(app: tauri::AppHandle, project_id: Uuid) {
    tauri::async_runtime::spawn(async move {
        let payload = {
            let catalog_state = app.state::<CatalogState>();
            let catalog = catalog_state.0.lock().expect("catalog mutex poisoned");
            let Ok(Some(project)) = catalog.get_collection(project_id) else {
                return;
            };
            let roles = resolve_sync_roles_for_project(&catalog, &project);
            if roles.is_empty() {
                // Nothing configured to organize yet — leave status at
                // whatever it already is (likely 'not_synced') rather than
                // recording a pointless sync attempt.
                return;
            }
            if let Err(error) = catalog.set_resolve_sync_syncing(project_id) {
                eprintln!("resolve-bridge: failed to mark project {project_id} syncing: {error:?}");
                return;
            }
            serde_json::json!({ "action": "sync_structure", "project_name": project.name, "roles": roles })
        };

        let result = run_resolve_bridge_action(&app, payload).await;
        let catalog_state = app.state::<CatalogState>();
        let catalog = catalog_state.0.lock().expect("catalog mutex poisoned");
        let error_message = result.as_ref().err().cloned().or_else(|| {
            result.as_ref().ok().and_then(|value| {
                (!value.get("success").and_then(|v| v.as_bool()).unwrap_or(false))
                    .then(|| value.get("error").and_then(|e| e.as_str()).unwrap_or("unknown error").to_string())
            })
        });
        if let Err(storage_error) = catalog.set_resolve_sync_result(project_id, error_message.as_deref()) {
            eprintln!("resolve-bridge: failed to record sync result for project {project_id}: {storage_error:?}");
        }
    });
}

#[tauri::command]
fn get_project_resolve_sync_status(
    state: tauri::State<CatalogState>,
    project_id: String,
) -> Result<ResolveSyncStatus, String> {
    let project_id = parse_uuid_field(&project_id, "project id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog.resolve_sync_status(project_id).map_err(storage_error_message)
}

/// Polled by the Background Activity panel — only returns projects that
/// have ever been synced (or attempted), so a library full of ordinary
/// projects that don't use the Resolve bridge at all doesn't show anything.
#[tauri::command]
fn get_resolve_sync_statuses_for_library(
    state: tauri::State<CatalogState>,
    library_id: String,
) -> Result<Vec<ResolveSyncStatus>, String> {
    let library_id = parse_uuid_field(&library_id, "library id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .resolve_sync_statuses_for_library(library_id)
        .map_err(storage_error_message)
}

/// Gates one-click send-to-timeline — see `Catalog::approve_resolve_sync`'s
/// doc comment for why this refuses anything but a `'synced'` project.
#[tauri::command]
fn approve_project_resolve_sync(state: tauri::State<CatalogState>, project_id: String) -> Result<(), String> {
    let project_id = parse_uuid_field(&project_id, "project id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog.approve_resolve_sync(project_id).map_err(storage_error_message)
}

/// The one-click "send to timeline" action — refuses unless the target
/// project's structure has been synced *and* approved (see
/// `Catalog::approve_resolve_sync`). Unlike structure sync, this one is
/// awaited by its caller (the user is waiting on this specific action to
/// finish, unlike the invisible background sync), same pattern as the
/// existing Reveal/Copy-Path actions' `editorActionStatus` feedback.
#[tauri::command(async)]
async fn send_asset_to_resolve_timeline(
    app: tauri::AppHandle,
    state: tauri::State<'_, CatalogState>,
    asset_id: String,
    project_id: String,
) -> Result<String, String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let project_id = parse_uuid_field(&project_id, "project id")?;

    let (project_name, role, file_path) = {
        let catalog = state.0.lock().expect("catalog mutex poisoned");
        let sync_status = catalog.resolve_sync_status(project_id).map_err(storage_error_message)?;
        if sync_status.approved_at.is_none() {
            return Err(
                "this project's Resolve structure hasn't been approved yet — sync and approve it first"
                    .to_string(),
            );
        }
        let project = catalog
            .get_collection(project_id)
            .map_err(storage_error_message)?
            .ok_or_else(|| "project not found".to_string())?;
        let asset = catalog
            .get_asset(asset_id)
            .map_err(storage_error_message)?
            .ok_or_else(|| "asset not found".to_string())?;
        let file_path = local_asset_path(&app, &catalog, &asset)?;
        (project.name, asset.media_type, file_path)
    };

    let payload = serde_json::json!({
        "action": "send_to_timeline",
        "project_name": project_name,
        "role": role,
        "file_path": file_path,
    });
    let result = run_resolve_bridge_action(&app, payload).await?;
    if !result.get("success").and_then(|v| v.as_bool()).unwrap_or(false) {
        let message = result.get("error").and_then(|e| e.as_str()).unwrap_or("unknown error");
        return Err(message.to_string());
    }
    let placed_at_playhead = result.get("placed_at_playhead").and_then(|v| v.as_bool()).unwrap_or(false);
    Ok(if placed_at_playhead {
        "Sent to Resolve at the playhead".to_string()
    } else {
        "Sent to Resolve (appended to timeline)".to_string()
    })
}

#[tauri::command]
fn create_project(
    app: tauri::AppHandle,
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
    let project = catalog
        .get_collection(project.id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "project not found after creation".to_string())?;
    spawn_resolve_structure_sync(app, project.id);
    Ok(project)
}

#[tauri::command]
fn set_project_export_path(
    app: tauri::AppHandle,
    state: tauri::State<CatalogState>,
    project_id: String,
    export_path: Option<String>,
) -> Result<CollectionRecord, String> {
    let project_id = parse_uuid_field(&project_id, "project id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .set_collection_export_path(project_id, export_path.as_deref())
        .map_err(storage_error_message)?;
    let _ = catalog.clear_resolve_sync_approval(project_id);
    let project = catalog
        .get_collection(project_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "project not found".to_string())?;
    spawn_resolve_structure_sync(app, project_id);
    Ok(project)
}

#[tauri::command]
fn set_project_sfx_export_path(
    app: tauri::AppHandle,
    state: tauri::State<CatalogState>,
    project_id: String,
    sfx_export_path: Option<String>,
) -> Result<CollectionRecord, String> {
    let project_id = parse_uuid_field(&project_id, "project id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .set_collection_sfx_export_path(project_id, sfx_export_path.as_deref())
        .map_err(storage_error_message)?;
    let _ = catalog.clear_resolve_sync_approval(project_id);
    let project = catalog
        .get_collection(project_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "project not found".to_string())?;
    spawn_resolve_structure_sync(app, project_id);
    Ok(project)
}

/// The sidebar's hover-to-reveal edit icon: renames a project.
#[tauri::command]
fn rename_project(
    state: tauri::State<CatalogState>,
    project_id: String,
    name: String,
) -> Result<CollectionRecord, String> {
    let project_id = parse_uuid_field(&project_id, "project id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .rename_collection(project_id, &name)
        .map_err(storage_error_message)?;
    catalog
        .get_collection(project_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "project not found".to_string())
}

/// Adds a categorized export folder to a project — the advanced
/// counterpart to its two built-in `export_path`/`sfx_export_path` slots.
/// `role` is one of the same media-type-ish strings a library `folders`
/// row uses (music, sound_effect, voiceover, foley, ambience, documents);
/// `export_asset_to_project` checks these first before falling back to the
/// built-in two-bucket split.
#[tauri::command]
fn add_project_export_folder(
    app: tauri::AppHandle,
    state: tauri::State<CatalogState>,
    project_id: String,
    path: String,
    role: String,
) -> Result<ProjectExportFolderRecord, String> {
    let project_id = parse_uuid_field(&project_id, "project id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    let folder = catalog
        .add_project_export_folder(project_id, path, role)
        .map_err(storage_error_message)?;
    let _ = catalog.clear_resolve_sync_approval(project_id);
    spawn_resolve_structure_sync(app, project_id);
    Ok(folder)
}

#[tauri::command]
fn list_project_export_folders(
    state: tauri::State<CatalogState>,
    project_id: String,
) -> Result<Vec<ProjectExportFolderRecord>, String> {
    let project_id = parse_uuid_field(&project_id, "project id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .list_project_export_folders(project_id)
        .map_err(storage_error_message)
}

#[tauri::command]
fn remove_project_export_folder(
    app: tauri::AppHandle,
    state: tauri::State<CatalogState>,
    folder_id: String,
) -> Result<(), String> {
    let folder_id = parse_uuid_field(&folder_id, "folder id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    // Looked up before deleting — the row (and its collection_id) won't
    // exist to look up afterward.
    let project_id = catalog.export_folder_collection_id(folder_id).ok().flatten();
    catalog
        .remove_project_export_folder(folder_id)
        .map_err(storage_error_message)?;
    if let Some(project_id) = project_id {
        let _ = catalog.clear_resolve_sync_approval(project_id);
        spawn_resolve_structure_sync(app, project_id);
    }
    Ok(())
}

#[tauri::command]
fn set_project_export_folder_role(
    state: tauri::State<CatalogState>,
    folder_id: String,
    role: String,
) -> Result<(), String> {
    let folder_id = parse_uuid_field(&folder_id, "folder id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    catalog
        .set_project_export_folder_role(folder_id, role)
        .map_err(storage_error_message)
}

/// The "editor's dream" button: copies one sound straight into a project's
/// configured folder (e.g. an editing app's watch folder) so it can be
/// dragged into a timeline immediately, without an export-destination
/// dialog. Reuses the same editorial export pipeline as
/// `export_selected_asset`, just with the destination pre-resolved instead
/// of a user-picked one. Checks the project's categorized
/// `project_export_folders` first (a folder whose `role` matches the
/// asset's `media_type` exactly) — only when none matches does it fall
/// back to the two built-in slots: music goes to `export_path` (the "sound
/// folder"), everything else goes to `sfx_export_path` (the "sound effects
/// folder"). A basic project with no categorized folders configured
/// behaves exactly as before.
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
    let matching_custom_folder = catalog
        .list_project_export_folders(project_id)
        .map_err(storage_error_message)?
        .into_iter()
        .find(|folder| folder.role == asset.media_type);

    let destination_folder = if let Some(folder) = matching_custom_folder {
        folder.path
    } else if is_music {
        project.export_path.ok_or_else(|| "this project has no sound folder configured".to_string())?
    } else {
        project
            .sfx_export_path
            .ok_or_else(|| "this project has no sound effects folder configured".to_string())?
    };

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

    // The asset's category (set by Organize, or by hand in the inspector)
    // decides its export subfolder — e.g. 03_SFX/Doors/. Only without one
    // does it fall back to the old "first tag" rule, which is alphabetical
    // by facet and so effectively arbitrary once an asset has several tags.
    let asset_tags = catalog.tags_for_asset(asset_id).map_err(storage_error_message)?;
    let category_name = catalog
        .asset_categories_for_library(asset.library_id)
        .map_err(storage_error_message)?
        .remove(&asset_id)
        .map(|(_, name)| name);
    let primary_tag_name = category_name.or_else(|| asset_tags.into_iter().next().map(|tag| tag.name));
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

/// Takes tracks back out of a project — the "clean up" action in the
/// project view's bulk bar. Membership only: nothing is deleted from the
/// library, and any file already copied into the project's export folder
/// stays exactly where it is on disk (Darkwave never reaches into an
/// editor's working folder to remove media it might already be cut with).
/// Returns an undo id, same as `add_to_collection`.
#[tauri::command]
fn remove_from_collection(
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
        .remove_assets_from_collection(collection_id, &asset_ids)
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

#[derive(Debug, serde::Serialize)]
struct LicenseDateCandidateDto {
    date: String,
    keyword: String,
    context: String,
}

#[derive(Debug, serde::Serialize)]
struct LicenseDocumentAttachOutcome {
    source: SourceRecordDraft,
    candidates: Vec<LicenseDateCandidateDto>,
}

fn license_document_relative_path(asset_id: Uuid, original_filename: &str) -> String {
    format!(
        "License/{}-{}",
        asset_id,
        export_pipeline::sanitize_filename(original_filename)
    )
}

fn asset_media_root(catalog: &Catalog, asset_id: Uuid) -> Result<PathBuf, String> {
    let asset = catalog
        .get_asset(asset_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "asset not found".to_string())?;
    let library = catalog
        .get_library(asset.library_id)
        .map_err(storage_error_message)?
        .ok_or_else(|| "library not found".to_string())?;
    Ok(PathBuf::from(library.media_root))
}

/// Copies a license/usage-rights PDF the user picked into
/// `<media_root>/License/`, best-effort extracts candidate expiry dates
/// (a parse failure here — encrypted, scanned/image-only, malformed PDF —
/// does not fail the attach, it just means zero candidates), and records
/// the document's path on the asset's `source_records` row. Deliberately
/// does **not** write `license_valid_until` itself — the returned
/// candidates are suggestions for the frontend to offer, never applied
/// unseen. See `crates/license-documents` for why.
///
/// A plain `&Catalog` function (like `resolve_asset_path`) rather than
/// taking `tauri::State` directly, so it's callable from a unit test
/// without a running Tauri app.
fn attach_license_document_impl(
    catalog: &Catalog,
    asset_id: Uuid,
    source_path: &std::path::Path,
) -> Result<LicenseDocumentAttachOutcome, String> {
    let media_root = asset_media_root(catalog, asset_id)?;
    let original_filename = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| "license document has no filename".to_string())?;
    let relative_path = license_document_relative_path(asset_id, original_filename);

    let mut draft = catalog
        .get_source_record(asset_id)
        .map_err(storage_error_message)?
        .unwrap_or(SourceRecordDraft {
            asset_id,
            ..Default::default()
        });

    // Replacing an already-attached document with a differently-named
    // file (e.g. a renewed license PDF) would otherwise leave the old copy
    // orphaned under License/ forever — best-effort, since a missing old
    // file (already deleted by hand) shouldn't block attaching the new one.
    if let Some(previous_relative_path) = &draft.license_document_path {
        if previous_relative_path != &relative_path {
            let _ = std::fs::remove_file(media_root.join(previous_relative_path));
        }
    }

    let license_dir = media_root.join("License");
    std::fs::create_dir_all(&license_dir)
        .map_err(|error| format!("create License folder: {error}"))?;
    let source_bytes =
        std::fs::read(source_path).map_err(|error| format!("read license document: {error}"))?;
    let destination = media_root.join(&relative_path);
    std::fs::write(&destination, &source_bytes)
        .map_err(|error| format!("copy license document: {error}"))?;

    let candidates = license_documents::extract_text(&source_bytes)
        .ok()
        .map(|text| license_documents::find_date_candidates(&text))
        .unwrap_or_default();

    draft.license_document_path = Some(relative_path);
    catalog
        .set_source_record(draft.clone())
        .map_err(storage_error_message)?;

    Ok(LicenseDocumentAttachOutcome {
        source: draft,
        candidates: candidates
            .into_iter()
            .map(|candidate| LicenseDateCandidateDto {
                date: candidate.date.to_string(),
                keyword: candidate.keyword.to_string(),
                context: candidate.context,
            })
            .collect(),
    })
}

#[tauri::command(async)]
fn attach_license_document(
    state: tauri::State<CatalogState>,
    asset_id: String,
    source_path: String,
) -> Result<LicenseDocumentAttachOutcome, String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    attach_license_document_impl(&catalog, asset_id, std::path::Path::new(&source_path))
}

/// Best-effort deletes the attached PDF and clears its path. Leaves a
/// manually-confirmed `license_valid_until` in place (the user's own
/// judgment doesn't depend on the file still existing); clears it if it
/// was still just an unconfirmed extracted suggestion, since its only
/// source of truth is gone. `license_valid_from` is never touched here —
/// unlike `license_valid_until`, nothing in this feature ever suggests a
/// start date from the PDF, so it's always something the user typed
/// themselves and `license_expiry_source` (which only describes
/// `license_valid_until`'s provenance) has no bearing on it.
fn remove_license_document_impl(catalog: &Catalog, asset_id: Uuid) -> Result<(), String> {
    let Some(mut draft) = catalog.get_source_record(asset_id).map_err(storage_error_message)? else {
        return Ok(());
    };

    if let Some(relative_path) = draft.license_document_path.take() {
        if let Ok(media_root) = asset_media_root(catalog, asset_id) {
            let _ = std::fs::remove_file(media_root.join(relative_path));
        }
    }

    if draft.license_expiry_source.as_deref() == Some("extracted") {
        draft.license_valid_until = None;
        draft.license_expiry_source = None;
    }

    catalog.set_source_record(draft).map_err(storage_error_message)
}

#[tauri::command(async)]
fn remove_license_document(state: tauri::State<CatalogState>, asset_id: String) -> Result<(), String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    remove_license_document_impl(&catalog, asset_id)
}

/// Resolves the attached PDF's relative path to an absolute one, for the
/// frontend's "Reveal in Folder" button (mirrors `resolve_asset_path`).
fn resolve_license_document_path_impl(catalog: &Catalog, asset_id: Uuid) -> Result<Option<String>, String> {
    let Some(relative_path) = catalog
        .get_source_record(asset_id)
        .map_err(storage_error_message)?
        .and_then(|draft| draft.license_document_path)
    else {
        return Ok(None);
    };

    let media_root = asset_media_root(catalog, asset_id)?;
    Ok(Some(media_root.join(relative_path).to_string_lossy().to_string()))
}

#[tauri::command]
fn resolve_license_document_path(
    state: tauri::State<CatalogState>,
    asset_id: String,
) -> Result<Option<String>, String> {
    let asset_id = parse_uuid_field(&asset_id, "asset id")?;
    let catalog = state.0.lock().expect("catalog mutex poisoned");
    resolve_license_document_path_impl(&catalog, asset_id)
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
        // Must be the first plugin registered (see the plugin's own docs) —
        // its callback fires in the *already-running* instance whenever a
        // second launch happens (e.g. double-clicking a second `.darkwave`
        // file on Windows, where the file path arrives as a plain CLI arg
        // rather than through macOS's RunEvent::Opened), so that launch
        // gets forwarded here instead of opening a second window onto a
        // library file that's already open elsewhere.
        .plugin(tauri_plugin_single_instance::init(|app, argv, _cwd| {
            if let Some(path) = darkwave_library_path_from_args(&argv) {
                open_library_file_and_notify_frontend(app, &path);
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.unminimize();
                let _ = window.set_focus();
            }
        }))
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_opener::init());

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

            // Bootstrap order: (1) a `.darkwave` file passed on the command
            // line — how Windows hands us the path when the app is launched
            // fresh by double-clicking a file (macOS delivers that as
            // RunEvent::Opened instead, handled separately below, but this
            // also harmlessly covers a `darkwave-desktop /path/to/x.darkwave`
            // launch on any platform); (2) reopen the last library file the
            // user had open, so relaunching feels like reopening a document
            // in After Effects rather than starting over; (3) fall back to
            // the legacy shared catalog.sqlite for anyone who hasn't
            // migrated to the library-file model yet — the frontend detects
            // this case via `active_library_file_path` returning `None` and
            // shows the migration screen instead of the New Setup/Open
            // Library first-run screen; (4) a fresh install with none of the
            // above gets an empty in-memory placeholder, replaced the moment
            // the first-run screen calls
            // `create_library_file`/`open_library_file`.
            let legacy_catalog_path = app_data_dir.join("catalog.sqlite");
            let preferences_file_path = app_data_dir.join("preferences.json");
            let startup_preferences = preferences::load_preferences(&preferences_file_path)
                .unwrap_or_else(|_| preferences::AppPreferences::default_for_editorial_audio());

            let cli_args: Vec<String> = std::env::args().collect();
            let candidate_library_file_path = darkwave_library_path_from_args(&cli_args)
                .filter(|path| std::path::Path::new(path).is_file())
                .or_else(|| {
                    startup_preferences
                        .last_active_library_file_path
                        .clone()
                        .filter(|path| std::path::Path::new(path).is_file())
                });

            let (catalog, active_library_file_path) = if let Some(library_file_path) =
                candidate_library_file_path
            {
                match Catalog::open(&library_file_path) {
                    Ok(catalog) => (catalog, Some(library_file_path)),
                    Err(error) => {
                        log_startup_event(&format!(
                            "could not reopen last library file {library_file_path}: {error} — falling back"
                        ));
                        match Catalog::open(&legacy_catalog_path) {
                            Ok(catalog) => (catalog, None),
                            Err(error) => {
                                report_fatal_setup_error(
                                    app.handle(),
                                    "open the local catalog database",
                                    error,
                                );
                                return Ok(());
                            }
                        }
                    }
                }
            } else if legacy_catalog_path.exists() {
                match Catalog::open(&legacy_catalog_path) {
                    Ok(catalog) => (catalog, None),
                    Err(error) => {
                        report_fatal_setup_error(app.handle(), "open the local catalog database", error);
                        return Ok(());
                    }
                }
            } else {
                match Catalog::open(":memory:") {
                    Ok(catalog) => (catalog, None),
                    Err(error) => {
                        report_fatal_setup_error(app.handle(), "open the local catalog database", error);
                        return Ok(());
                    }
                }
            };
            log_startup_event("catalog opened");
            app.manage(CatalogState(Mutex::new(catalog)));
            app.manage(ActiveLibraryFileState(Mutex::new(active_library_file_path)));
            app.manage(JobControlState {
                audio_analysis_paused: std::sync::atomic::AtomicBool::new(false),
                waveform_generation_paused: std::sync::atomic::AtomicBool::new(false),
            });
            app.manage(power::PowerAssertionState::new());
            app.manage(InstrumentModelState {
                model: std::sync::Arc::new(Mutex::new(None)),
                tried: std::sync::atomic::AtomicBool::new(false),
            });
            #[cfg(all(target_os = "macos", not(feature = "direct-dist")))]
            app.manage(BookmarkAccessState(Mutex::new(std::collections::HashMap::new())));

            // Standing background worker: requeues jobs that failed with
            // retries left, polls every folder configured on the currently
            // open library (see `folders`/`add_folder`), then tells the
            // frontend to drain whatever's pending. This is what actually
            // fixes jobs only ever processing right after Import/Refresh —
            // everywhere, not just those two triggers, and it's what makes
            // watched-folder import a live feature instead of
            // tested-but-never-invoked library code. A plain thread +
            // sleep, not async/tokio: each tick's own work (a few SQL
            // statements, a handful of directory reads) is fast and
            // synchronous, so there's nothing here that benefits from an
            // async runtime.
            let worker_app_handle = app.handle().clone();
            std::thread::spawn(move || {
                use tauri::Emitter;
                // Keyed by folder path so each folder's own file-size
                // stability state persists across ticks, same as
                // import_pipeline's own single-folder design always
                // intended. Rebuilt from the open library's `folders` rows
                // every tick (one query, no filesystem work of its own) —
                // adding, removing, or re-tagging a folder in Settings or
                // during New Setup takes effect on the very next tick, no
                // restart and no separate invalidation signal needed. A
                // `kind = "documents"` folder never gets an entry here —
                // it's scanned separately below, since it's plain
                // inventory, not an audio-import source.
                let mut watched_pollers: std::collections::HashMap<
                    PathBuf,
                    (Uuid, Option<String>, import_pipeline::WatchedFolderPoller),
                > = std::collections::HashMap::new();
                // Mirrors watched_pollers, but for "documents" folders:
                // paths already recorded (or already known to fail — see
                // below), so an unchanged folder costs zero catalog work
                // on repeat ticks instead of re-running an INSERT OR
                // IGNORE per file every 20s forever.
                let mut documents_seen: std::collections::HashMap<Uuid, std::collections::HashSet<PathBuf>> =
                    std::collections::HashMap::new();

                loop {
                    std::thread::sleep(std::time::Duration::from_secs(20));

                    if let Some(state) = worker_app_handle.try_state::<CatalogState>() {
                        // Brief lock: reading which library/folders are
                        // currently configured. Every filesystem poll,
                        // directory listing, and file hash below runs with
                        // the lock released — a slow or unresponsive folder
                        // (a stalled NAS/SMB mount, a documents folder with
                        // hundreds of files) can then only block this worker
                        // thread, never every foreground command that shares
                        // this same mutex (add_folder, list_libraries, any
                        // asset/tag query, export_asset_to_project, etc.).
                        // The catalog is re-locked only briefly, per file
                        // actually committed — the same hash-outside/
                        // commit-inside split `run_batch_import` already
                        // uses.
                        //
                        // Deliberately no automatic `requeue_failed_jobs`
                        // here: a job that reaches 'failed' either exhausted
                        // `defer_unavailable_job`'s own silent-retry budget
                        // (a real NAS/availability problem, already retried
                        // for ~an hour) or hit a genuine processing error
                        // (e.g. an unsupported codec, which will never
                        // succeed no matter how many times it's requeued —
                        // an unsupported-codec asset burned real retry
                        // cycles this way before this was removed). Retrying
                        // a 'failed' job is now exclusively user-initiated
                        // via `retry_failed_jobs` (the Background Activity
                        // panel's "Retry Now"), so a deterministic failure
                        // doesn't loop invisibly and a fix a user actually
                        // asked for is the only thing that reclaims it.
                        let library_and_folders = {
                            let catalog = state.0.lock().expect("catalog mutex poisoned");

                            // One project file = one library (see the
                            // project-file model) — the worker always
                            // watches whichever library the currently open
                            // catalog holds, never a preferences-level
                            // global setting.
                            catalog
                                .list_libraries()
                                .ok()
                                .and_then(|libraries| libraries.into_iter().next())
                                .map(|library| {
                                    let folders = catalog.list_folders(library.id).unwrap_or_default();
                                    (library.id, folders)
                                })
                        };

                        if let Some((library_id, folders)) = library_and_folders {
                            let audio_folders: std::collections::HashMap<
                                PathBuf,
                                (Uuid, Option<String>),
                            > = folders
                                .iter()
                                .filter(|folder| folder.kind != "documents")
                                .map(|folder| {
                                    (
                                        PathBuf::from(&folder.path),
                                        (folder.id, folder.role.clone()),
                                    )
                                })
                                .collect();

                            // Drop pollers for folders that were removed
                            // (or renamed away) since the last tick.
                            watched_pollers.retain(|path, _| audio_folders.contains_key(path));
                            documents_seen.retain(|folder_id, _| {
                                folders
                                    .iter()
                                    .any(|folder| folder.kind == "documents" && folder.id == *folder_id)
                            });

                            for (path, (folder_id, role)) in &audio_folders {
                                let entry = watched_pollers.entry(path.clone()).or_insert_with(|| {
                                    (
                                        *folder_id,
                                        role.clone(),
                                        import_pipeline::WatchedFolderPoller::new(path.clone()),
                                    )
                                });
                                // A role edited in Settings since the last
                                // tick should apply to the very next file
                                // this folder yields.
                                entry.1 = role.clone();

                                // Filesystem-only (fs::read_dir + per-file
                                // metadata) — no catalog access, so no lock
                                // needed for this step.
                                let candidates = entry.2.poll().unwrap_or_default();
                                for candidate in candidates {
                                    // The slow, disk-bound part (streamed
                                    // SHA-256 + metadata/tag extraction) —
                                    // runs fully unlocked, same as
                                    // run_batch_import's own prepare stage.
                                    let prepared = import_pipeline::prepare_import(
                                        &candidate.path,
                                        import_pipeline::ImportMode::Referenced,
                                        entry.1.as_deref(),
                                    );
                                    if let Ok(prepared) = prepared {
                                        let catalog = state.0.lock().expect("catalog mutex poisoned");
                                        let _ = import_pipeline::commit_prepared_import(
                                            &catalog, library_id, prepared, None,
                                        );
                                    }
                                }
                            }

                            // Documents folders (e.g. a Licence/PDF
                            // folder): plain inventory. Listing the
                            // directory happens unlocked; only the actual
                            // insert (skipped entirely once a path is in
                            // documents_seen) takes a brief lock, one file
                            // at a time.
                            for folder in folders.iter().filter(|folder| folder.kind == "documents") {
                                let Ok(entries) = std::fs::read_dir(&folder.path) else {
                                    continue;
                                };
                                let seen = documents_seen.entry(folder.id).or_default();
                                for entry in entries.flatten() {
                                    let is_file = entry
                                        .metadata()
                                        .map(|metadata| metadata.is_file())
                                        .unwrap_or(false);
                                    if !is_file {
                                        continue;
                                    }
                                    let path = entry.path();
                                    if seen.contains(&path) {
                                        continue;
                                    }
                                    let catalog = state.0.lock().expect("catalog mutex poisoned");
                                    let recorded = catalog
                                        .record_folder_document(
                                            folder.id,
                                            library_id,
                                            path.to_string_lossy().as_ref(),
                                        )
                                        .unwrap_or(false);
                                    if recorded {
                                        seen.insert(path);
                                    }
                                }
                            }
                        }
                    }

                    let _ = worker_app_handle.emit("background-tick", ());
                }
            });

            // Autonomous overnight drive loop: unlike the worker thread
            // above (which only nudges the frontend via background-tick),
            // this tokio task calls the same job-kind processors the
            // frontend calls via invoke() directly, so a queued batch keeps
            // draining even if the webview is backgrounded or throttled by
            // the OS. It also holds a power assertion (see power.rs) for as
            // long as there's real *unpaused* work outstanding, which is
            // what lets a large overnight batch actually finish instead of
            // being cut short by idle system sleep — released the moment
            // the queue empties or the user pauses, so it never holds the
            // machine awake for no reason.
            let drive_app_handle = app.handle().clone();
            tauri::async_runtime::spawn(async move {
                use std::sync::atomic::Ordering;

                loop {
                    let _ = process_pending_jobs(drive_app_handle.state::<CatalogState>());

                    let waveform_processed = process_waveform_jobs(
                        drive_app_handle.clone(),
                        drive_app_handle.state::<CatalogState>(),
                        drive_app_handle.state::<JobControlState>(),
                        drive_app_handle.state::<ActiveLibraryFileState>(),
                    )
                    .await
                    .unwrap_or(0);

                    let instrument_processed = process_instrument_jobs(
                        drive_app_handle.clone(),
                        drive_app_handle.state::<CatalogState>(),
                        drive_app_handle.state::<InstrumentModelState>(),
                        drive_app_handle.state::<ActiveLibraryFileState>(),
                    )
                    .await
                    .unwrap_or(0);

                    let analysis_processed = process_audio_analysis_jobs(
                        drive_app_handle.clone(),
                        drive_app_handle.state::<CatalogState>(),
                        drive_app_handle.state::<JobControlState>(),
                        drive_app_handle.state::<ActiveLibraryFileState>(),
                    )
                    .await
                    .unwrap_or(0);

                    // Paused kinds are deliberately excluded here (not just
                    // skipped above) — a paused queue still has pending
                    // rows, but nothing is actually going to touch them
                    // until the user unpauses, so it shouldn't keep the
                    // machine awake either. Same reasoning for instrument
                    // detection with no model installed: process_instrument_jobs
                    // claims nothing and those jobs sit 'pending' forever
                    // (see instrument_detection_available's doc comment,
                    // which the frontend already checks for exactly this
                    // reason) — without this guard, any library with
                    // instrument-detection jobs queued (every import queues
                    // one per asset, model or no model) would hold the sleep-
                    // prevention power assertion engaged and this loop
                    // pinned at its busy 1s cadence forever, even though
                    // nothing is actually happening.
                    let instrument_model_available = drive_app_handle
                        .state::<InstrumentModelState>()
                        .model
                        .lock()
                        .expect("instrument model mutex poisoned")
                        .is_some();
                    let unpaused_pending = {
                        let job_control = drive_app_handle.state::<JobControlState>();
                        let catalog_state = drive_app_handle.state::<CatalogState>();
                        let catalog = catalog_state.0.lock().expect("catalog mutex poisoned");
                        let mut total = catalog.pending_job_count(JobKind::MetadataExtraction).unwrap_or(0);
                        if instrument_model_available {
                            total += catalog
                                .pending_job_count(JobKind::InstrumentDetection)
                                .unwrap_or(0);
                        }
                        if !job_control.audio_analysis_paused.load(Ordering::SeqCst) {
                            total += catalog.pending_job_count(JobKind::AudioAnalysis).unwrap_or(0);
                        }
                        if !job_control.waveform_generation_paused.load(Ordering::SeqCst) {
                            total += catalog
                                .pending_job_count(JobKind::WaveformGeneration)
                                .unwrap_or(0);
                        }
                        total
                    };

                    let sleep_prevention_enabled = preferences_path(&drive_app_handle)
                        .ok()
                        .and_then(|path| preferences::load_preferences(path).ok())
                        .map(|preferences| preferences.prevent_sleep_during_analysis)
                        .unwrap_or(true);

                    let power_state = drive_app_handle.state::<power::PowerAssertionState>();
                    if unpaused_pending > 0 && sleep_prevention_enabled {
                        power_state.engage();
                    } else {
                        power_state.release();
                    }

                    let idle = unpaused_pending == 0
                        && waveform_processed == 0
                        && instrument_processed == 0
                        && analysis_processed == 0;
                    tokio::time::sleep(std::time::Duration::from_secs(if idle { 5 } else { 1 }))
                        .await;
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
            let find_item =
                tauri::menu::MenuItem::with_id(app, "find", "Find", true, Some("CmdOrCtrl+F"))?;
            // Same accelerator as the in-app keyboard shortcut
            // (ShortcutMap::default_audio_workspace's Mod+K) so the menu
            // item's hint always matches what actually happens — both paths
            // just toggle the same palette.
            let command_palette_item = tauri::menu::MenuItem::with_id(
                app,
                "command-palette",
                "Command Palette",
                true,
                Some("CmdOrCtrl+K"),
            )?;
            let edit_menu = tauri::menu::SubmenuBuilder::new(app, "Edit")
                .item(&undo_item)
                .item(&redo_item)
                .separator()
                .cut()
                .copy()
                .paste()
                .select_all()
                .separator()
                .item(&find_item)
                .item(&command_palette_item)
                .build()?;

            let preferences_item = tauri::menu::MenuItem::with_id(
                app,
                "preferences",
                "Preferences…",
                true,
                Some("CmdOrCtrl+,"),
            )?;
            let app_menu = tauri::menu::SubmenuBuilder::new(app, "Darkwave")
                .about(None)
                .separator()
                .item(&preferences_item)
                .separator()
                .hide()
                .hide_others()
                .show_all()
                .separator()
                .quit()
                .build()?;

            let new_library_item = tauri::menu::MenuItem::with_id(
                app,
                "new-library",
                "New Library…",
                true,
                Some("CmdOrCtrl+N"),
            )?;
            let open_library_item = tauri::menu::MenuItem::with_id(
                app,
                "open-library",
                "Open Library…",
                true,
                Some("CmdOrCtrl+O"),
            )?;
            // Populated below via refresh_open_recent_menu once its handle
            // is registered in app state — every recent library, not just a
            // single fixed "second most recent" entry (the old "Last Open"
            // item this replaces).
            let open_recent_submenu = tauri::menu::SubmenuBuilder::new(app, "Open Recent").build()?;
            let import_item = tauri::menu::MenuItem::with_id(
                app,
                "import-folder",
                "Import Folder…",
                true,
                Some("CmdOrCtrl+I"),
            )?;
            let export_selected_item = tauri::menu::MenuItem::with_id(
                app,
                "export-selected",
                "Export Selected…",
                true,
                Some("CmdOrCtrl+E"),
            )?;
            let license_report_item = tauri::menu::MenuItem::with_id(
                app,
                "export-license-report",
                "Export License Report…",
                true,
                None::<&str>,
            )?;
            // Deliberately not CmdOrCtrl+R — the in-app ShortcutMap already
            // binds that to Toggle Reviewed, and a native menu accelerator
            // would win the keystroke before the webview ever saw it.
            let refresh_library_item = tauri::menu::MenuItem::with_id(
                app,
                "refresh-library",
                "Refresh Library",
                true,
                Some("CmdOrCtrl+Shift+R"),
            )?;
            let backup_library_item = tauri::menu::MenuItem::with_id(
                app,
                "backup-library",
                "Backup Library…",
                true,
                None::<&str>,
            )?;
            let restore_library_item = tauri::menu::MenuItem::with_id(
                app,
                "restore-library",
                "Restore Library from Backup…",
                true,
                None::<&str>,
            )?;
            let file_menu = tauri::menu::SubmenuBuilder::new(app, "File")
                .item(&new_library_item)
                .item(&open_library_item)
                .item(&open_recent_submenu)
                .separator()
                .item(&import_item)
                .item(&export_selected_item)
                .item(&license_report_item)
                .separator()
                .item(&refresh_library_item)
                .separator()
                .item(&backup_library_item)
                .item(&restore_library_item)
                .build()?;

            let toggle_sidebar_item = tauri::menu::MenuItem::with_id(
                app,
                "toggle-sidebar",
                "Toggle Sidebar",
                true,
                None::<&str>,
            )?;
            let toggle_inspector_item = tauri::menu::MenuItem::with_id(
                app,
                "toggle-inspector",
                "Toggle Inspector",
                true,
                None::<&str>,
            )?;
            let toggle_editor_workflow_item = tauri::menu::MenuItem::with_id(
                app,
                "toggle-editor-workflow",
                "Editor Workflow",
                true,
                None::<&str>,
            )?;
            let view_menu = tauri::menu::SubmenuBuilder::new(app, "View")
                .item(&toggle_sidebar_item)
                .item(&toggle_inspector_item)
                .separator()
                .item(&toggle_editor_workflow_item)
                .separator()
                .fullscreen()
                .build()?;

            let new_project_item = tauri::menu::MenuItem::with_id(
                app,
                "new-project",
                "New Project…",
                true,
                Some("CmdOrCtrl+Shift+N"),
            )?;
            let sonic_radar_resync_item = tauri::menu::MenuItem::with_id(
                app,
                "sonic-radar-resync-all",
                "Re-analyse Everything",
                true,
                None::<&str>,
            )?;
            let sonic_radar_backfill_item = tauri::menu::MenuItem::with_id(
                app,
                "sonic-radar-backfill",
                "Backfill Tempo/Key/Pitch/Vocals",
                true,
                None::<&str>,
            )?;
            let sonic_radar_menu = tauri::menu::SubmenuBuilder::new(app, "Sonic Radar")
                .item(&sonic_radar_resync_item)
                .item(&sonic_radar_backfill_item)
                .build()?;
            let check_integrity_item = tauri::menu::MenuItem::with_id(
                app,
                "check-library-integrity",
                "Check and Repair…",
                true,
                None::<&str>,
            )?;
            let optimize_item = tauri::menu::MenuItem::with_id(
                app,
                "optimize-library",
                "Optimize…",
                true,
                None::<&str>,
            )?;
            // No accelerator, deliberately: handleEmptyLibraryTrash bypasses
            // the normal trash retention wait entirely, so this shouldn't be
            // one accidental keystroke away — click + its own confirm
            // dialog only.
            let empty_trash_item = tauri::menu::MenuItem::with_id(
                app,
                "empty-trash",
                "Empty Trash…",
                true,
                None::<&str>,
            )?;
            let library_menu = tauri::menu::SubmenuBuilder::new(app, "Library")
                .item(&new_project_item)
                .separator()
                .item(&sonic_radar_menu)
                .separator()
                .item(&check_integrity_item)
                .item(&optimize_item)
                .separator()
                .item(&empty_trash_item)
                .build()?;

            let zoom_item = tauri::menu::PredefinedMenuItem::maximize(app, Some("Zoom"))?;
            let bring_all_to_front_item =
                tauri::menu::PredefinedMenuItem::bring_all_to_front(app, Some("Bring All to Front"))?;
            let window_menu = tauri::menu::SubmenuBuilder::new(app, "Window")
                .minimize()
                .close_window()
                .separator()
                .item(&zoom_item)
                .separator()
                .item(&bring_all_to_front_item)
                .build()?;

            let shortcuts_item = tauri::menu::MenuItem::with_id(
                app,
                "keyboard-shortcuts",
                "Keyboard Shortcuts",
                true,
                Some("CmdOrCtrl+/"),
            )?;
            let release_readiness_item = tauri::menu::MenuItem::with_id(
                app,
                "release-readiness",
                "Release Readiness",
                true,
                None::<&str>,
            )?;
            let open_source_licenses_item = tauri::menu::MenuItem::with_id(
                app,
                "open-source-licenses",
                "Open Source Licenses",
                true,
                None::<&str>,
            )?;
            let help_menu = tauri::menu::SubmenuBuilder::new(app, "Help")
                .item(&shortcuts_item)
                .item(&release_readiness_item)
                .item(&open_source_licenses_item)
                .build()?;

            let menu = tauri::menu::MenuBuilder::new(app)
                .item(&app_menu)
                .item(&file_menu)
                .item(&edit_menu)
                .item(&view_menu)
                .item(&library_menu)
                .item(&window_menu)
                .item(&help_menu)
                .build()?;
            app.set_menu(menu)?;

            app.manage(OpenRecentMenuState(open_recent_submenu));
            refresh_open_recent_menu(app.handle(), &startup_preferences.recent_library_files);

            Ok(())
        })
        .on_menu_event(|app, event| {
            use tauri::Emitter;
            let id = event.id().as_ref();
            if let Some(path) = id.strip_prefix("open-recent:") {
                open_library_file_and_notify_frontend(app, path);
                return;
            }
            match id {
                "undo" => {
                    let _ = app.emit("menu-undo", ());
                }
                "redo" => {
                    let _ = app.emit("menu-redo", ());
                }
                "new-library" => {
                    let _ = app.emit("menu-new-library", ());
                }
                "open-library" => {
                    let _ = app.emit("menu-open-library", ());
                }
                "import-folder" => {
                    let _ = app.emit("menu-import-folder", ());
                }
                "export-selected" => {
                    let _ = app.emit("menu-export-selected", ());
                }
                "export-license-report" => {
                    let _ = app.emit("menu-export-license-report", ());
                }
                "refresh-library" => {
                    let _ = app.emit("menu-refresh-library", ());
                }
                "backup-library" => {
                    let _ = app.emit("menu-backup-library", ());
                }
                "restore-library" => {
                    let _ = app.emit("menu-restore-library", ());
                }
                "preferences" => {
                    let _ = app.emit("menu-preferences", ());
                }
                "find" => {
                    let _ = app.emit("menu-find", ());
                }
                "command-palette" => {
                    let _ = app.emit("menu-command-palette", ());
                }
                "toggle-sidebar" => {
                    let _ = app.emit("menu-toggle-sidebar", ());
                }
                "toggle-inspector" => {
                    let _ = app.emit("menu-toggle-inspector", ());
                }
                "toggle-editor-workflow" => {
                    let _ = app.emit("menu-toggle-editor-workflow", ());
                }
                "new-project" => {
                    let _ = app.emit("menu-new-project", ());
                }
                "sonic-radar-resync-all" => {
                    let _ = app.emit("menu-sonic-radar-resync-all", ());
                }
                "sonic-radar-backfill" => {
                    let _ = app.emit("menu-sonic-radar-backfill", ());
                }
                "check-library-integrity" => {
                    let _ = app.emit("menu-check-library-integrity", ());
                }
                "optimize-library" => {
                    let _ = app.emit("menu-optimize-library", ());
                }
                "empty-trash" => {
                    let _ = app.emit("menu-empty-trash", ());
                }
                "release-readiness" => {
                    let _ = app.emit("menu-release-readiness", ());
                }
                "keyboard-shortcuts" => {
                    let _ = app.emit("menu-keyboard-shortcuts", ());
                }
                "open-source-licenses" => {
                    use tauri_plugin_opener::OpenerExt;
                    match third_party_notices_path(app) {
                        Ok(path) => {
                            if let Err(error) = app.opener().open_path(path.to_string_lossy(), None::<&str>) {
                                eprintln!("open-source-licenses: failed to open {path:?}: {error:?}");
                            }
                        }
                        Err(error) => eprintln!("open-source-licenses: {error}"),
                    }
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
                // Don't leave an engaged IOPMAssertion (macOS) or
                // ES_SYSTEM_REQUIRED assertion (Windows) behind once the app
                // is quitting — see power.rs.
                if let Some(power_state) = window.app_handle().try_state::<power::PowerAssertionState>() {
                    power_state.release();
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
            set_library_media_root,
            set_library_import_root,
            set_library_import_subfolders,
            add_folder,
            list_folders,
            remove_folder,
            set_folder_role,
            active_library_file_path,
            list_recent_library_files,
            is_path_network_tolerant,
            create_library_file,
            open_library_file,
            migrate_legacy_library,
            finish_legacy_migration,
            import_dropped_paths,
            detect_stem_groups,
            stem_group_members,
            purge_library_cache,
            check_library_integrity,
            optimize_library,
            clear_deleted_import_history,
            empty_library_trash,
            scan_junk_files,
            clean_up_junk_files,
            propose_sfx_organization,
            apply_sfx_organization,
            rename_sfx_category,
            list_sfx_categories,
            asset_categories,
            delete_library,
            list_assets,
            list_recent_assets,
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
            rename_project,
            add_project_export_folder,
            list_project_export_folders,
            remove_project_export_folder,
            set_project_export_folder_role,
            export_asset_to_project,
            get_project_resolve_sync_status,
            get_resolve_sync_statuses_for_library,
            approve_project_resolve_sync,
            send_asset_to_resolve_timeline,
            add_to_collection,
            remove_from_collection,
            assets_in_collection,
            search_assets_advanced,
            create_smart_collection,
            assets_in_smart_collection,
            get_source_record,
            set_source_record,
            attach_license_document,
            remove_license_document,
            resolve_license_document_path,
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
            process_waveform_jobs,
            process_instrument_jobs,
            instrument_detection_available,
            resync_analysis,
            asset_job_state,
            instruments_for_library,
            assets_by_instruments,
            asset_instruments,
            set_audio_analysis_paused,
            audio_analysis_paused,
            set_waveform_generation_paused,
            waveform_generation_paused,
            job_status,
            retry_failed_jobs,
            failed_job_assets,
            similar_assets,
            get_waveform,
            store_waveform_peaks,
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
        .build(tauri::generate_context!())
        .expect("failed to build Darkwave desktop shell")
        .run(|app_handle, event| {
            // macOS's document-open mechanism (Finder double-click, or the
            // Dock icon's Open Recent menu) — delivered here whether this
            // is a fresh launch or the app was already running, unlike the
            // single-instance plugin's callback above, which only fires for
            // a *second* launch attempt (mainly the Windows path).
            //
            // `RunEvent::Opened` only exists on macOS/iOS/Android (see tauri's
            // `app.rs`) — Windows has no such variant, so this arm has to be
            // compiled out there entirely rather than just never matching.
            #[cfg(any(target_os = "macos", target_os = "ios", target_os = "android"))]
            if let tauri::RunEvent::Opened { urls } = event {
                for url in urls {
                    if let Ok(path) = url.to_file_path() {
                        open_library_file_and_notify_frontend(app_handle, &path.to_string_lossy());
                    }
                }
            }
            #[cfg(not(any(target_os = "macos", target_os = "ios", target_os = "android")))]
            let _ = (app_handle, event);
        });
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
    fn darkwave_library_path_from_args_finds_a_darkwave_file_case_insensitively() {
        let args = vec![
            "darkwave-desktop.exe".to_string(),
            "C:\\Users\\Name\\My Library.DARKWAVE".to_string(),
        ];
        assert_eq!(
            super::darkwave_library_path_from_args(&args),
            Some("C:\\Users\\Name\\My Library.DARKWAVE".to_string())
        );
    }

    #[test]
    fn darkwave_library_path_from_args_ignores_a_bak_file_and_argv_zero() {
        let args = vec![
            "/Volumes/x/My Library.darkwave".to_string(), // argv[0], must be skipped
            "/Volumes/x/My Library.darkwavebak".to_string(),
        ];
        assert_eq!(super::darkwave_library_path_from_args(&args), None);
    }

    #[test]
    fn darkwave_library_path_from_args_returns_none_with_no_matching_arg() {
        assert_eq!(
            super::darkwave_library_path_from_args(&["darkwave-desktop".to_string()]),
            None
        );
    }

    #[test]
    fn folder_role_for_path_matches_a_registered_folder_and_ignores_others() {
        let catalog = storage::Catalog::open(":memory:").expect("open catalog");
        let library = catalog.create_library("Home Studio", "/sounds").expect("library");
        catalog
            .add_folder(library.id, "/sounds/SFX", Some("sound_effect"), "watched")
            .expect("add folder");

        assert_eq!(
            super::folder_role_for_path(&catalog, library.id, "/sounds/SFX"),
            Some("sound_effect".to_string())
        );
        assert_eq!(
            super::folder_role_for_path(&catalog, library.id, "/sounds/Unregistered"),
            None
        );
    }

    #[test]
    fn resolve_sync_roles_combines_export_folders_with_the_two_built_in_slots() {
        let catalog = storage::Catalog::open(":memory:").expect("open catalog");
        let library = catalog.create_library("Home Studio", "/sounds").expect("library");
        let project = catalog
            .create_collection(library.id, "Trailer", storage::CollectionType::Project)
            .expect("project");
        catalog
            .set_collection_export_path(project.id, Some("/Edit/Trailer/Sound"))
            .expect("set export path");
        catalog
            .add_project_export_folder(project.id, "/Edit/Trailer/Foley", "foley")
            .expect("add foley folder");
        // A `documents` folder never becomes a Resolve bin, same as it's
        // excluded from audio export routing everywhere else.
        catalog
            .add_project_export_folder(project.id, "/Edit/Trailer/Docs", "documents")
            .expect("add documents folder");

        let project = catalog
            .get_collection(project.id)
            .expect("get project")
            .expect("project exists");
        let roles = super::resolve_sync_roles_for_project(&catalog, &project);

        // music: from export_path. foley: from the categorized folder.
        // sound_effect: absent (sfx_export_path was never set). documents:
        // never included.
        assert_eq!(roles, vec!["foley".to_string(), "music".to_string()]);
    }

    #[test]
    fn resolve_sync_roles_is_empty_for_a_project_with_no_export_configuration() {
        let catalog = storage::Catalog::open(":memory:").expect("open catalog");
        let library = catalog.create_library("Home Studio", "/sounds").expect("library");
        let project = catalog
            .create_collection(library.id, "Untouched", storage::CollectionType::Project)
            .expect("project");

        let roles = super::resolve_sync_roles_for_project(&catalog, &project);
        assert!(roles.is_empty());
    }

    #[test]
    fn sync_media_root_folder_registers_a_folder_when_none_exists() {
        let catalog = storage::Catalog::open(":memory:").expect("open catalog");
        let library = catalog.create_library("Home Studio", "").expect("library");

        super::sync_media_root_folder(&catalog, library.id, "/sounds");

        let folders = catalog.list_folders(library.id).expect("list folders");
        assert_eq!(folders.len(), 1);
        assert_eq!(folders[0].path, "/sounds");
        assert_eq!(folders[0].kind, "media_root");
        assert_eq!(folders[0].role, None);
    }

    #[test]
    fn sync_media_root_folder_replaces_a_stale_path_without_duplicating() {
        let catalog = storage::Catalog::open(":memory:").expect("open catalog");
        let library = catalog.create_library("Home Studio", "/old/mount").expect("library");
        super::sync_media_root_folder(&catalog, library.id, "/old/mount");

        // Simulates the MAS bookmark self-heal: the same logical folder
        // remounts under a different /Volumes/... path.
        super::sync_media_root_folder(&catalog, library.id, "/new/mount");

        let folders = catalog.list_folders(library.id).expect("list folders");
        assert_eq!(folders.len(), 1, "must replace, not duplicate, the media_root folder");
        assert_eq!(folders[0].path, "/new/mount");
    }

    #[test]
    fn sync_media_root_folder_is_a_no_op_for_an_empty_path() {
        let catalog = storage::Catalog::open(":memory:").expect("open catalog");
        let library = catalog.create_library("Home Studio", "").expect("library");

        super::sync_media_root_folder(&catalog, library.id, "   ");

        assert_eq!(catalog.list_folders(library.id).expect("list folders").len(), 0);
    }

    #[test]
    fn sync_media_root_folder_does_not_disturb_other_folders() {
        let catalog = storage::Catalog::open(":memory:").expect("open catalog");
        let library = catalog.create_library("Home Studio", "/sounds").expect("library");
        catalog
            .add_folder(library.id, "/sounds/SFX", Some("sound_effect"), "watched")
            .expect("add watched folder");

        super::sync_media_root_folder(&catalog, library.id, "/sounds");
        super::sync_media_root_folder(&catalog, library.id, "/sounds/relocated");

        let folders = catalog.list_folders(library.id).expect("list folders");
        assert_eq!(folders.len(), 2);
        assert!(folders.iter().any(|folder| folder.path == "/sounds/SFX" && folder.kind == "watched"));
        assert!(folders.iter().any(|folder| folder.path == "/sounds/relocated" && folder.kind == "media_root"));
    }

    #[test]
    fn parse_job_kind_field_round_trips_waveform_generation() {
        assert_eq!(
            super::parse_job_kind_field("waveform_generation"),
            Ok(storage::JobKind::WaveformGeneration)
        );
    }

    #[test]
    fn build_waveform_payload_produces_a_bounded_strip_and_serializes() {
        let buffer = audio_metadata::DecodedAudioBuffer {
            sample_rate: 48_000,
            channels: 2,
            samples: (0..20_000)
                .map(|i| if i % 2 == 0 { -0.8 } else { 0.6 })
                .collect(),
        };

        let payload = super::build_waveform_payload(&buffer);
        assert_eq!(payload.sample_rate, 48_000);
        assert_eq!(payload.peaks.len(), waveform::TRANSPORT_STRIP_BUCKETS);
        assert!(payload.peaks.iter().all(|value| (0.0..=1.0).contains(value)));

        let json = serde_json::to_string(&payload).expect("serialize");
        let restored: super::StoredWaveform = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(restored.peaks, payload.peaks);
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

    fn organize_fixture(name: &str) -> (std::path::PathBuf, storage::Catalog, uuid::Uuid) {
        let root = std::env::temp_dir().join(format!("darkwave-organize-{name}-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(root.join("_SoundFX")).expect("dir");
        let catalog = storage::Catalog::open(root.join("lib.darkwave")).expect("catalog");
        let library = catalog.create_library("Org", root.to_string_lossy()).expect("library");
        (root, catalog, library.id)
    }

    fn referenced_asset(catalog: &storage::Catalog, library_id: uuid::Uuid, path: &std::path::Path, hash: &str) -> storage::AssetRecord {
        std::fs::write(path, hash.as_bytes()).expect("write");
        match catalog
            .register_asset(storage::NewAssetRecord {
                library_id,
                original_filename: path.file_name().unwrap().to_string_lossy().to_string(),
                display_name: "x".into(),
                path: storage::AssetPath::Referenced(path.to_string_lossy().to_string()),
                storage_mode: shared_types::StorageMode::Referenced,
                content_hash: Some(hash.into()),
                media_type: "sound_effect".into(),
                file_size: hash.len() as u64,
                availability_state: shared_types::AvailabilityState::Local,
            })
            .expect("register")
        {
            storage::RegisterAssetOutcome::Created(asset) => asset,
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn organize_moves_files_into_category_folders_and_undo_puts_them_back() {
        let (root, catalog, library_id) = organize_fixture("move");
        let sfx = root.join("_SoundFX");
        let door = referenced_asset(&catalog, library_id, &sfx.join("door-closing.wav"), "door");
        let slam = referenced_asset(&catalog, library_id, &sfx.join("doorslam.wav"), "slam");
        std::fs::write(sfx.join("._doorslam.wav"), b"sidecar").expect("sidecar");
        // A file already sitting where one would land must never be overwritten.
        std::fs::create_dir_all(sfx.join("Doors")).expect("pre-existing");
        std::fs::write(sfx.join("Doors").join("doorslam.wav"), b"someone else's").expect("occupant");

        let result = super::organize_assets(
            &catalog,
            &[(door.id, Some("Doors".into())), (slam.id, Some("Doors".into()))],
            true,
        )
        .expect("organize");
        assert_eq!(result.moved, 2);
        assert_eq!(result.folders_created, 0, "Doors already existed");
        assert!(sfx.join("Doors/door-closing.wav").is_file());
        assert!(sfx.join("Doors/doorslam (2).wav").is_file());
        assert_eq!(std::fs::read(sfx.join("Doors/doorslam.wav")).unwrap(), b"someone else's");
        assert!(sfx.join("Doors/._doorslam (2).wav").is_file(), "sidecar travels with its file");
        let moved = catalog.get_asset(door.id).unwrap().unwrap();
        assert_eq!(moved.path, storage::AssetPath::Referenced(sfx.join("Doors/door-closing.wav").to_string_lossy().to_string()));

        // Re-sorting moves sideways, never nests.
        let undo_id = result.undo_id.parse().unwrap();
        let resort = super::organize_assets(&catalog, &[(door.id, Some("Impacts".into()))], true).expect("resort");
        assert!(sfx.join("Impacts/door-closing.wav").is_file());
        assert!(!sfx.join("Doors/Impacts").exists());
        super::undo_with_files(&catalog, resort.undo_id.parse().unwrap()).expect("undo resort");
        assert!(!sfx.join("Impacts").exists(), "created folder removed once empty");

        super::undo_with_files(&catalog, undo_id).expect("undo");
        assert!(sfx.join("door-closing.wav").is_file());
        assert!(sfx.join("doorslam.wav").is_file());
        assert!(sfx.join("Doors/doorslam.wav").is_file(), "the occupant is untouched");
        assert!(catalog.asset_categories_for_library(library_id).unwrap().is_empty());

        super::redo_with_files(&catalog, undo_id).expect("redo");
        assert!(sfx.join("Doors/door-closing.wav").is_file());
        assert_eq!(catalog.asset_categories_for_library(library_id).unwrap()[&door.id].1, "Doors");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn renaming_a_category_moves_its_folder_and_merging_works_too() {
        let (root, catalog, library_id) = organize_fixture("rename");
        let sfx = root.join("_SoundFX");
        let a = referenced_asset(&catalog, library_id, &sfx.join("a.wav"), "a");
        let b = referenced_asset(&catalog, library_id, &sfx.join("b.wav"), "b");
        let c = referenced_asset(&catalog, library_id, &sfx.join("c.wav"), "c");
        super::organize_assets(&catalog, &[(a.id, Some("Wooshes".into())), (b.id, Some("Wooshes".into())), (c.id, Some("Whooshes".into()))], true)
            .expect("organize");
        let typo = catalog.find_category_tag("Wooshes").unwrap().unwrap();

        // Rename the typo onto the existing "Whooshes": a merge.
        let result = super::rename_category(&catalog, library_id, typo.id, "Whooshes", true).expect("rename");
        assert_eq!(result.moved, 2);
        assert!(sfx.join("Whooshes/a.wav").is_file() && sfx.join("Whooshes/c.wav").is_file());
        assert!(!sfx.join("Wooshes").exists(), "the emptied old folder is gone");
        let counts = catalog.category_counts_for_library(library_id).unwrap();
        assert_eq!(counts.iter().map(|(tag, n)| (tag.name.clone(), *n)).collect::<Vec<_>>(), vec![("Whooshes".to_string(), 3)]);

        // One undo un-merges, files and all.
        super::undo_with_files(&catalog, result.undo_id.parse().unwrap()).expect("undo");
        assert!(sfx.join("Wooshes/a.wav").is_file());
        assert_eq!(catalog.asset_categories_for_library(library_id).unwrap()[&a.id].1, "Wooshes");

        // Capitalization-only rename keeps the same category.
        let whooshes = catalog.find_category_tag("Whooshes").unwrap().unwrap();
        super::rename_category(&catalog, library_id, whooshes.id, "WHOOSHES", true).expect("case rename");
        assert_eq!(catalog.asset_categories_for_library(library_id).unwrap()[&c.id].1, "WHOOSHES");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn refiling_a_misfiled_sound_effect_is_undone_with_the_rest() {
        let (root, catalog, library_id) = organize_fixture("refile");
        let asset = referenced_asset(&catalog, library_id, &root.join("_SoundFX/whoosh.wav"), "w");
        catalog.set_media_type(asset.id, "music").unwrap();
        let result = super::organize_assets_refiling(
            &catalog,
            &[(asset.id, Some("Whooshes".into()))],
            &[(asset.id, "sound_effect".into())],
            false,
        )
        .expect("refile");
        assert_eq!(catalog.get_asset(asset.id).unwrap().unwrap().media_type, "sound_effect");
        super::undo_with_files(&catalog, result.undo_id.parse().unwrap()).expect("undo");
        assert_eq!(catalog.get_asset(asset.id).unwrap().unwrap().media_type, "music");
        assert!(catalog.asset_categories_for_library(library_id).unwrap().is_empty());
        super::redo_with_files(&catalog, result.undo_id.parse().unwrap()).expect("redo");
        assert_eq!(catalog.get_asset(asset.id).unwrap().unwrap().media_type, "sound_effect");
        let _ = std::fs::remove_dir_all(&root);
    }

    #[test]
    fn organize_keeps_the_category_but_reports_unreachable_files() {
        let (root, catalog, library_id) = organize_fixture("offline");
        let gone = referenced_asset(&catalog, library_id, &root.join("_SoundFX/whoosh.wav"), "w");
        std::fs::remove_file(root.join("_SoundFX/whoosh.wav")).unwrap();
        let result = super::organize_assets(&catalog, &[(gone.id, Some("Whooshes".into()))], true).expect("organize");
        assert_eq!(result.moved, 0);
        assert_eq!(result.move_failures.len(), 1);
        assert!(!root.join("_SoundFX/Whooshes").exists());
        assert_eq!(catalog.asset_categories_for_library(library_id).unwrap()[&gone.id].1, "Whooshes");
        let _ = std::fs::remove_dir_all(&root);
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
    fn backup_restore_requirements_include_catalog_snapshot_and_media_root() {
        assert_eq!(
            super::backup_restore_requirements(),
            vec!["catalog_snapshot", "media_root"]
        );
    }

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "darkwave-license-doc-test-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn catalog_with_asset(media_root: &std::path::Path) -> (storage::Catalog, uuid::Uuid) {
        let catalog = storage::Catalog::open(":memory:").expect("open catalog");
        let library = catalog
            .create_library("Test Library", media_root.to_string_lossy())
            .expect("create library");
        let storage::RegisterAssetOutcome::Created(asset) = catalog
            .register_asset(storage::NewAssetRecord {
                library_id: library.id,
                original_filename: "track.wav".to_string(),
                display_name: "Track".to_string(),
                path: storage::AssetPath::Managed("Media/00/track.wav".to_string()),
                storage_mode: shared_types::StorageMode::Managed,
                content_hash: Some("hash-license-doc-test".to_string()),
                media_type: "music".to_string(),
                file_size: 0,
                availability_state: shared_types::AvailabilityState::Local,
            })
            .expect("register asset")
        else {
            panic!("expected a freshly registered asset");
        };
        (catalog, asset.id)
    }

    #[test]
    fn attach_license_document_copies_into_media_root_license_folder_and_extracts_candidates() {
        let media_root = temp_dir("attach");
        let (catalog, asset_id) = catalog_with_asset(&media_root);

        let source_dir = temp_dir("attach-source");
        let source_path = source_dir.join("My License.pdf");
        // Not a real PDF — attach must still succeed (extraction is
        // best-effort) and just report zero candidates.
        std::fs::write(&source_path, b"not actually a pdf").unwrap();

        let outcome = super::attach_license_document_impl(&catalog, asset_id, &source_path)
            .expect("attach should succeed even when extraction fails");

        assert!(outcome.candidates.is_empty());
        let relative_path = outcome
            .source
            .license_document_path
            .clone()
            .expect("document path recorded");
        assert!(relative_path.starts_with(&format!("License/{asset_id}-")));
        assert!(media_root.join(&relative_path).exists());

        let reloaded = catalog
            .get_source_record(asset_id)
            .expect("query")
            .expect("record exists");
        assert_eq!(reloaded.license_document_path, Some(relative_path));

        std::fs::remove_dir_all(&media_root).ok();
        std::fs::remove_dir_all(&source_dir).ok();
    }

    #[test]
    fn attaching_a_replacement_document_removes_the_previous_file() {
        let media_root = temp_dir("attach-replace");
        let (catalog, asset_id) = catalog_with_asset(&media_root);
        let source_dir = temp_dir("attach-replace-source");

        let first_source = source_dir.join("Old License.pdf");
        std::fs::write(&first_source, b"not actually a pdf").unwrap();
        let first_outcome = super::attach_license_document_impl(&catalog, asset_id, &first_source).expect("first attach");
        let first_relative_path = first_outcome.source.license_document_path.expect("first path recorded");
        let first_absolute_path = media_root.join(&first_relative_path);
        assert!(first_absolute_path.exists());

        let second_source = source_dir.join("Renewed License.pdf");
        std::fs::write(&second_source, b"not actually a pdf either").unwrap();
        let second_outcome = super::attach_license_document_impl(&catalog, asset_id, &second_source).expect("second attach");
        let second_relative_path = second_outcome.source.license_document_path.expect("second path recorded");

        assert_ne!(first_relative_path, second_relative_path);
        assert!(!first_absolute_path.exists(), "replaced document should be cleaned up");
        assert!(media_root.join(&second_relative_path).exists());

        std::fs::remove_dir_all(&media_root).ok();
        std::fs::remove_dir_all(&source_dir).ok();
    }

    #[test]
    fn remove_license_document_deletes_file_and_clears_extracted_dates_but_keeps_manual_ones() {
        let media_root = temp_dir("remove");
        let (catalog, asset_id) = catalog_with_asset(&media_root);
        let source_dir = temp_dir("remove-source");
        let source_path = source_dir.join("license.pdf");
        std::fs::write(&source_path, b"not actually a pdf").unwrap();

        let outcome = super::attach_license_document_impl(&catalog, asset_id, &source_path).expect("attach");
        let mut draft = outcome.source;
        draft.license_valid_until = Some("2027-01-05".to_string());
        draft.license_expiry_source = Some("manual".to_string());
        catalog.set_source_record(draft).expect("save manual date");

        super::remove_license_document_impl(&catalog, asset_id).expect("remove");

        let reloaded = catalog
            .get_source_record(asset_id)
            .expect("query")
            .expect("record exists");
        assert_eq!(reloaded.license_document_path, None);
        // A manually-confirmed date must survive document removal.
        assert_eq!(reloaded.license_valid_until.as_deref(), Some("2027-01-05"));

        std::fs::remove_dir_all(&media_root).ok();
        std::fs::remove_dir_all(&source_dir).ok();
    }

    #[test]
    fn remove_license_document_clears_only_the_unconfirmed_extracted_date_not_a_manual_valid_from() {
        let media_root = temp_dir("remove-mixed");
        let (catalog, asset_id) = catalog_with_asset(&media_root);
        let source_dir = temp_dir("remove-mixed-source");
        let source_path = source_dir.join("license.pdf");
        std::fs::write(&source_path, b"not actually a pdf").unwrap();

        let outcome = super::attach_license_document_impl(&catalog, asset_id, &source_path).expect("attach");
        let mut draft = outcome.source;
        // license_valid_from is manually typed (there is no extraction path
        // for it at all) while license_valid_until is still just an
        // unconfirmed extracted suggestion — license_expiry_source only
        // describes the latter, and must not cause the former to be wiped.
        draft.license_valid_from = Some("2026-01-01".to_string());
        draft.license_valid_until = Some("2027-01-05".to_string());
        draft.license_expiry_source = Some("extracted".to_string());
        catalog.set_source_record(draft).expect("save mixed manual/extracted dates");

        super::remove_license_document_impl(&catalog, asset_id).expect("remove");

        let reloaded = catalog
            .get_source_record(asset_id)
            .expect("query")
            .expect("record exists");
        assert_eq!(reloaded.license_valid_from.as_deref(), Some("2026-01-01"));
        assert_eq!(reloaded.license_valid_until, None);
        assert_eq!(reloaded.license_expiry_source, None);

        std::fs::remove_dir_all(&media_root).ok();
        std::fs::remove_dir_all(&source_dir).ok();
    }

    #[test]
    fn resolve_license_document_path_returns_absolute_path_under_media_root() {
        let media_root = temp_dir("resolve");
        let (catalog, asset_id) = catalog_with_asset(&media_root);
        let source_dir = temp_dir("resolve-source");
        let source_path = source_dir.join("license.pdf");
        std::fs::write(&source_path, b"not actually a pdf").unwrap();
        super::attach_license_document_impl(&catalog, asset_id, &source_path).expect("attach");

        let resolved = super::resolve_license_document_path_impl(&catalog, asset_id)
            .expect("resolve")
            .expect("path present");
        assert!(std::path::Path::new(&resolved).starts_with(&media_root));
        assert!(std::path::Path::new(&resolved).exists());

        std::fs::remove_dir_all(&media_root).ok();
        std::fs::remove_dir_all(&source_dir).ok();
    }
}
