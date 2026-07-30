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

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupSource {
    pub catalog_path: String,
    pub manifest_path: String,
    pub backup_dir: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackupError {
    CatalogSnapshotFailed,
    ManifestSnapshotFailed,
}

/// Copies the live catalog and manifest into `source.backup_dir` via the injected `copy`
/// operation, producing a package that can later be restored. Stops after the catalog
/// snapshot fails so a partial, manifest-less backup is never reported as usable.
pub fn create_backup(
    library_id: Uuid,
    manifest_revision: u64,
    media_root: impl Into<String>,
    source: &BackupSource,
    created_at_ms: u64,
    mut copy: impl FnMut(&str, &str) -> bool,
) -> Result<BackupPackage, BackupError> {
    let backup_dir = source.backup_dir.trim_end_matches('/');
    let catalog_snapshot_path = format!("{backup_dir}/catalog.sqlite");
    let manifest_path = format!("{backup_dir}/library.darkwave-manifest.json");

    if !copy(&source.catalog_path, &catalog_snapshot_path) {
        return Err(BackupError::CatalogSnapshotFailed);
    }

    if !copy(&source.manifest_path, &manifest_path) {
        return Err(BackupError::ManifestSnapshotFailed);
    }

    Ok(BackupPackage {
        library_id,
        manifest_revision,
        media_root: media_root.into(),
        catalog_snapshot_path,
        manifest_path,
        created_at_ms,
    })
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

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RestoreError {
    CatalogRestoreFailed,
    ManifestRestoreFailed,
}

impl RestorePlan {
    /// Copies the backed-up catalog and manifest into their live locations via the
    /// injected `copy` operation. Callers should run `validate_restore_inputs` first;
    /// this only reports whether each copy itself succeeded.
    pub fn apply(
        &self,
        destination_catalog_path: &str,
        destination_manifest_path: &str,
        mut copy: impl FnMut(&str, &str) -> bool,
    ) -> Result<(), RestoreError> {
        if !copy(&self.catalog_snapshot_path, destination_catalog_path) {
            return Err(RestoreError::CatalogRestoreFailed);
        }

        if !copy(&self.manifest_path, destination_manifest_path) {
            return Err(RestoreError::ManifestRestoreFailed);
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

    #[test]
    fn create_backup_snapshots_catalog_and_manifest_into_backup_dir() {
        let library_id = Uuid::new_v4();
        let source = BackupSource {
            catalog_path: "AppData/catalog.sqlite".to_string(),
            manifest_path: "Library/library.darkwave-manifest.json".to_string(),
            backup_dir: "Backups/2026-07-30/".to_string(),
        };
        let mut copied = Vec::new();

        let package = create_backup(library_id, 9, "/Volumes/SFX", &source, 1_700, |from, to| {
            copied.push((from.to_string(), to.to_string()));
            true
        })
        .expect("backup created");

        assert_eq!(package.library_id, library_id);
        assert_eq!(package.manifest_revision, 9);
        assert_eq!(package.media_root, "/Volumes/SFX");
        assert_eq!(
            package.catalog_snapshot_path,
            "Backups/2026-07-30/catalog.sqlite"
        );
        assert_eq!(
            package.manifest_path,
            "Backups/2026-07-30/library.darkwave-manifest.json"
        );
        assert_eq!(
            copied,
            vec![
                (
                    "AppData/catalog.sqlite".to_string(),
                    "Backups/2026-07-30/catalog.sqlite".to_string()
                ),
                (
                    "Library/library.darkwave-manifest.json".to_string(),
                    "Backups/2026-07-30/library.darkwave-manifest.json".to_string()
                ),
            ]
        );
    }

    #[test]
    fn create_backup_stops_after_catalog_snapshot_failure() {
        let source = BackupSource {
            catalog_path: "AppData/catalog.sqlite".to_string(),
            manifest_path: "Library/library.darkwave-manifest.json".to_string(),
            backup_dir: "Backups/2026-07-30".to_string(),
        };

        let result = create_backup(Uuid::new_v4(), 1, "/Volumes/SFX", &source, 1_700, |_, _| {
            false
        });

        assert_eq!(result, Err(BackupError::CatalogSnapshotFailed));
    }

    #[test]
    fn restore_plan_applies_catalog_and_manifest_copies() {
        let plan = test_package().restore_plan();
        let mut copied = Vec::new();

        let result = plan.apply(
            "AppData/catalog.sqlite",
            "Library/manifest.json",
            |from, to| {
                copied.push((from.to_string(), to.to_string()));
                true
            },
        );

        assert_eq!(result, Ok(()));
        assert_eq!(
            copied,
            vec![
                (
                    "catalog.sqlite".to_string(),
                    "AppData/catalog.sqlite".to_string()
                ),
                (
                    "manifest.json".to_string(),
                    "Library/manifest.json".to_string()
                ),
            ]
        );
    }

    #[test]
    fn restore_plan_reports_manifest_copy_failure() {
        let plan = test_package().restore_plan();

        let result = plan.apply(
            "AppData/catalog.sqlite",
            "Library/manifest.json",
            |from, _| from == "catalog.sqlite",
        );

        assert_eq!(result, Err(RestoreError::ManifestRestoreFailed));
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
