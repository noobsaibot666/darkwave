use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum TrashState {
    InTrash,
    Restored,
    Purged,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct TrashItem {
    pub asset_id: Uuid,
    pub original_path: String,
    pub trashed_at_ms: u64,
    pub reason: String,
    pub state: TrashState,
    pub file_deleted: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RestorePlan {
    pub asset_id: Uuid,
    pub restore_path: String,
}

impl TrashItem {
    pub fn for_asset(
        asset_id: Uuid,
        original_path: String,
        trashed_at_ms: u64,
        reason: String,
    ) -> Self {
        Self {
            asset_id,
            original_path,
            trashed_at_ms,
            reason,
            state: TrashState::InTrash,
            file_deleted: false,
        }
    }

    pub fn restore_plan(&self) -> RestorePlan {
        RestorePlan {
            asset_id: self.asset_id,
            restore_path: self.original_path.clone(),
        }
    }

    pub fn is_purge_allowed(&self, now_ms: u64, retention_ms: u64, explicit_request: bool) -> bool {
        explicit_request
            && self.state == TrashState::InTrash
            && now_ms.saturating_sub(self.trashed_at_ms) >= retention_ms
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn move_to_trash_records_restore_location_without_deleting_file() {
        let asset_id = Uuid::new_v4();
        let item = TrashItem::for_asset(
            asset_id,
            "/library/Media/00/hit.wav".to_string(),
            1_000,
            "duplicate review".to_string(),
        );

        assert_eq!(item.asset_id, asset_id);
        assert_eq!(item.original_path, "/library/Media/00/hit.wav");
        assert_eq!(item.state, TrashState::InTrash);
        assert!(!item.file_deleted);
    }

    #[test]
    fn restore_from_trash_returns_relink_instruction() {
        let item = TrashItem::for_asset(
            Uuid::new_v4(),
            "/library/Media/00/hit.wav".to_string(),
            1_000,
            "mistake".to_string(),
        );

        assert_eq!(
            item.restore_plan(),
            RestorePlan {
                asset_id: item.asset_id,
                restore_path: "/library/Media/00/hit.wav".to_string(),
            }
        );
    }

    #[test]
    fn purge_requires_explicit_request_and_retention_age() {
        let item = TrashItem::for_asset(
            Uuid::new_v4(),
            "/library/old.wav".to_string(),
            1_000,
            "cleanup".to_string(),
        );

        assert!(!item.is_purge_allowed(1_000 + 6_000, 7_000, false));
        assert!(!item.is_purge_allowed(1_000 + 6_000, 7_000, true));
        assert!(item.is_purge_allowed(1_000 + 7_000, 7_000, true));
    }
}
