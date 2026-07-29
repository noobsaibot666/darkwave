use shared_types::{AssetIdentity, AvailabilityState, StorageMode};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum LibraryError {
    #[error("library name is required")]
    MissingName,
    #[error("media root is required")]
    MissingMediaRoot,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LibraryDraft {
    pub name: String,
    pub media_root: String,
}

pub fn product_codename() -> &'static str {
    "Darkwave"
}

pub fn validate_library_draft(draft: &LibraryDraft) -> Result<(), LibraryError> {
    if draft.name.trim().is_empty() {
        return Err(LibraryError::MissingName);
    }

    if draft.media_root.trim().is_empty() {
        return Err(LibraryError::MissingMediaRoot);
    }

    Ok(())
}

pub fn provisional_asset(original_filename: &str, storage_mode: StorageMode) -> AssetIdentity {
    let display_name = original_filename
        .rsplit_once('.')
        .map_or(original_filename, |(name, _)| name)
        .replace(['_', '-'], " ");

    AssetIdentity {
        id: Uuid::new_v4(),
        original_filename: original_filename.to_string(),
        display_name,
        storage_mode,
        availability_state: AvailabilityState::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn library_draft_requires_name() {
        let draft = LibraryDraft {
            name: " ".to_string(),
            media_root: "/Volumes/Sound Library".to_string(),
        };

        assert_eq!(
            validate_library_draft(&draft),
            Err(LibraryError::MissingName)
        );
    }

    #[test]
    fn provisional_asset_uses_filename_as_first_signal_only() {
        let asset = provisional_asset("dark-impact_03.wav", StorageMode::Managed);

        assert_eq!(asset.original_filename, "dark-impact_03.wav");
        assert_eq!(asset.display_name, "dark impact 03");
    }
}
