pub fn should_ignore_watched_file(filename: &str) -> bool {
    let lower = filename.to_ascii_lowercase();
    lower.ends_with(".crdownload")
        || lower.ends_with(".download")
        || lower.ends_with(".part")
        || lower.ends_with(".tmp")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn incomplete_browser_downloads_are_ignored() {
        assert!(should_ignore_watched_file("track.wav.crdownload"));
        assert!(!should_ignore_watched_file("track.wav"));
    }
}
