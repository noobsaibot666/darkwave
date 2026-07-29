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

#[derive(Clone, Debug, Eq, PartialEq)]
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
}
