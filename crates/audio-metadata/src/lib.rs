#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FileMetadata {
    pub extension: String,
    pub file_size: u64,
}

pub fn supported_mvp_format(extension: &str) -> bool {
    matches!(
        extension.to_ascii_lowercase().as_str(),
        "wav" | "aiff" | "aif" | "mp3" | "flac" | "aac" | "m4a" | "ogg"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mvp_supports_required_formats() {
        for extension in ["wav", "aiff", "mp3", "flac", "m4a", "ogg"] {
            assert!(supported_mvp_format(extension));
        }
    }
}
