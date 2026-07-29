use thiserror::Error;

#[derive(Debug, Error, Eq, PartialEq)]
pub enum StorageError {
    #[error("path must be absolute")]
    RelativePath,
}

pub fn is_network_tolerant_catalog_path(path: &str) -> Result<bool, StorageError> {
    if !path.starts_with('/') && !path.contains(":\\") {
        return Err(StorageError::RelativePath);
    }

    Ok(!path.contains("/Volumes/") && !path.starts_with("\\\\"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nas_paths_are_not_valid_live_catalog_locations() {
        assert_eq!(
            is_network_tolerant_catalog_path("/Volumes/TrueNAS/catalog.sqlite"),
            Ok(false)
        );
    }
}
