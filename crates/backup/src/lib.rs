use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BackupPackage {
    pub library_id: Uuid,
    pub manifest_revision: u64,
    pub media_root: String,
    pub catalog_snapshot_path: String,
    pub manifest_path: String,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RestorePlan {
    pub library_id: Uuid,
    pub manifest_revision: u64,
    pub media_root: String,
    pub catalog_snapshot_path: String,
    pub manifest_path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RestoreValidationError {
    MissingCatalogSnapshot,
    MissingManifest,
}

impl BackupPackage {
    pub fn restore_plan(&self) -> RestorePlan {
        RestorePlan {
            library_id: self.library_id,
            manifest_revision: self.manifest_revision,
            media_root: self.media_root.clone(),
            catalog_snapshot_path: self.catalog_snapshot_path.clone(),
            manifest_path: self.manifest_path.clone(),
        }
    }

    pub fn validate_restore_inputs(
        &self,
        exists: impl Fn(&str) -> bool,
    ) -> Result<(), RestoreValidationError> {
        if !exists(&self.catalog_snapshot_path) {
            return Err(RestoreValidationError::MissingCatalogSnapshot);
        }

        if !exists(&self.manifest_path) {
            return Err(RestoreValidationError::MissingManifest);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn backup_package_preserves_manifest_and_media_root() {
        let package = BackupPackage {
            library_id: Uuid::new_v4(),
            manifest_revision: 42,
            media_root: "/Volumes/Sound Library".to_string(),
            catalog_snapshot_path: "Backups/catalog.sqlite".to_string(),
            manifest_path: "Backups/library.darkwave-manifest.json".to_string(),
            created_at_ms: 1_000,
        };

        assert_eq!(package.restore_plan().library_id, package.library_id);
        assert_eq!(package.restore_plan().media_root, "/Volumes/Sound Library");
    }

    #[test]
    fn restore_validation_rejects_missing_catalog_snapshot() {
        let package = test_package();

        assert_eq!(
            package.validate_restore_inputs(|path| path.ends_with("manifest.json")),
            Err(RestoreValidationError::MissingCatalogSnapshot)
        );
    }

    #[test]
    fn restore_validation_rejects_missing_manifest() {
        let package = test_package();

        assert_eq!(
            package.validate_restore_inputs(|path| path.ends_with("catalog.sqlite")),
            Err(RestoreValidationError::MissingManifest)
        );
    }

    #[test]
    fn restore_validation_accepts_complete_package() {
        let package = test_package();

        assert_eq!(package.validate_restore_inputs(|_| true), Ok(()));
    }

    fn test_package() -> BackupPackage {
        BackupPackage {
            library_id: Uuid::new_v4(),
            manifest_revision: 7,
            media_root: "/library".to_string(),
            catalog_snapshot_path: "catalog.sqlite".to_string(),
            manifest_path: "manifest.json".to_string(),
            created_at_ms: 1_000,
        }
    }
}
