//! Recognizes the "separated-instrument stems" naming convention several
//! stock-music sites (Epidemic Sound's `ES_` catalog, in particular) use
//! for the extra downloads that accompany a track's full mix:
//!
//! ```text
//! ES_Go Lucky - Heyson.mp3                    <- the full mix
//! ES_Go Lucky STEMS BASS - Heyson.mp3         <- one part
//! ES_Go Lucky STEMS DRUMS - Heyson.mp3        <- another part
//! ES_Go Lucky STEMS INSTRUMENTS - Heyson.mp3
//! ES_Go Lucky STEMS MELODY - Heyson.mp3
//! ```
//!
//! Stripping " STEMS <LABEL>" out of a stem's filename reconstructs the
//! full mix's own filename exactly — that's the whole detection: no
//! filename-similarity scoring, no configuration, just this one very
//! reliable structural marker. See `docs/samples` for a real example this
//! was built against.

use uuid::Uuid;

/// One file considered for grouping: an opaque id (the caller's asset id —
/// this module never looks at it, just carries it through) plus the
/// filename to pattern-match against.
pub type StemCandidate = (Uuid, String);

/// One resolved group: `(asset_id, display_label, is_primary)` for every
/// member, exactly one of which has `is_primary = true` — the row a
/// caller should show in the main asset list, with the rest reachable
/// only through that row's own "Stems" panel.
pub type StemGroup = Vec<(Uuid, String, bool)>;

/// Scans `files` for the STEMS naming pattern and groups whatever matches.
/// `files` is typically one freshly-imported folder's worth of files, or a
/// whole library's un-grouped assets for a retroactive pass — either way,
/// just the (id, filename) pairs to consider together; unrelated files
/// mixed into the same call are harmless, they simply never match.
///
/// A lone stem with no siblings (no full mix present, no other stem
/// sharing its reconstructed base name) doesn't form a group by itself —
/// grouping is only about relating 2+ files, not about noticing that one
/// file happens to have "STEMS" in its name.
pub fn detect_stem_groups(files: &[StemCandidate]) -> Vec<StemGroup> {
    use std::collections::HashMap;

    // Exact filename (lowercased) -> id, so a stem's reconstructed
    // full-mix filename can be looked up directly.
    let by_filename: HashMap<String, Uuid> = files
        .iter()
        .map(|(id, name)| (name.to_lowercase(), *id))
        .collect();

    // Reconstructed full-mix filename (lowercased) -> the stems that
    // imply it.
    let mut candidates: HashMap<String, Vec<(Uuid, String)>> = HashMap::new();
    for (id, name) in files {
        if let Some((implied_main, label)) = split_stem_filename(name) {
            candidates
                .entry(implied_main.to_lowercase())
                .or_default()
                .push((*id, label));
        }
    }

    let mut groups: Vec<StemGroup> = candidates
        .into_iter()
        .filter_map(|(implied_main, mut stems)| {
            let mut members: StemGroup = Vec::new();
            if let Some(&main_id) = by_filename.get(&implied_main) {
                members.push((main_id, "Full Mix".to_string(), true));
            } else {
                // No actual full mix in this batch — promote one stem
                // (stable, alphabetical by label) to stand in as the row
                // the browser shows.
                stems.sort_by(|a, b| a.1.cmp(&b.1));
                let (id, label) = stems.remove(0);
                members.push((id, label, true));
            }
            for (id, label) in stems {
                members.push((id, label, false));
            }
            // Exactly one file (a promoted stem with nothing left after
            // taking the "primary" slot) isn't a real group.
            if members.len() < 2 {
                None
            } else {
                Some(members)
            }
        })
        .collect();

    // Deterministic output order (by primary member's id) — callers that
    // persist results shouldn't see HashMap-order churn between runs.
    groups.sort_by_key(|members| members[0].0);
    groups
}

/// If `filename` matches `<base> STEMS <LABEL> - <rest>` or, just as
/// common in practice, `<base> STEMS <LABEL>.<ext>` (no artist suffix at
/// all), returns (the implied full-mix filename, `<LABEL>`) — e.g.
/// `"Song STEMS DRUMS - Band.mp3"` -> `("Song - Band.mp3", "Drums")`,
/// `"Song STEMS DRUMS.mp3"` -> `("Song.mp3", "Drums")`. The `STEMS`
/// marker itself is matched case-insensitively (ASCII only, so this can't
/// misalign across a multi-byte UTF-8 character — every byte in the
/// marker is plain ASCII, which can only ever match other ASCII bytes at
/// the same position); everything else is passed through byte-for-byte.
fn split_stem_filename(filename: &str) -> Option<(String, String)> {
    const MARKER: &str = " STEMS ";
    let marker_pos = find_ascii_case_insensitive(filename, MARKER)?;
    let prefix = &filename[..marker_pos];
    let after = &filename[marker_pos + MARKER.len()..];

    // The label is whatever sits before the next " - " (the separator
    // leading into the artist/rest of the filename) — typically one or
    // two short words: DRUMS, MELODY, FULL MIX, VOX... When there's no
    // " - " at all, the label runs up to the file extension instead (a
    // real, common case: many stems ship as just "<title> STEMS
    // <LABEL>.<ext>" with no artist suffix).
    let (label, rest) = match after.find(" - ") {
        Some(dash_pos) => (after[..dash_pos].trim(), &after[dash_pos..]),
        None => {
            let ext_pos = after.rfind('.')?;
            (after[..ext_pos].trim(), &after[ext_pos..])
        }
    };
    if label.is_empty() {
        return None;
    }

    Some((format!("{prefix}{rest}"), title_case(label)))
}

fn find_ascii_case_insensitive(haystack: &str, needle: &str) -> Option<usize> {
    let haystack = haystack.as_bytes();
    let needle = needle.as_bytes();
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    (0..=haystack.len() - needle.len()).find(|&start| haystack[start..start + needle.len()].eq_ignore_ascii_case(needle))
}

fn title_case(input: &str) -> String {
    input
        .split_whitespace()
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + &chars.as_str().to_lowercase(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_a_full_mix_with_its_stems() {
        let main = Uuid::new_v4();
        let bass = Uuid::new_v4();
        let drums = Uuid::new_v4();
        let melody = Uuid::new_v4();
        let files = vec![
            (main, "ES_Go Lucky - Heyson.mp3".to_string()),
            (bass, "ES_Go Lucky STEMS BASS - Heyson.mp3".to_string()),
            (drums, "ES_Go Lucky STEMS DRUMS - Heyson.mp3".to_string()),
            (melody, "ES_Go Lucky STEMS MELODY - Heyson.mp3".to_string()),
        ];

        let groups = detect_stem_groups(&files);
        assert_eq!(groups.len(), 1);
        let group = &groups[0];
        assert_eq!(group.len(), 4);

        let primary: Vec<_> = group.iter().filter(|(_, _, is_primary)| *is_primary).collect();
        assert_eq!(primary.len(), 1);
        assert_eq!(primary[0].0, main);
        assert_eq!(primary[0].1, "Full Mix");

        let labels: std::collections::HashSet<_> = group
            .iter()
            .filter(|(_, _, is_primary)| !is_primary)
            .map(|(_, label, _)| label.as_str())
            .collect();
        assert_eq!(
            labels,
            std::collections::HashSet::from(["Bass", "Drums", "Melody"])
        );
    }

    #[test]
    fn groups_stems_even_without_the_full_mix_present() {
        let drums = Uuid::new_v4();
        let melody = Uuid::new_v4();
        let files = vec![
            (drums, "Song STEMS DRUMS - Artist.wav".to_string()),
            (melody, "Song STEMS MELODY - Artist.wav".to_string()),
        ];

        let groups = detect_stem_groups(&files);
        assert_eq!(groups.len(), 1);
        let primaries: Vec<_> = groups[0].iter().filter(|(_, _, is_primary)| *is_primary).collect();
        assert_eq!(primaries.len(), 1, "one stem stands in as the primary when no full mix exists");
        // Alphabetically first label wins the promotion, deterministically.
        assert_eq!(primaries[0].0, drums);
    }

    #[test]
    fn a_lone_file_with_stems_in_its_name_but_no_sibling_is_not_grouped() {
        let only = Uuid::new_v4();
        let files = vec![(only, "Song STEMS DRUMS - Artist.wav".to_string())];
        assert_eq!(detect_stem_groups(&files), Vec::<StemGroup>::new());
    }

    #[test]
    fn unrelated_files_are_ignored() {
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        let files = vec![
            (a, "Rain Ambience.wav".to_string()),
            (b, "Door Slam.wav".to_string()),
        ];
        assert_eq!(detect_stem_groups(&files), Vec::<StemGroup>::new());
    }

    #[test]
    fn matching_is_case_insensitive_on_the_stems_marker() {
        let main = Uuid::new_v4();
        let drum = Uuid::new_v4();
        let files = vec![
            (main, "Track - Band.mp3".to_string()),
            (drum, "Track stems Drums - Band.mp3".to_string()),
        ];
        let groups = detect_stem_groups(&files);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }

    #[test]
    fn groups_stems_with_no_artist_suffix_at_all() {
        // A real variant found scattered in this app's own test library:
        // no " - <artist>" suffix, the label just runs up to the
        // extension. "1AM OMW STEMS MELODY.mp3" -> main "1AM OMW.mp3".
        let main = Uuid::new_v4();
        let melody = Uuid::new_v4();
        let files = vec![
            (main, "1AM OMW.mp3".to_string()),
            (melody, "1AM OMW STEMS MELODY.mp3".to_string()),
        ];
        let groups = detect_stem_groups(&files);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
        let primary = groups[0].iter().find(|(_, _, is_primary)| *is_primary).expect("primary");
        assert_eq!(primary.0, main);
    }

    #[test]
    fn a_file_with_no_extension_and_no_dash_after_stems_is_ignored() {
        // Neither a " - <rest>" suffix nor a "." to fall back to — nothing
        // to reconstruct a main filename from, so it's left alone rather
        // than guessed at.
        let only = Uuid::new_v4();
        let files = vec![(only, "Weird STEMS Naming".to_string())];
        assert_eq!(detect_stem_groups(&files), Vec::<StemGroup>::new());
    }
}
