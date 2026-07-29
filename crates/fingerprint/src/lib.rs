pub fn exact_duplicate_key(content_hash: &str, file_size: u64) -> String {
    format!("{content_hash}:{file_size}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_key_combines_hash_and_size() {
        assert_eq!(exact_duplicate_key("abc", 42), "abc:42");
    }
}
