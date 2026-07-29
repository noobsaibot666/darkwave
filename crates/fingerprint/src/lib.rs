pub fn exact_duplicate_key(content_hash: &str, file_size: u64) -> String {
    format!("{content_hash}:{file_size}")
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioFingerprint {
    bits: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DuplicateClassification {
    EquivalentDuplicate,
    RelatedVariant,
    Distinct,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DuplicateReviewAction {
    KeepBoth,
    LinkAsVariants,
    MergeMetadata,
    ReplaceLowerQuality,
    MoveDuplicateToTrash,
}

impl AudioFingerprint {
    pub fn from_bits(bits: u64) -> Self {
        Self { bits }
    }

    pub fn hamming_distance(&self, other: &Self) -> u32 {
        (self.bits ^ other.bits).count_ones()
    }
}

pub fn classify_fingerprint_match(
    first: &AudioFingerprint,
    second: &AudioFingerprint,
    equivalent_threshold: u32,
) -> DuplicateClassification {
    let distance = first.hamming_distance(second);

    if distance <= equivalent_threshold {
        DuplicateClassification::EquivalentDuplicate
    } else if distance <= equivalent_threshold.saturating_mul(2) {
        DuplicateClassification::RelatedVariant
    } else {
        DuplicateClassification::Distinct
    }
}

pub fn duplicate_review_actions() -> Vec<DuplicateReviewAction> {
    vec![
        DuplicateReviewAction::KeepBoth,
        DuplicateReviewAction::LinkAsVariants,
        DuplicateReviewAction::MergeMetadata,
        DuplicateReviewAction::ReplaceLowerQuality,
        DuplicateReviewAction::MoveDuplicateToTrash,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duplicate_key_combines_hash_and_size() {
        assert_eq!(exact_duplicate_key("abc", 42), "abc:42");
    }

    #[test]
    fn equivalent_duplicate_allows_small_fingerprint_distance() {
        let first = AudioFingerprint::from_bits(0b1010_1010);
        let second = AudioFingerprint::from_bits(0b1010_1110);

        assert_eq!(first.hamming_distance(&second), 1);
        assert_eq!(
            classify_fingerprint_match(&first, &second, 2),
            DuplicateClassification::EquivalentDuplicate
        );
    }

    #[test]
    fn related_variant_uses_wider_fingerprint_distance() {
        let first = AudioFingerprint::from_bits(0b1111_0000);
        let second = AudioFingerprint::from_bits(0b1100_0011);

        assert_eq!(
            classify_fingerprint_match(&first, &second, 2),
            DuplicateClassification::RelatedVariant
        );
    }

    #[test]
    fn distant_fingerprints_are_not_duplicates() {
        let first = AudioFingerprint::from_bits(0b1111_1111);
        let second = AudioFingerprint::from_bits(0b0000_0000);

        assert_eq!(
            classify_fingerprint_match(&first, &second, 2),
            DuplicateClassification::Distinct
        );
    }

    #[test]
    fn duplicate_review_options_are_non_destructive() {
        assert_eq!(
            duplicate_review_actions(),
            vec![
                DuplicateReviewAction::KeepBoth,
                DuplicateReviewAction::LinkAsVariants,
                DuplicateReviewAction::MergeMetadata,
                DuplicateReviewAction::ReplaceLowerQuality,
                DuplicateReviewAction::MoveDuplicateToTrash
            ]
        );
    }
}
