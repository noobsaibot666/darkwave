use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SuggestionOrigin {
    Filename,
    Metadata,
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

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct EmbeddedAudioMetadata {
    pub title: Option<String>,
    pub genre: Option<String>,
    pub comment: Option<String>,
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
        .filter_map(|token| tag_for_token(token, 0.0))
        .map(|(name, facet, confidence)| TagSuggestion {
            name: name.to_string(),
            facet: facet.to_string(),
            confidence,
            origin: SuggestionOrigin::Filename,
        })
        .collect()
}

pub fn suggest_tags_from_embedded_metadata(metadata: &EmbeddedAudioMetadata) -> Vec<TagSuggestion> {
    let mut suggestions = Vec::new();

    if let Some(genre) = &metadata.genre {
        suggestions.extend(metadata_field_suggestions(genre, 0.1));
    }
    if let Some(title) = &metadata.title {
        suggestions.extend(metadata_field_suggestions(title, 0.0));
    }
    if let Some(comment) = &metadata.comment {
        suggestions.extend(metadata_field_suggestions(comment, -0.04));
    }

    suggestions
        .into_iter()
        .fold(Vec::new(), |mut deduped, next| {
            if let Some(existing) = deduped
                .iter_mut()
                .find(|existing| existing.name == next.name && existing.facet == next.facet)
            {
                existing.confidence = existing.confidence.max(next.confidence);
            } else {
                deduped.push(next);
            }
            deduped
        })
}

fn metadata_field_suggestions(value: &str, confidence_delta: f32) -> Vec<TagSuggestion> {
    tokenize_metadata_value(value)
        .iter()
        .filter_map(|token| tag_for_token(token, confidence_delta))
        .map(|(name, facet, confidence)| TagSuggestion {
            name: name.to_string(),
            facet: facet.to_string(),
            confidence,
            origin: SuggestionOrigin::Metadata,
        })
        .collect()
}

fn tag_for_token(token: &str, confidence_delta: f32) -> Option<(&'static str, &'static str, f32)> {
    let (name, facet, confidence) = match token {
        "impact" | "hit" => ("Impact", "action", 0.86),
        "whoosh" => ("Whoosh", "action", 0.9),
        "metal" | "metallic" => ("Metal", "source", 0.84),
        "bright" => ("Bright", "frequency", 0.78),
        "dark" => ("Dark", "character", 0.76),
        "short" => ("Short", "duration", 0.72),
        "cinematic" | "trailer" => ("Cinematic", "character", 0.82),
        "sfx" | "effect" | "effects" => ("Sound Effect", "media_type", 0.82),
        "ambience" | "ambient" => ("Ambience", "media_type", 0.84),
        "music" | "song" | "track" => ("Music", "media_type", 0.84),
        _ => return None,
    };

    Some((name, facet, (confidence + confidence_delta).clamp(0.0, 1.0)))
}

fn tokenize_metadata_value(value: &str) -> Vec<String> {
    value
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
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

    #[test]
    fn embedded_metadata_suggests_traceable_tags() {
        let metadata = EmbeddedAudioMetadata {
            title: Some("Dark metallic impact".to_string()),
            genre: Some("Cinematic Sound Effects".to_string()),
            comment: Some("short trailer hit".to_string()),
        };

        let suggestions = suggest_tags_from_embedded_metadata(&metadata);

        assert!(suggestions.iter().any(|suggestion| {
            suggestion.name == "Sound Effect"
                && suggestion.facet == "media_type"
                && suggestion.origin == SuggestionOrigin::Metadata
                && suggestion.confidence > 0.8
        }));
        assert!(suggestions.iter().any(|suggestion| {
            suggestion.name == "Impact" && suggestion.origin == SuggestionOrigin::Metadata
        }));
        assert!(suggestions
            .iter()
            .any(|suggestion| suggestion.name == "Cinematic"));
    }
}
