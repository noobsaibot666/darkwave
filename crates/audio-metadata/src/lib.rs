use std::path::Path;
use thiserror::Error;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileMetadata {
    pub extension: String,
    pub file_size: u64,
}

#[derive(Debug, Error)]
pub enum MetadataError {
    #[error("file has no extension")]
    MissingExtension,
    #[error("metadata read failed: {0}")]
    Io(#[from] std::io::Error),
}

pub fn supported_mvp_format(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "wav" | "aiff" | "aif" | "mp3" | "flac" | "aac" | "m4a" | "ogg"
    )
}

pub fn extract_immediate_metadata(path: impl AsRef<Path>) -> Result<FileMetadata, MetadataError> {
    let path = path.as_ref();
    let extension = path
        .extension()
        .and_then(|extension| extension.to_str())
        .ok_or(MetadataError::MissingExtension)?
        .to_ascii_lowercase();
    let metadata = std::fs::metadata(path)?;

    Ok(FileMetadata {
        extension,
        file_size: metadata.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn mvp_supports_required_formats() {
        for extension in ["wav", "aiff", "mp3", "flac", "m4a", "ogg"] {
            assert!(supported_mvp_format(extension));
        }
    }

    #[test]
    fn extracts_immediate_file_metadata_without_decoding_audio() {
        let mut path = std::env::temp_dir();
        path.push(format!("darkwave-metadata-{}.wav", Uuid::new_v4()));
        fs::write(&path, b"audio fixture").expect("fixture");

        let metadata = extract_immediate_metadata(&path).expect("metadata");

        assert_eq!(metadata.extension, "wav");
        assert_eq!(metadata.file_size, 13);
    }
}
