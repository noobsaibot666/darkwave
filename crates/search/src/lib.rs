use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SuggestionOrigin {
    Filename,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TagSuggestion {
    pub name: String,
    pub facet: String,
    pub confidence: f32,
    pub origin: SuggestionOrigin,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ParsedFilename {
    pub display_name: String,
    pub tokens: Vec<String>,
    pub bpm: Option<u16>,
    pub musical_key: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct VisibleFilter {
    pub field: String,
    pub operator: String,
    pub value: String,
}

pub fn explain_text_query(query: &str) -> Vec<VisibleFilter> {
    query
        .split_whitespace()
        .filter(|token| !token.is_empty())
        .map(|token| VisibleFilter {
            field: "text".to_string(),
            operator: "contains".to_string(),
            value: token.to_string(),
        })
        .collect()
}

pub fn parse_audio_filename(filename: &str) -> ParsedFilename {
    let stem = filename.rsplit_once('.').map_or(filename, |(stem, _)| stem);
    let raw_tokens = stem
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty());
    let mut tokens = Vec::new();
    let mut bpm = None;
    let mut musical_key = None;

    for token in raw_tokens {
        let lower = token.to_ascii_lowercase();
        if let Some(value) = lower
            .strip_suffix("bpm")
            .and_then(|value| value.parse::<u16>().ok())
        {
            bpm = Some(value);
            continue;
        }
        if is_key_token(token) {
            musical_key = Some(token.to_string());
            continue;
        }
        if lower.starts_with("vendor")
            || lower == "pack"
            || lower.chars().all(|c| c.is_ascii_digit())
        {
            continue;
        }
        tokens.push(lower);
    }

    ParsedFilename {
        display_name: tokens.join(" "),
        tokens,
        bpm,
        musical_key,
    }
}

pub fn suggest_tags_from_filename(filename: &str) -> Vec<TagSuggestion> {
    let parsed = parse_audio_filename(filename);
    parsed
        .tokens
        .iter()
        .filter_map(|token| match token.as_str() {
            "impact" | "hit" => Some(("Impact", "action", 0.86)),
            "whoosh" => Some(("Whoosh", "action", 0.9)),
            "metal" | "metallic" => Some(("Metal", "source", 0.84)),
            "bright" => Some(("Bright", "frequency", 0.78)),
            "dark" => Some(("Dark", "character", 0.76)),
            "short" => Some(("Short", "duration", 0.72)),
            _ => None,
        })
        .map(|(name, facet, confidence)| TagSuggestion {
            name: name.to_string(),
            facet: facet.to_string(),
            confidence,
            origin: SuggestionOrigin::Filename,
        })
        .collect()
}

fn is_key_token(token: &str) -> bool {
    matches!(
        token,
        "A" | "B" | "C" | "D" | "E" | "F" | "G" | "Am" | "Bm" | "Cm" | "Dm" | "Em" | "Fm" | "Gm"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_terms_are_visible_as_filters() {
        let filters = explain_text_query("dark metallic impact");

        assert_eq!(filters.len(), 3);
        assert_eq!(filters[0].value, "dark");
    }

    #[test]
    fn filename_parser_extracts_clean_name_bpm_key_and_vocabulary() {
        let parsed = parse_audio_filename("VendorPack_128bpm_Am_dark-metal-impact-03.wav");

        assert_eq!(parsed.display_name, "dark metal impact");
        assert_eq!(parsed.bpm, Some(128));
        assert_eq!(parsed.musical_key, Some("Am".to_string()));
        assert!(parsed.tokens.contains(&"dark".to_string()));
        assert!(parsed.tokens.contains(&"impact".to_string()));
    }

    #[test]
    fn filename_parser_suggests_traceable_tags() {
        let suggestions = suggest_tags_from_filename("short_bright_whoosh_metal.wav");

        assert!(suggestions.iter().any(|suggestion| {
            suggestion.name == "Whoosh"
                && suggestion.facet == "action"
                && suggestion.origin == SuggestionOrigin::Filename
                && suggestion.confidence > 0.7
        }));
        assert!(suggestions
            .iter()
            .any(|suggestion| suggestion.name == "Metal"));
    }
}
