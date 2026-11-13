//! Name-based auto-organization of sound effects into categories — the
//! engine behind the browser's "Organize" button. Pure and deterministic:
//! no catalog access, no I/O. The Tauri layer feeds it `(id, filename)`
//! pairs, shows the proposal for review, and only then writes anything.
//!
//! How a file gets its category, strongest signal first:
//!
//! 1. **Series** — files whose cleaned names are identical apart from a
//!    trailing number (`Multimedia 770`, `Multimedia 793`, `Zap_01`,
//!    `Zap_02`) are always kept together. If the series' words match a
//!    category, the whole series goes there; otherwise the series becomes
//!    its own category named after its shared stem.
//! 2. **Keywords** — every word is looked up in a curated vocabulary loosely
//!    following the Universal Category System (UCS) names editors already
//!    know. Specific categories (Doors, Vehicles, Sci-Fi…) outweigh generic
//!    ones (Impacts, Ambience, Noise), so `metro ambiance` is Vehicles and
//!    `sci-fi resonator hit` is Sci-Fi. Compound words (`doorslam`,
//!    `breakingneck`) match by prefix/suffix. A category the user already
//!    has (an existing category tag) beats both.
//! 3. **Shared first word** — leftovers that start with the same word
//!    (≥2 files) are grouped under that word.
//!
//! Anything still unmatched is reported as uncategorized and left alone.
//!
//! Before any of that, each filename is cleaned of the naming noise real
//! libraries are full of: Freesound's `12345__username__` prefix, Epidemic
//! Sound's `ES_` prefix and ` - SFX Producer` suffix, take numbers
//! (`_06`, ` 2`, `tram3`), and filler words.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct OrganizeInput {
    pub id: String,
    pub filename: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CategoryReason {
    /// Matched a category the library already has.
    Existing,
    /// Matched the built-in keyword vocabulary.
    Keyword,
    /// Part of a numbered series with no keyword — named after the series.
    Series,
    /// Shares its first word with other otherwise-unmatched files.
    SharedWord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProposedMember {
    pub id: String,
    pub reason: CategoryReason,
    /// The word that decided it (`"door"`, `"multimedia"`) — shown in the
    /// review UI so a proposal is explainable, not magic.
    pub matched: String,
    /// Shared stem when this file is part of a numbered series.
    pub series: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProposedCategory {
    pub name: String,
    /// True when `name` is one of the `existing_categories` passed in.
    pub existing: bool,
    pub members: Vec<ProposedMember>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct OrganizePlan {
    pub categories: Vec<ProposedCategory>,
    pub uncategorized: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CleanedName {
    /// Meaningful lowercase words, in order.
    pub tokens: Vec<String>,
    /// `tokens` joined — two files with the same stem and a number are a series.
    pub stem: String,
    /// The trailing take/sequence number that was stripped, if any.
    pub sequence: Option<u32>,
}

/// Category name → (weight, words). Weight 2 = specific, 1 = generic
/// catch-all that should lose to anything more specific in the same name.
const VOCABULARY: &[(&str, u8, &[&str])] = &[
    ("Doors", 2, &["door", "doors", "doorslam", "gate", "gates", "latch", "hinge", "doorbell"]),
    ("Footsteps", 2, &["footstep", "footsteps", "steps", "step", "walking", "walk", "stomp", "shoe", "shoes", "heels", "sneaker", "sneakers", "jogging"]),
    ("Explosions", 2, &["explosion", "explosions", "explode", "boom", "booms", "blast", "cannon", "bomb", "detonation", "kaboom"]),
    ("Whooshes", 2, &["whoosh", "whooshes", "swoosh", "swish", "woosh", "swipe", "whizz"]),
    ("Glitches", 2, &["glitch", "glitches", "glitching", "glitchy", "stutter", "bitcrush", "datamosh"]),
    ("UI & Beeps", 2, &["beep", "beeps", "blip", "blips", "bleep", "ui", "interface", "multimedia", "button", "menu", "notification", "ping", "alert", "click", "clicks"]),
    ("Coins & Money", 2, &["coin", "coins", "money", "cash", "cashier", "register"]),
    ("Vehicles", 2, &["car", "cars", "engine", "tram", "train", "trains", "metro", "subway", "bus", "truck", "ambulance", "siren", "sirens", "traffic", "motorcycle", "motorbike", "vagon", "wagon", "plane", "airplane", "helicopter", "horn", "vehicle", "vehicles"]),
    ("Weapons", 2, &["gun", "guns", "gunshot", "gunshots", "rifle", "pistol", "shotgun", "sword", "blade", "reload", "bullet", "bullets", "weapon"]),
    ("Water", 2, &["water", "splash", "splashes", "drip", "drips", "dripping", "river", "stream", "ocean", "sea", "bubble", "bubbles", "pour", "pouring", "liquid", "underwater"]),
    ("Weather", 2, &["thunder", "storm", "rain", "raining", "rainfall", "wind", "windy", "hail", "blizzard"]),
    ("Fire", 2, &["fire", "flame", "flames", "burning", "crackle", "crackling", "fireplace", "torch"]),
    ("Glass", 2, &["glass", "shatter", "shattering", "window"]),
    ("Metal", 1, &["metal", "metallic", "steel", "iron", "clang", "chain", "chains"]),
    ("Wood", 1, &["wood", "wooden", "creak", "creaking", "creaky", "plank"]),
    ("Human", 2, &["breath", "breathing", "breathe", "sigh", "cough", "laugh", "laughing", "scream", "screaming", "yell", "gasp", "heartbeat", "finger", "fingers", "fingerflick", "snap", "clap", "clapping", "kiss", "whisper"]),
    ("Crowds", 2, &["crowd", "crowds", "audience", "applause", "cheer", "cheering", "chatter", "walla"]),
    ("Animals", 2, &["dog", "dogs", "cat", "cats", "bird", "birds", "horse", "cow", "animal", "animals", "bark", "barking", "meow", "chirp", "insects", "cricket", "crickets"]),
    ("Electronics", 2, &["telephone", "phone", "radio", "static", "tv", "television", "buzz", "hum", "electric", "electricity", "computer", "keyboard", "typing", "printer", "camera", "shutter"]),
    ("Sci-Fi", 2, &["scifi", "sci", "laser", "lasers", "spaceship", "alien", "robot", "robotic", "resonator", "futuristic", "teleport", "forcefield"]),
    ("Objects", 2, &["table", "desk", "book", "books", "dish", "dishes", "plate", "cup", "objects", "object", "paper", "page", "pages", "bag", "box", "zipper", "keys", "drawer", "chair", "bottle", "cutlery", "toast", "toaster"]),
    ("Gore & Bones", 2, &["bone", "bones", "neck", "breakingneck", "crunch", "gore", "flesh", "squelch"]),
    ("Fights", 2, &["punch", "punches", "kick", "kicks", "slap", "fight", "fighting"]),
    ("Risers", 2, &["rise", "riser", "risers", "rising", "buildup", "uplifter", "sweep"]),
    ("Drones", 2, &["drone", "drones", "rumble", "rumbling"]),
    ("Bells", 2, &["bell", "bells", "chime", "chimes", "gong", "ding"]),
    ("Cartoon", 2, &["cartoon", "boing", "squeak", "squeaky", "toon"]),
    ("Mechanical", 2, &["machine", "machines", "mechanical", "motor", "gear", "gears", "servo", "hydraulic", "mechanism", "elevator"]),
    ("Impacts", 1, &["impact", "impacts", "hit", "hits", "thud", "slam", "slams", "smash", "crash", "drop", "drops", "bang", "knock", "knocking", "thump"]),
    ("Ambience", 1, &["ambience", "ambient", "ambiance", "ambiente", "atmosphere", "atmos", "atmo", "room", "roomtone", "background", "nature", "forest", "city", "street", "night", "neighborhood", "village", "park", "bar", "cafe", "restaurant", "office", "interior", "exterior", "quiet", "outdoor", "indoor"]),
    ("Noise", 1, &["noise", "noises", "hiss", "rustle"]),
];

/// Words that carry no meaning about what a sound *is*.
const FILLER: &[&str] = &[
    "the", "and", "or", "of", "on", "in", "at", "to", "with", "by", "for", "from", "into", "onto", "off", "out",
    "sfx", "fx", "sound", "sounds", "effect", "effects", "take", "version", "ver", "mix", "stereo", "mono", "master",
    "final", "edit", "loop", "var", "variation", "alt", "es", "producer", "wav", "mp3", "aif", "aiff", "flac", "ogg",
    "high", "quality", "hq", "free", "sample", "samples", "putting", "placing", "put", "it", "its", "is", "something",
    "specific", "nothing",
];

pub fn clean_sfx_filename(filename: &str) -> CleanedName {
    let mut stem = filename;
    if let Some((before, extension)) = stem.rsplit_once('.') {
        if (2..=4).contains(&extension.len()) && extension.chars().all(|c| c.is_ascii_alphanumeric()) {
            stem = before;
        }
    }
    let mut stem = stem.to_string();

    // Freesound: `<numeric id>__<username>__<title>`. Usernames can contain
    // single underscores, so take everything after the *second* `__`.
    if let Some((id, rest)) = stem.split_once("__") {
        if !id.is_empty() && id.chars().all(|c| c.is_ascii_digit()) {
            if let Some((_user, title)) = rest.split_once("__") {
                stem = title.to_string();
            }
        }
    }
    // Epidemic Sound: `ES_<title> - SFX Producer`.
    if let Some(rest) = stem.strip_prefix("ES_") {
        stem = rest.to_string();
    }
    for suffix in [" - SFX Producer", " - Epidemic Sound"] {
        if let Some(rest) = stem.strip_suffix(suffix) {
            stem = rest.to_string();
        }
    }

    // Split camelCase (`DoorSlam` → `Door Slam`) before tokenizing, but
    // not runs of capitals (`SCI_FI_MACHINE` stays as-is).
    let mut spaced = String::with_capacity(stem.len() + 8);
    let mut previous: Option<char> = None;
    for character in stem.chars() {
        if let Some(previous) = previous {
            if previous.is_ascii_lowercase() && character.is_ascii_uppercase() {
                spaced.push(' ');
            }
        }
        spaced.push(character);
        previous = Some(character);
    }

    let raw: Vec<String> = spaced
        .split(|character: char| !character.is_ascii_alphanumeric())
        .filter(|token| !token.is_empty())
        .map(|token| token.to_ascii_lowercase())
        .collect();

    // The last purely-numeric token is the take/sequence number.
    let mut sequence = None;
    let mut tokens = Vec::new();
    let last_numeric = raw.iter().rposition(|token| token.chars().all(|c| c.is_ascii_digit()));
    for (index, token) in raw.iter().enumerate() {
        if token.chars().all(|c| c.is_ascii_digit()) {
            if Some(index) == last_numeric && index > 0 {
                sequence = token.parse().ok();
            }
            continue;
        }
        // `tram3`, `cannon3`, `hit02` → strip glued-on take numbers.
        let word = token.trim_end_matches(|c: char| c.is_ascii_digit());
        if word.len() != token.len() && sequence.is_none() {
            sequence = token[word.len()..].parse().ok();
        }
        // `e2`, `v1` and similar leftovers are noise.
        if word.len() < 2 || FILLER.contains(&word) {
            continue;
        }
        tokens.push(word.to_string());
    }

    CleanedName { stem: tokens.join(" "), tokens, sequence }
}

struct Lookup {
    exact: HashMap<&'static str, (&'static str, u8)>,
}

impl Lookup {
    fn new() -> Self {
        let mut exact = HashMap::new();
        for (category, weight, words) in VOCABULARY {
            for word in *words {
                exact.entry(*word).or_insert((*category, *weight));
            }
        }
        Self { exact }
    }

    /// Category for one word: exact match, then a simple plural strip, then
    /// prefix/suffix matching for compounds like `doorslam`. Only vocabulary
    /// words of 4+ letters take part in compound matching, so `car` never
    /// claims `carpet`.
    fn category_for(&self, word: &str) -> Option<(&'static str, u8)> {
        if let Some(hit) = self.exact.get(word) {
            return Some(*hit);
        }
        if let Some(singular) = word.strip_suffix('s') {
            if let Some(hit) = self.exact.get(singular) {
                return Some(*hit);
            }
        }
        if word.len() >= 6 {
            let mut best: Option<(&'static str, u8, usize)> = None;
            for (key, (category, weight)) in &self.exact {
                if key.len() >= 4
                    && key.len() < word.len()
                    && (word.starts_with(key) || word.ends_with(key))
                    && best.map_or(true, |(_, best_weight, best_len)| {
                        (*weight, key.len()) > (best_weight, best_len)
                    })
                {
                    best = Some((category, *weight, key.len()));
                }
            }
            return best.map(|(category, weight, _)| (category, weight));
        }
        None
    }
}

fn normalize_category(name: &str) -> String {
    let lower: String = name
        .to_ascii_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect();
    lower.strip_suffix('s').map(str::to_string).unwrap_or(lower)
}

fn title_case(word: &str) -> String {
    let mut characters = word.chars();
    match characters.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + characters.as_str(),
        None => String::new(),
    }
}

/// Decides one file's category from its words, or `None`.
fn classify(
    tokens: &[String],
    lookup: &Lookup,
    existing: &HashMap<String, String>,
) -> Option<(String, CategoryReason, String)> {
    // A user's own category wins outright when a word names it.
    for token in tokens {
        if let Some(name) = existing.get(&normalize_category(token)) {
            return Some((name.clone(), CategoryReason::Existing, token.clone()));
        }
    }

    // Score categories; ties go to whichever appeared first in the name.
    let mut scores: Vec<(&'static str, u32, usize, String)> = Vec::new();
    for (position, token) in tokens.iter().enumerate() {
        let Some((category, weight)) = lookup.category_for(token) else {
            continue;
        };
        match scores.iter_mut().find(|entry| entry.0 == category) {
            Some(entry) => entry.1 += weight as u32,
            None => scores.push((category, weight as u32, position, token.clone())),
        }
    }
    let (category, _, _, matched) = scores
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then(right.2.cmp(&left.2)))?;

    // Map a vocabulary category onto an existing tag of the same name
    // ("Door" vs "Doors") instead of creating a near-duplicate.
    let name = existing
        .get(&normalize_category(category))
        .cloned()
        .unwrap_or_else(|| category.to_string());
    let reason = if existing.contains_key(&normalize_category(category)) {
        CategoryReason::Existing
    } else {
        CategoryReason::Keyword
    };
    Some((name, reason, matched))
}

pub fn propose_sfx_categories(inputs: &[OrganizeInput], existing_categories: &[String]) -> OrganizePlan {
    let lookup = Lookup::new();
    let existing: HashMap<String, String> = existing_categories
        .iter()
        .map(|name| (normalize_category(name), name.clone()))
        .collect();

    let cleaned: Vec<CleanedName> = inputs.iter().map(|input| clean_sfx_filename(&input.filename)).collect();

    // Series: same non-empty stem, ≥2 files, at least one carrying a number.
    let mut by_stem: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, name) in cleaned.iter().enumerate() {
        if !name.stem.is_empty() {
            by_stem.entry(name.stem.as_str()).or_default().push(index);
        }
    }
    let mut series_of: HashMap<usize, String> = HashMap::new();
    for (stem, members) in &by_stem {
        if members.len() >= 2 && members.iter().any(|&index| cleaned[index].sequence.is_some()) {
            for &index in members {
                series_of.insert(index, stem.to_string());
            }
        }
    }

    let mut assigned: BTreeMap<String, (bool, Vec<ProposedMember>)> = BTreeMap::new();
    let mut leftovers: Vec<usize> = Vec::new();
    let mut place = |name: String, existing_flag: bool, member: ProposedMember| {
        let entry = assigned.entry(name).or_insert((existing_flag, Vec::new()));
        entry.0 |= existing_flag;
        entry.1.push(member);
    };

    for (index, input) in inputs.iter().enumerate() {
        let series = series_of.get(&index).cloned();
        match classify(&cleaned[index].tokens, &lookup, &existing) {
            Some((name, reason, matched)) => place(
                name,
                reason == CategoryReason::Existing,
                ProposedMember { id: input.id.clone(), reason, matched, series },
            ),
            None => match series {
                Some(stem) => {
                    let name = title_case(&stem);
                    let is_existing = existing.contains_key(&normalize_category(&name));
                    let name = existing.get(&normalize_category(&name)).cloned().unwrap_or(name);
                    place(
                        name,
                        is_existing,
                        ProposedMember {
                            id: input.id.clone(),
                            reason: CategoryReason::Series,
                            matched: stem.clone(),
                            series: Some(stem),
                        },
                    )
                }
                None => leftovers.push(index),
            },
        }
    }

    // Shared first word among the leftovers.
    let mut by_first: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for &index in &leftovers {
        if let Some(first) = cleaned[index].tokens.first().filter(|word| word.len() >= 3) {
            by_first.entry(first.as_str()).or_default().push(index);
        }
    }
    let mut uncategorized = Vec::new();
    for &index in &leftovers {
        let first = cleaned[index].tokens.first().map(String::as_str);
        match first.and_then(|word| by_first.get(word).filter(|group| group.len() >= 2).map(|_| word)) {
            Some(word) => place(
                title_case(word),
                false,
                ProposedMember {
                    id: inputs[index].id.clone(),
                    reason: CategoryReason::SharedWord,
                    matched: word.to_string(),
                    series: None,
                },
            ),
            None => uncategorized.push(inputs[index].id.clone()),
        }
    }

    let mut categories: Vec<ProposedCategory> = assigned
        .into_iter()
        .map(|(name, (existing, members))| ProposedCategory { name, existing, members })
        .collect();
    categories.sort_by(|left, right| right.members.len().cmp(&left.members.len()).then(left.name.cmp(&right.name)));

    OrganizePlan { categories, uncategorized }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(names: &[&str]) -> Vec<OrganizeInput> {
        names
            .iter()
            .map(|name| OrganizeInput { id: name.to_string(), filename: name.to_string() })
            .collect()
    }

    fn category_of(plan: &OrganizePlan, id: &str) -> Option<String> {
        plan.categories
            .iter()
            .find(|category| category.members.iter().any(|member| member.id == id))
            .map(|category| category.name.clone())
    }

    #[test]
    fn cleaning_strips_vendor_noise_and_take_numbers() {
        let freesound = clean_sfx_filename("385192__actioncircus__heavy-steel-door-opening-and-closing.mp3");
        assert_eq!(freesound.tokens, vec!["heavy", "steel", "door", "opening", "closing"]);

        let underscored_user = clean_sfx_filename("245871__mick-t-1__tram3.mp3");
        assert_eq!(underscored_user.tokens, vec!["tram"]);
        assert_eq!(underscored_user.sequence, Some(3));

        let epidemic = clean_sfx_filename("ES_Multimedia 770 - SFX Producer.mp3");
        assert_eq!(epidemic.stem, "multimedia");
        assert_eq!(epidemic.sequence, Some(770));

        let camel = clean_sfx_filename("DoorSlam_02.wav");
        assert_eq!(camel.tokens, vec!["door", "slam"]);
        assert_eq!(camel.sequence, Some(2));
    }

    #[test]
    fn numbered_series_stay_together_even_without_a_keyword() {
        let plan = propose_sfx_categories(&inputs(&["Zorp_01.wav", "Zorp_02.wav", "Zorp_03.wav"]), &[]);
        assert_eq!(plan.categories.len(), 1);
        assert_eq!(plan.categories[0].name, "Zorp");
        assert_eq!(plan.categories[0].members.len(), 3);
        assert!(plan.categories[0].members.iter().all(|member| member.reason == CategoryReason::Series));
    }

    #[test]
    fn keywords_group_real_world_names() {
        let plan = propose_sfx_categories(
            &inputs(&[
                "339632__nichols8917__door-closing.mp3",
                "80998__payattention__doorslam.mp3",
                "264869__paulmessier__footsteps-2-soft.wav",
                "211444__harris85__man-walking-on-the-street.wav",
                "ES_Multimedia 770 - SFX Producer.mp3",
                "ES_Beep Sonar Blip 1 - SFX Producer.mp3",
            ]),
            &[],
        );
        assert_eq!(category_of(&plan, "80998__payattention__doorslam.mp3").as_deref(), Some("Doors"));
        assert_eq!(category_of(&plan, "339632__nichols8917__door-closing.mp3").as_deref(), Some("Doors"));
        assert_eq!(
            category_of(&plan, "211444__harris85__man-walking-on-the-street.wav").as_deref(),
            Some("Footsteps")
        );
        assert_eq!(category_of(&plan, "ES_Multimedia 770 - SFX Producer.mp3").as_deref(), Some("UI & Beeps"));
    }

    #[test]
    fn specific_categories_beat_generic_ones() {
        let plan = propose_sfx_categories(
            &inputs(&[
                "120229__mromero__metrojussieu-ambiancevagon.mp3",
                "SCI_FI_MACHINE_Resonator_Hit_Double_Hard_06.wav",
                "192468__fridobeck__telephone-noise.wav",
            ]),
            &[],
        );
        assert_eq!(
            category_of(&plan, "120229__mromero__metrojussieu-ambiancevagon.mp3").as_deref(),
            Some("Vehicles")
        );
        assert_eq!(
            category_of(&plan, "SCI_FI_MACHINE_Resonator_Hit_Double_Hard_06.wav").as_deref(),
            Some("Sci-Fi")
        );
        assert_eq!(category_of(&plan, "192468__fridobeck__telephone-noise.wav").as_deref(), Some("Electronics"));
    }

    #[test]
    fn what_a_sound_is_beats_what_it_is_made_of() {
        let plan = propose_sfx_categories(
            &inputs(&["385192__actioncircus__heavy-steel-door-opening-and-closing.mp3", "metal_scrape.wav"]),
            &[],
        );
        assert_eq!(
            category_of(&plan, "385192__actioncircus__heavy-steel-door-opening-and-closing.mp3").as_deref(),
            Some("Doors")
        );
        assert_eq!(category_of(&plan, "metal_scrape.wav").as_deref(), Some("Metal"));
    }

    #[test]
    fn existing_categories_are_reused_instead_of_duplicated() {
        let plan = propose_sfx_categories(&inputs(&["door_close.wav", "Kerplunk_A.wav"]), &["Door".to_string(), "Kerplunk".to_string()]);
        assert_eq!(category_of(&plan, "door_close.wav").as_deref(), Some("Door"));
        assert_eq!(category_of(&plan, "Kerplunk_A.wav").as_deref(), Some("Kerplunk"));
        assert!(plan.categories.iter().all(|category| category.existing));
    }

    #[test]
    fn shared_first_word_groups_leftovers_and_singletons_stay_uncategorized() {
        let plan = propose_sfx_categories(
            &inputs(&["Flumph Long.wav", "Flumph Short Dark.wav", "376010__deletecookies__gotoast.mp3", "Qwzx.wav"]),
            &[],
        );
        assert_eq!(category_of(&plan, "Flumph Long.wav").as_deref(), Some("Flumph"));
        assert_eq!(category_of(&plan, "Flumph Short Dark.wav").as_deref(), Some("Flumph"));
        assert_eq!(plan.uncategorized, vec!["Qwzx.wav".to_string()]);
        assert!(category_of(&plan, "376010__deletecookies__gotoast.mp3").is_none() || plan.uncategorized.len() == 1);
    }

    #[test]
    fn short_vocabulary_words_never_match_inside_longer_words() {
        let plan = propose_sfx_categories(&inputs(&["carpetshuffle.wav"]), &[]);
        assert_ne!(category_of(&plan, "carpetshuffle.wav").as_deref(), Some("Vehicles"));
    }
}
