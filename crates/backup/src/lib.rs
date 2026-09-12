use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A backup is a single `.darkwavebak` file — a plain copy of the project's
/// `.darkwave` catalog at the moment of backup (the caller is expected to
/// checkpoint its WAL first, e.g. via `storage::Catalog::checkpoint_wal`, so
/// the copy is complete). Nothing else needs to travel alongside it: the
/// catalog file already contains every asset, tag, and folder-role a
/// restore needs — there is no separate manifest to keep in sync with it,
/// unlike the old shared-catalog era's `library.darkwave-manifest.json`
/// (a derived, regeneratable export, never independent state).
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BackupPackage {
    pub library_id: Uuid,
    pub media_root: String,
    pub backup_file_path: String,
    pub created_at_ms: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RestorePlan {
    pub library_id: Uuid,
    pub media_root: String,
    pub backup_file_path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RestoreValidationError {
    MissingBackupFile,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BackupSource {
    pub catalog_path: String,
    pub backup_file_path: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BackupError {
    CatalogSnapshotFailed,
}

/// Copies the live catalog to `source.backup_file_path` via the injected
/// `copy` operation, producing a package describing what was just written.
pub fn create_backup(
    library_id: Uuid,
    media_root: impl Into<String>,
    source: &BackupSource,
    created_at_ms: u64,
    mut copy: impl FnMut(&str, &str) -> bool,
) -> Result<BackupPackage, BackupError> {
    if !copy(&source.catalog_path, &source.backup_file_path) {
        return Err(BackupError::CatalogSnapshotFailed);
    }

    Ok(BackupPackage {
        library_id,
        media_root: media_root.into(),
        backup_file_path: source.backup_file_path.clone(),
        created_at_ms,
    })
}

impl BackupPackage {
    pub fn restore_plan(&self) -> RestorePlan {
        RestorePlan {
            library_id: self.library_id,
            media_root: self.media_root.clone(),
            backup_file_path: self.backup_file_path.clone(),
        }
    }

    pub fn validate_restore_inputs(
        &self,
        exists: impl Fn(&str) -> bool,
    ) -> Result<(), RestoreValidationError> {
        if !exists(&self.backup_file_path) {
            return Err(RestoreValidationError::MissingBackupFile);
        }

        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum RestoreError {
    CatalogRestoreFailed,
}

impl RestorePlan {
    /// Copies the backed-up catalog into its live project-file location via
    /// the injected `copy` operation. Callers should run
    /// `validate_restore_inputs` first; this only reports whether the copy
    /// itself succeeded.
    pub fn apply(
        &self,
        destination_catalog_path: &str,
        mut copy: impl FnMut(&str, &str) -> bool,
    ) -> Result<(), RestoreError> {
        if !copy(&self.backup_file_path, destination_catalog_path) {
            return Err(RestoreError::CatalogRestoreFailed);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn backup_package_preserves_media_root() {
        let package = BackupPackage {
            library_id: Uuid::new_v4(),
            media_root: "/Volumes/Sound Library".to_string(),
            backup_file_path: "Backups/My Library.darkwavebak".to_string(),
            created_at_ms: 1_000,
        };

        assert_eq!(package.restore_plan().library_id, package.library_id);
        assert_eq!(package.restore_plan().media_root, "/Volumes/Sound Library");
    }

    #[test]
    fn restore_validation_rejects_missing_backup_file() {
        let package = test_package();

        assert_eq!(
            package.validate_restore_inputs(|_| false),
            Err(RestoreValidationError::MissingBackupFile)
        );
    }

    #[test]
    fn restore_validation_accepts_an_existing_backup_file() {
        let package = test_package();

        assert_eq!(package.validate_restore_inputs(|_| true), Ok(()));
    }

    #[test]
    fn create_backup_snapshots_the_catalog_into_the_backup_file_path() {
        let library_id = Uuid::new_v4();
        let source = BackupSource {
            catalog_path: "AppData/My Library.darkwave".to_string(),
            backup_file_path: "Backups/2026-07-30/My Library.darkwavebak".to_string(),
        };
        let mut copied = Vec::new();

        let package = create_backup(library_id, "/Volumes/SFX", &source, 1_700, |from, to| {
            copied.push((from.to_string(), to.to_string()));
            true
        })
        .expect("backup created");

        assert_eq!(package.library_id, library_id);
        assert_eq!(package.media_root, "/Volumes/SFX");
        assert_eq!(
            package.backup_file_path,
            "Backups/2026-07-30/My Library.darkwavebak"
        );
        assert_eq!(
            copied,
            vec![(
                "AppData/My Library.darkwave".to_string(),
                "Backups/2026-07-30/My Library.darkwavebak".to_string()
            )]
        );
    }

    #[test]
    fn create_backup_reports_failure_without_producing_a_package() {
        let source = BackupSource {
            catalog_path: "AppData/My Library.darkwave".to_string(),
            backup_file_path: "Backups/2026-07-30/My Library.darkwavebak".to_string(),
        };

        let result = create_backup(Uuid::new_v4(), "/Volumes/SFX", &source, 1_700, |_, _| false);

        assert_eq!(result, Err(BackupError::CatalogSnapshotFailed));
    }

    #[test]
    fn restore_plan_applies_the_catalog_copy() {
        let plan = test_package().restore_plan();
        let mut copied = Vec::new();

        let result = plan.apply("AppData/My Library.darkwave", |from, to| {
            copied.push((from.to_string(), to.to_string()));
            true
        });

        assert_eq!(result, Ok(()));
        assert_eq!(
            copied,
            vec![(
                "My Library.darkwavebak".to_string(),
                "AppData/My Library.darkwave".to_string()
            )]
        );
    }

    #[test]
    fn restore_plan_reports_catalog_copy_failure() {
        let plan = test_package().restore_plan();

        let result = plan.apply("AppData/My Library.darkwave", |_, _| false);

        assert_eq!(result, Err(RestoreError::CatalogRestoreFailed));
    }

    fn test_package() -> BackupPackage {
        BackupPackage {
            library_id: Uuid::new_v4(),
            media_root: "/library".to_string(),
            backup_file_path: "My Library.darkwavebak".to_string(),
            created_at_ms: 1_000,
        }
    }
}
