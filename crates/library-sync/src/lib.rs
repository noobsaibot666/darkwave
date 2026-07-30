use std::path::Path;

use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum WriterLeaseState {
    Writable,
    ReadOnlyBecauseAnotherWriterExists,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ManifestAsset {
    pub id: Uuid,
    pub relative_path: String,
    pub content_hash: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PortableManifest {
    pub library_id: Uuid,
    pub revision: u64,
    pub assets: Vec<ManifestAsset>,
}

#[derive(Debug)]
pub enum SyncFileError {
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl From<std::io::Error> for SyncFileError {
    fn from(error: std::io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for SyncFileError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct WriterLease {
    pub device_id: String,
    pub acquired_at_ms: u64,
    pub ttl_ms: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MediaRootStatus {
    Online,
    Offline,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaRootProbe {
    pub media_root: String,
    pub status: MediaRootStatus,
    pub reconnect_validation_required: bool,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconnectValidationJob {
    pub library_id: Uuid,
    pub manifest_revision: u64,
    pub paths_to_validate: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconnectValidationReport {
    pub library_id: Uuid,
    pub manifest_revision: u64,
    pub checked_paths: usize,
    pub missing_paths: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ReconnectValidationStatus {
    Pending,
    Completed,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct QueuedReconnectValidation {
    pub queue_id: u64,
    pub job: ReconnectValidationJob,
    pub status: ReconnectValidationStatus,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconnectValidationScheduler {
    next_queue_id: u64,
    pub entries: Vec<QueuedReconnectValidation>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OfflineControlState {
    pub media_root: String,
    pub catalog_only: bool,
    pub validation_paused: bool,
    pub reconnect_requested: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum OfflineControlCommand {
    UseCatalogOnly,
    RetryReconnect,
    PauseValidation,
    ResumeValidation,
    RelinkMediaRoot { media_root: String },
}

impl PortableManifest {
    pub fn new(library_id: Uuid, revision: u64) -> Self {
        Self {
            library_id,
            revision,
            assets: Vec::new(),
        }
    }

    pub fn with_asset(mut self, asset: ManifestAsset) -> Self {
        self.assets.push(asset);
        self
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn from_json(value: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(value)
    }
}

pub fn write_manifest_file(
    manifest: &PortableManifest,
    path: impl AsRef<Path>,
) -> Result<(), SyncFileError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, manifest.to_json()?)?;

    Ok(())
}

pub fn read_manifest_file(path: impl AsRef<Path>) -> Result<PortableManifest, SyncFileError> {
    let contents = std::fs::read_to_string(path)?;
    Ok(PortableManifest::from_json(&contents)?)
}

pub fn write_lease_file(lease: &WriterLease, path: impl AsRef<Path>) -> Result<(), SyncFileError> {
    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(lease)?)?;

    Ok(())
}

pub fn read_lease_file(path: impl AsRef<Path>) -> Result<WriterLease, SyncFileError> {
    let contents = std::fs::read_to_string(path)?;
    Ok(serde_json::from_str(&contents)?)
}

/// Attempts to claim the shared-media writer lease file for `device_id`.
///
/// Writes a fresh lease (and so renews it) whenever the caller is allowed to be the
/// writer: no lease exists, the existing lease expired, or `device_id` already holds
/// it. Leaves another device's live lease untouched and reports it as read-only.
pub fn acquire_lease_file(
    path: impl AsRef<Path>,
    device_id: &str,
    ttl_ms: u64,
    now_ms: u64,
) -> Result<WriterLeaseState, SyncFileError> {
    let path = path.as_ref();
    let existing = if path.exists() {
        Some(read_lease_file(path)?)
    } else {
        None
    };

    let state = lease_state_at(existing.as_ref(), device_id, now_ms);
    if state == WriterLeaseState::Writable {
        write_lease_file(
            &WriterLease {
                device_id: device_id.to_string(),
                acquired_at_ms: now_ms,
                ttl_ms,
            },
            path,
        )?;
    }

    Ok(state)
}

/// Removes the lease file, but only when it is still owned by `device_id`.
pub fn release_lease_file(path: impl AsRef<Path>, device_id: &str) -> Result<(), SyncFileError> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(());
    }

    if read_lease_file(path)?.device_id == device_id {
        std::fs::remove_file(path)?;
    }

    Ok(())
}

pub fn probe_media_root(
    media_root: impl AsRef<str>,
    exists: impl Fn(&str) -> bool,
) -> MediaRootProbe {
    let media_root = media_root.as_ref().to_string();
    let status = if exists(&media_root) {
        MediaRootStatus::Online
    } else {
        MediaRootStatus::Offline
    };

    MediaRootProbe {
        media_root,
        status,
        reconnect_validation_required: status == MediaRootStatus::Online,
    }
}

pub fn plan_reconnect_validation(
    manifest: &PortableManifest,
    probe: &MediaRootProbe,
) -> Option<ReconnectValidationJob> {
    if !probe.reconnect_validation_required || probe.status != MediaRootStatus::Online {
        return None;
    }

    let media_root = probe.media_root.trim_end_matches('/');
    Some(ReconnectValidationJob {
        library_id: manifest.library_id,
        manifest_revision: manifest.revision,
        paths_to_validate: manifest
            .assets
            .iter()
            .map(|asset| format!("{media_root}/{}", asset.relative_path))
            .collect(),
    })
}

pub fn validate_reconnect_paths(
    job: &ReconnectValidationJob,
    exists: impl Fn(&str) -> bool,
) -> ReconnectValidationReport {
    let missing_paths = job
        .paths_to_validate
        .iter()
        .filter(|path| !exists(path))
        .cloned()
        .collect();

    ReconnectValidationReport {
        library_id: job.library_id,
        manifest_revision: job.manifest_revision,
        checked_paths: job.paths_to_validate.len(),
        missing_paths,
    }
}

pub fn lease_state(active_writer_device: Option<&str>, current_device: &str) -> WriterLeaseState {
    match active_writer_device {
        Some(device) if device != current_device => {
            WriterLeaseState::ReadOnlyBecauseAnotherWriterExists
        }
        _ => WriterLeaseState::Writable,
    }
}

pub fn lease_state_at(
    active_lease: Option<&WriterLease>,
    current_device: &str,
    now_ms: u64,
) -> WriterLeaseState {
    match active_lease {
        Some(lease)
            if lease.device_id != current_device
                && now_ms.saturating_sub(lease.acquired_at_ms) <= lease.ttl_ms =>
        {
            WriterLeaseState::ReadOnlyBecauseAnotherWriterExists
        }
        _ => WriterLeaseState::Writable,
    }
}

impl ReconnectValidationScheduler {
    pub fn new() -> Self {
        Self {
            next_queue_id: 1,
            entries: Vec::new(),
        }
    }

    pub fn enqueue_from_probe(
        &mut self,
        manifest: &PortableManifest,
        probe: &MediaRootProbe,
    ) -> Option<QueuedReconnectValidation> {
        let job = plan_reconnect_validation(manifest, probe)?;

        if self.entries.iter().any(|entry| {
            entry.status == ReconnectValidationStatus::Pending
                && entry.job.library_id == job.library_id
                && entry.job.manifest_revision == job.manifest_revision
        }) {
            return None;
        }

        let queued = QueuedReconnectValidation {
            queue_id: self.next_queue_id,
            job,
            status: ReconnectValidationStatus::Pending,
        };
        self.next_queue_id += 1;
        self.entries.push(queued.clone());

        Some(queued)
    }

    pub fn next_pending(&self) -> Option<&QueuedReconnectValidation> {
        self.entries
            .iter()
            .find(|entry| entry.status == ReconnectValidationStatus::Pending)
    }

    pub fn mark_completed(&mut self, queue_id: u64) -> bool {
        if let Some(entry) = self
            .entries
            .iter_mut()
            .find(|entry| entry.queue_id == queue_id)
        {
            entry.status = ReconnectValidationStatus::Completed;
            return true;
        }

        false
    }

    pub fn pending_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.status == ReconnectValidationStatus::Pending)
            .count()
    }

    pub fn completed_count(&self) -> usize {
        self.entries
            .iter()
            .filter(|entry| entry.status == ReconnectValidationStatus::Completed)
            .count()
    }
}

impl ReconnectValidationReport {
    pub fn all_available(&self) -> bool {
        self.missing_paths.is_empty()
    }
}

impl Default for ReconnectValidationScheduler {
    fn default() -> Self {
        Self::new()
    }
}

impl OfflineControlState {
    pub fn new(media_root: impl Into<String>) -> Self {
        Self {
            media_root: media_root.into(),
            catalog_only: false,
            validation_paused: false,
            reconnect_requested: false,
        }
    }

    pub fn apply(&mut self, command: OfflineControlCommand) {
        match command {
            OfflineControlCommand::UseCatalogOnly => {
                self.catalog_only = true;
                self.reconnect_requested = false;
            }
            OfflineControlCommand::RetryReconnect => {
                self.catalog_only = false;
                self.reconnect_requested = true;
            }
            OfflineControlCommand::PauseValidation => {
                self.validation_paused = true;
            }
            OfflineControlCommand::ResumeValidation => {
                self.validation_paused = false;
            }
            OfflineControlCommand::RelinkMediaRoot { media_root } => {
                self.media_root = media_root;
                self.catalog_only = false;
                self.reconnect_requested = true;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn conflicting_writer_forces_read_only_mode() {
        assert_eq!(
            lease_state(Some("edit-suite"), "laptop"),
            WriterLeaseState::ReadOnlyBecauseAnotherWriterExists
        );
    }

    #[test]
    fn manifest_snapshot_round_trips_library_assets_and_revision() {
        let library_id = Uuid::new_v4();
        let asset_id = Uuid::new_v4();
        let manifest = PortableManifest::new(library_id, 7).with_asset(ManifestAsset {
            id: asset_id,
            relative_path: "Media/00/impact.wav".to_string(),
            content_hash: "abc".to_string(),
        });

        let encoded = manifest.to_json().expect("encode");
        let decoded = PortableManifest::from_json(&encoded).expect("decode");

        assert_eq!(decoded.library_id, library_id);
        assert_eq!(decoded.revision, 7);
        assert_eq!(decoded.assets[0].id, asset_id);
    }

    #[test]
    fn manifest_snapshot_round_trips_through_file() {
        let library_id = Uuid::new_v4();
        let manifest = PortableManifest::new(library_id, 8).with_asset(ManifestAsset {
            id: Uuid::new_v4(),
            relative_path: "Media/00/impact.wav".to_string(),
            content_hash: "hash-impact".to_string(),
        });
        let path = unique_manifest_path("portable-manifest");

        write_manifest_file(&manifest, &path).expect("write manifest");
        let loaded = read_manifest_file(&path).expect("read manifest");

        assert_eq!(loaded, manifest);
    }

    #[test]
    fn acquire_lease_file_claims_lease_when_none_exists() {
        let path = unique_manifest_path("lease-fresh");

        let state = acquire_lease_file(&path, "laptop", 1_000, 1_000).expect("acquire");

        assert_eq!(state, WriterLeaseState::Writable);
        let lease = read_lease_file(&path).expect("read lease");
        assert_eq!(lease.device_id, "laptop");
        assert_eq!(lease.acquired_at_ms, 1_000);
    }

    #[test]
    fn acquire_lease_file_denies_other_device_within_ttl() {
        let path = unique_manifest_path("lease-conflict");
        acquire_lease_file(&path, "edit-suite", 1_000, 1_000).expect("first acquire");

        let state = acquire_lease_file(&path, "laptop", 1_000, 1_200).expect("second acquire");

        assert_eq!(state, WriterLeaseState::ReadOnlyBecauseAnotherWriterExists);
        assert_eq!(
            read_lease_file(&path).expect("read lease").device_id,
            "edit-suite"
        );
    }

    #[test]
    fn acquire_lease_file_allows_takeover_after_expiry() {
        let path = unique_manifest_path("lease-expired");
        acquire_lease_file(&path, "edit-suite", 500, 1_000).expect("first acquire");

        let state = acquire_lease_file(&path, "laptop", 500, 2_000).expect("second acquire");

        assert_eq!(state, WriterLeaseState::Writable);
        assert_eq!(
            read_lease_file(&path).expect("read lease").device_id,
            "laptop"
        );
    }

    #[test]
    fn release_lease_file_removes_lease_owned_by_device() {
        let path = unique_manifest_path("lease-release");
        acquire_lease_file(&path, "laptop", 1_000, 1_000).expect("acquire");

        release_lease_file(&path, "laptop").expect("release");

        assert!(!path.exists());
    }

    #[test]
    fn release_lease_file_ignores_non_owner() {
        let path = unique_manifest_path("lease-release-denied");
        acquire_lease_file(&path, "edit-suite", 1_000, 1_000).expect("acquire");

        release_lease_file(&path, "laptop").expect("release attempt");

        assert!(path.exists());
        assert_eq!(
            read_lease_file(&path).expect("read lease").device_id,
            "edit-suite"
        );
    }

    #[test]
    fn expired_writer_lease_allows_current_device_to_take_over() {
        let lease = WriterLease {
            device_id: "edit-suite".to_string(),
            acquired_at_ms: 1_000,
            ttl_ms: 500,
        };

        assert_eq!(
            lease_state_at(Some(&lease), "laptop", 1_600),
            WriterLeaseState::Writable
        );
        assert_eq!(
            lease_state_at(Some(&lease), "laptop", 1_200),
            WriterLeaseState::ReadOnlyBecauseAnotherWriterExists
        );
    }

    #[test]
    fn online_media_root_requests_reconnect_validation() {
        let probe = probe_media_root("/Volumes/TrueNAS/SFX", |path| {
            path == "/Volumes/TrueNAS/SFX"
        });

        assert_eq!(probe.status, MediaRootStatus::Online);
        assert!(probe.reconnect_validation_required);
    }

    #[test]
    fn offline_media_root_keeps_catalog_available_without_validation() {
        let probe = probe_media_root("/Volumes/TrueNAS/SFX", |_| false);

        assert_eq!(probe.status, MediaRootStatus::Offline);
        assert!(!probe.reconnect_validation_required);
    }

    #[test]
    fn online_media_root_plans_reconnect_validation_for_manifest_assets() {
        let library_id = Uuid::new_v4();
        let manifest = PortableManifest::new(library_id, 9)
            .with_asset(ManifestAsset {
                id: Uuid::new_v4(),
                relative_path: "Media/00/impact.wav".to_string(),
                content_hash: "hash-impact".to_string(),
            })
            .with_asset(ManifestAsset {
                id: Uuid::new_v4(),
                relative_path: "Media/00/riser.wav".to_string(),
                content_hash: "hash-riser".to_string(),
            });
        let probe = MediaRootProbe {
            media_root: "/Volumes/TrueNAS/SFX".to_string(),
            status: MediaRootStatus::Online,
            reconnect_validation_required: true,
        };

        let job = plan_reconnect_validation(&manifest, &probe).expect("validation job");

        assert_eq!(job.library_id, library_id);
        assert_eq!(job.manifest_revision, 9);
        assert_eq!(
            job.paths_to_validate,
            vec![
                "/Volumes/TrueNAS/SFX/Media/00/impact.wav".to_string(),
                "/Volumes/TrueNAS/SFX/Media/00/riser.wav".to_string(),
            ]
        );
    }

    #[test]
    fn offline_media_root_does_not_plan_reconnect_validation() {
        let manifest = PortableManifest::new(Uuid::new_v4(), 1);
        let probe = MediaRootProbe {
            media_root: "/Volumes/TrueNAS/SFX".to_string(),
            status: MediaRootStatus::Offline,
            reconnect_validation_required: false,
        };

        assert_eq!(plan_reconnect_validation(&manifest, &probe), None);
    }

    #[test]
    fn reconnect_validation_scheduler_enqueues_online_probe_once() {
        let library_id = Uuid::new_v4();
        let manifest = PortableManifest::new(library_id, 4).with_asset(ManifestAsset {
            id: Uuid::new_v4(),
            relative_path: "Media/00/impact.wav".to_string(),
            content_hash: "hash-impact".to_string(),
        });
        let probe = MediaRootProbe {
            media_root: "/Volumes/TrueNAS/SFX".to_string(),
            status: MediaRootStatus::Online,
            reconnect_validation_required: true,
        };
        let mut scheduler = ReconnectValidationScheduler::new();

        let first = scheduler.enqueue_from_probe(&manifest, &probe);
        let duplicate = scheduler.enqueue_from_probe(&manifest, &probe);

        assert!(first.is_some());
        assert_eq!(duplicate, None);
        assert_eq!(scheduler.pending_count(), 1);
        assert_eq!(
            scheduler.next_pending().map(|entry| entry.job.library_id),
            Some(library_id)
        );
    }

    #[test]
    fn reconnect_validation_scheduler_marks_pending_job_completed() {
        let manifest = PortableManifest::new(Uuid::new_v4(), 2).with_asset(ManifestAsset {
            id: Uuid::new_v4(),
            relative_path: "Media/00/riser.wav".to_string(),
            content_hash: "hash-riser".to_string(),
        });
        let probe = MediaRootProbe {
            media_root: "/Volumes/TrueNAS/SFX".to_string(),
            status: MediaRootStatus::Online,
            reconnect_validation_required: true,
        };
        let mut scheduler = ReconnectValidationScheduler::new();
        let queued = scheduler
            .enqueue_from_probe(&manifest, &probe)
            .expect("queued validation");

        assert!(scheduler.mark_completed(queued.queue_id));

        assert_eq!(scheduler.next_pending(), None);
        assert_eq!(scheduler.pending_count(), 0);
        assert_eq!(scheduler.completed_count(), 1);
    }

    #[test]
    fn reconnect_validation_reports_missing_manifest_paths() {
        let library_id = Uuid::new_v4();
        let job = ReconnectValidationJob {
            library_id,
            manifest_revision: 12,
            paths_to_validate: vec![
                "/Volumes/TrueNAS/SFX/Media/00/impact.wav".to_string(),
                "/Volumes/TrueNAS/SFX/Media/00/missing.wav".to_string(),
                "/Volumes/TrueNAS/SFX/Media/00/riser.wav".to_string(),
            ],
        };

        let report = validate_reconnect_paths(&job, |path| !path.ends_with("missing.wav"));

        assert_eq!(report.library_id, library_id);
        assert_eq!(report.manifest_revision, 12);
        assert_eq!(report.checked_paths, 3);
        assert_eq!(
            report.missing_paths,
            vec!["/Volumes/TrueNAS/SFX/Media/00/missing.wav".to_string()]
        );
        assert!(!report.all_available());
    }

    #[test]
    fn offline_controls_switch_to_catalog_only_and_retry_reconnect() {
        let mut controls = OfflineControlState::new("/Volumes/TrueNAS/SFX");

        controls.apply(OfflineControlCommand::UseCatalogOnly);
        assert!(controls.catalog_only);
        assert!(!controls.reconnect_requested);

        controls.apply(OfflineControlCommand::RetryReconnect);
        assert!(!controls.catalog_only);
        assert!(controls.reconnect_requested);
    }

    #[test]
    fn offline_controls_pause_resume_and_relink_media_root() {
        let mut controls = OfflineControlState::new("/Volumes/TrueNAS/SFX");

        controls.apply(OfflineControlCommand::PauseValidation);
        controls.apply(OfflineControlCommand::RelinkMediaRoot {
            media_root: "/Volumes/Sound/SFX".to_string(),
        });

        assert!(controls.validation_paused);
        assert_eq!(controls.media_root, "/Volumes/Sound/SFX");
        assert!(controls.reconnect_requested);

        controls.apply(OfflineControlCommand::ResumeValidation);
        assert!(!controls.validation_paused);
    }

    fn unique_manifest_path(name: &str) -> std::path::PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!("darkwave-{name}-{}", Uuid::new_v4()));
        path.push("library.darkwave-manifest.json");
        path
    }
}
