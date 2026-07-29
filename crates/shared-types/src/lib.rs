use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum StorageMode {
    Managed,
    Referenced,
    Hybrid,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum AvailabilityState {
    Unknown,
    Local,
    Cached,
    Missing,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetIdentity {
    pub id: Uuid,
    pub original_filename: String,
    pub display_name: String,
    pub storage_mode: StorageMode,
    pub availability_state: AvailabilityState,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn asset_identity_preserves_display_name_separate_from_original() {
        let asset = AssetIdentity {
            id: Uuid::nil(),
            original_filename: "vendor_003_big-hit.wav".to_string(),
            display_name: "Big Hit".to_string(),
            storage_mode: StorageMode::Referenced,
            availability_state: AvailabilityState::Unknown,
        };

        assert_ne!(asset.original_filename, asset.display_name);
    }
}
