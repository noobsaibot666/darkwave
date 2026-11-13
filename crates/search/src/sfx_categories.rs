//! Auto-organization of sound effects into categories — the engine behind
//! the "Organize" button. Pure and deterministic: no catalog access, no
//! I/O. The Tauri layer feeds it every imported track (name, folders,
//! current media type, duration), shows the proposal for review, and only
//! then writes anything.
//!
//! Two questions per track:
//!
//! **Is it a sound effect?** Tracks already filed as `sound_effect` are.
//! For anything filed elsewhere (music, ambience, other, needs-review)
//! [`sfx_evidence`] weighs: a UCS category code in the name, sitting under
//! an SFX folder (`_SoundFX`, `Sound Effects`…) vs. a music one
//! (`_SoundTracks`, `Music`…), name/folder words, and duration. Only a
//! clear case is proposed for re-filing, and the review lists it as such.
//!
//! **Which category?** Strongest signal first:
//!
//! 1. A category the user already has, named in the file name.
//! 2. A **UCS CatID** prefix (`DSGNSynth_…`, `WHSHFast_…`, `SCIMach_…`),
//!    the Universal Category System many commercial libraries use.
//! 3. **Words** in the file name and in its nearest folders
//!    (`Whooshes/170 Other Whooshes/01_001.wav` → Whooshes), scored by a
//!    curated UCS-style vocabulary where specific categories (Doors,
//!    Whooshes…) beat generic ones (Impacts, Ambience, Noise) and what a
//!    sound *is* beats what it's made of. The file name outweighs folders.
//! 4. **Numbered series** — identical names apart from a number — always
//!    stay together; with no keyword they become a category of their own.
//! 5. A **shared first word** among leftovers.
//!
//! Fallback groups (4 and 5) need at least [`MIN_FALLBACK_GROUP`] files and
//! get a short name, so a big library doesn't sprout hundreds of one-file
//! folders. Anything still unmatched is left uncategorized.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};

/// Fallback groups smaller than this are left uncategorized instead.
pub const MIN_FALLBACK_GROUP: usize = 3;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct OrganizeInput {
    pub id: String,
    pub filename: String,
    /// Enclosing folder names, nearest first (e.g. `["170 Other Whooshes",
    /// "Whooshes", "400 Whooshes – CINEM8.CO", "_SoundFX", …]`).
    pub folders: Vec<String>,
    /// Current media type; empty is treated as `sound_effect`.
    pub media_type: String,
    pub duration_ms: Option<i64>,
}

impl OrganizeInput {
    /// Shorthand for an already-filed sound effect known only by name.
    pub fn named(id: impl Into<String>, filename: impl Into<String>) -> Self {
        Self { id: id.into(), filename: filename.into(), ..Self::default() }
    }

    fn is_sound_effect(&self) -> bool {
        self.media_type.is_empty() || self.media_type == "sound_effect"
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CategoryReason {
    /// Matched a category the library already has.
    Existing,
    /// A UCS CatID at the start of the name.
    Ucs,
    /// Words in the file name.
    Keyword,
    /// Words in the folders it sits in.
    Folder,
    /// Part of a numbered series with no keyword — named after the series.
    Series,
    /// Shares its first word with other otherwise-unmatched files.
    SharedWord,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProposedMember {
    pub id: String,
    pub reason: CategoryReason,
    /// The word/code/folder that decided it — shown in the review so every
    /// placement is explainable.
    pub matched: String,
    /// Shared stem when this file is part of a numbered series.
    pub series: Option<String>,
    /// `Some(old media type)` when this track was filed as something other
    /// than a sound effect and is proposed for re-filing as one.
    pub reclassify_from: Option<String>,
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
    /// Sound effects no signal could place (ids).
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
/// catch-all (or a material) that should lose to anything more specific.
const VOCABULARY: &[(&str, u8, &[&str])] = &[
    ("Doors", 2, &["door", "doors", "doorslam", "gate", "gates", "latch", "hinge", "doorbell"]),
    ("Footsteps", 2, &["footstep", "footsteps", "steps", "step", "walking", "walk", "stomp", "shoe", "shoes", "heels", "sneaker", "sneakers", "jogging", "feet"]),
    ("Explosions", 2, &["explosion", "explosions", "explode", "boom", "booms", "blast", "cannon", "bomb", "detonation", "kaboom"]),
    ("Whooshes", 2, &["whoosh", "whooshes", "swoosh", "swooshes", "swish", "swishes", "woosh", "wooshes", "whoosch", "swipe", "swipes", "whizz", "whip", "whips", "flyby", "flybys", "passby", "fly", "zoom", "zooms", "swirl", "twirl", "spin", "swing", "pass"]),
    ("Glitches", 2, &["glitch", "glitches", "glitching", "glitchy", "stutter", "bitcrush", "datamosh", "distortion", "distortions"]),
    ("UI & Beeps", 2, &["beep", "beeps", "blip", "blips", "bleep", "ui", "interface", "multimedia", "button", "buttons", "menu", "notification", "notifications", "ping", "alert", "click", "clicks", "achievement", "hud", "select", "confirm"]),
    ("Coins & Money", 2, &["coin", "coins", "money", "cash", "cashier", "register"]),
    ("Vehicles", 2, &["car", "cars", "engine", "tram", "train", "trains", "metro", "subway", "bus", "truck", "ambulance", "siren", "sirens", "traffic", "motorcycle", "motorbike", "vagon", "wagon", "plane", "airplane", "helicopter", "horn", "vehicle", "vehicles", "runway", "airport"]),
    ("Weapons", 2, &["gun", "guns", "gunshot", "gunshots", "rifle", "pistol", "shotgun", "sword", "blade", "reload", "bullet", "bullets", "weapon", "weapons"]),
    ("Water", 2, &["water", "splash", "splashes", "drip", "drips", "dripping", "river", "stream", "ocean", "sea", "bubble", "bubbles", "pour", "pouring", "liquid", "underwater"]),
    ("Weather", 2, &["thunder", "storm", "rain", "raining", "rainfall", "wind", "windy", "hail", "blizzard"]),
    ("Fire", 2, &["fire", "flame", "flames", "burning", "crackle", "crackling", "fireplace", "torch"]),
    ("Glass", 2, &["glass", "shatter", "shattering", "window"]),
    ("Metal", 1, &["metal", "metallic", "steel", "iron", "clang", "chain", "chains"]),
    ("Wood", 1, &["wood", "wooden", "creak", "creaking", "creaky", "plank"]),
    ("Human", 2, &["breath", "breathing", "breathe", "sigh", "cough", "laugh", "laughing", "scream", "screaming", "yell", "gasp", "heartbeat", "finger", "fingers", "fingerflick", "snap", "clap", "clapping", "kiss", "whisper"]),
    ("Voices", 2, &["voice", "voices", "vox", "vocal", "vocals", "speech", "talk"]),
    ("Crowds", 2, &["crowd", "crowds", "audience", "applause", "cheer", "cheering", "chatter", "walla"]),
    ("Animals", 2, &["dog", "dogs", "cat", "cats", "bird", "birds", "horse", "cow", "animal", "animals", "bark", "barking", "meow", "chirp", "insects", "cricket", "crickets"]),
    ("Electronics", 2, &["telephone", "phone", "radio", "static", "tv", "television", "buzz", "hum", "electric", "electricity", "computer", "keyboard", "typing", "printer", "camera", "shutter"]),
    ("Sci-Fi", 2, &["scifi", "sci", "laser", "lasers", "spaceship", "alien", "robot", "robotic", "resonator", "futuristic", "teleport", "forcefield"]),
    ("Objects", 2, &["table", "desk", "book", "books", "dish", "dishes", "plate", "cup", "objects", "object", "paper", "page", "pages", "bag", "box", "zipper", "keys", "drawer", "chair", "bottle", "cutlery", "toast", "toaster"]),
    ("Gore & Bones", 2, &["bone", "bones", "neck", "breakingneck", "crunch", "gore", "flesh", "squelch"]),
    ("Fights", 2, &["punch", "punches", "kick", "kicks", "slap", "fight", "fighting"]),
    ("Risers", 2, &["rise", "riser", "risers", "rising", "buildup", "uplifter", "uplifters", "sweep", "sweeps", "upper", "uppers", "build"]),
    ("Drones & Rumbles", 2, &["drone", "drones", "rumble", "rumbles", "rumbling", "sub", "subbass", "bass", "lowend", "undertone", "undertones"]),
    ("Textures", 2, &["texture", "textures", "granular", "grain", "grains", "resonance", "resonances", "organic", "elemental", "atonal", "tonal", "pad", "pads"]),
    ("Impulse Responses", 2, &["impulse", "ir", "irs", "reverb", "convolution"]),
    ("Transitions", 1, &["transition", "transitions", "trans", "downer", "downers", "reverse", "reversed", "rewind", "tape", "stop"]),
    ("Bells", 2, &["bell", "bells", "chime", "chimes", "gong", "ding"]),
    ("Cartoon", 2, &["cartoon", "boing", "squeak", "squeaky", "toon"]),
    ("Mechanical", 2, &["machine", "machines", "mechanical", "motor", "gear", "gears", "servo", "hydraulic", "mechanism", "elevator"]),
    ("Alarms", 2, &["alarm", "alarms", "warning", "klaxon"]),
    ("Impacts", 1, &["impact", "impacts", "hit", "hits", "thud", "slam", "slams", "smash", "crash", "drop", "drops", "bang", "knock", "knocking", "thump", "stomps"]),
    ("Ambience", 1, &["ambience", "ambiences", "ambient", "ambiance", "ambiente", "atmosphere", "atmospheres", "atmos", "atmo", "room", "roomtone", "background", "nature", "forest", "city", "street", "night", "neighborhood", "village", "park", "bar", "cafe", "restaurant", "office", "interior", "exterior", "quiet", "outdoor", "indoor"]),
    ("Noise", 1, &["noise", "noises", "hiss", "rustle"]),
];

/// Categories that are perfectly normal for a track filed as Ambience —
/// they don't count as evidence that an Ambience track is really an SFX.
const AMBIENT_CATEGORIES: &[&str] = &[
    "Ambience", "Weather", "Water", "Crowds", "Animals", "Vehicles", "Noise", "Drones & Rumbles", "Textures", "Fire",
];

/// UCS (Universal Category System) CatID prefixes → our category. A UCS
/// file name starts `<CatID>_…`, the CatID being the category code plus a
/// capitalized subcategory (`DSGNSynth`, `WHSHFast`, `SCIMach`).
const UCS_CATEGORIES: &[(&str, &str)] = &[
    ("WHSH", "Whooshes"), ("SWSH", "Whooshes"), ("IMPT", "Impacts"), ("DSGN", "Designed"), ("SCI", "Sci-Fi"),
    ("LASR", "Sci-Fi"), ("ROBO", "Sci-Fi"), ("METL", "Metal"), ("MECH", "Mechanical"), ("MACH", "Mechanical"),
    ("MOTR", "Mechanical"), ("MUSC", "Musical"), ("AMB", "Ambience"), ("DOOR", "Doors"), ("FEET", "Footsteps"),
    ("GLAS", "Glass"), ("WATR", "Water"), ("RAIN", "Weather"), ("WIND", "Weather"), ("VEH", "Vehicles"),
    ("TRAN", "Vehicles"), ("AIR", "Vehicles"), ("BOAT", "Vehicles"), ("UI", "UI & Beeps"), ("BEEP", "UI & Beeps"),
    ("COMP", "Electronics"), ("COMM", "Electronics"), ("ELEC", "Electronics"), ("EXPL", "Explosions"),
    ("FIRE", "Fire"), ("GUN", "Weapons"), ("WEAP", "Weapons"), ("BLLT", "Weapons"), ("ANML", "Animals"),
    ("BIRD", "Animals"), ("CREA", "Animals"), ("HMN", "Human"), ("VOX", "Voices"), ("CRWD", "Crowds"),
    ("FGHT", "Fights"), ("GORE", "Gore & Bones"), ("WOOD", "Wood"), ("ALRM", "Alarms"), ("BELL", "Bells"),
    ("TOON", "Cartoon"), ("NOIS", "Noise"), ("OBJ", "Objects"), ("PAPR", "Objects"), ("DRWR", "Objects"),
    ("TOOL", "Objects"), ("DEST", "Impacts"), ("ROCK", "Impacts"), ("MAGC", "Magic"), ("FOLY", "Foley"),
    ("CLOTH", "Foley"), ("MOVE", "Foley"), ("GAME", "UI & Beeps"), ("TOY", "Objects"), ("FOOD", "Objects"), ("PLAS", "Objects"),
    ("RUBR", "Objects"), ("LETH", "Foley"), ("CERM", "Glass"), ("CHAIN", "Metal"), ("CLOCK", "Mechanical"),
    ("DIRT", "Foley"), ("ICE", "Glass"), ("SNOW", "Weather"), ("SPRT", "Objects"), ("WING", "Whooshes"),
    ("WINDW", "Glass"), ("HORN", "Vehicles"), ("EQUIP", "Mechanical"), ("LIQ", "Water"),
];

/// Words that carry no meaning about what a sound *is*.
const FILLER: &[&str] = &[
    "the", "and", "or", "of", "on", "in", "at", "to", "with", "by", "for", "from", "into", "onto", "off", "out",
    "sfx", "fx", "sound", "sounds", "effect", "effects", "take", "version", "ver", "mix", "stereo", "mono", "master",
    "final", "edit", "loop", "var", "variation", "alt", "es", "producer", "wav", "mp3", "aif", "aiff", "flac", "ogg",
    "high", "quality", "hq", "free", "sample", "samples", "putting", "placing", "put", "it", "its", "is", "something",
    "specific", "nothing", "artlist", "smorph", "cinem8",
];

/// Extra noise that only ever shows up in *folder* names (pack titles,
/// vendor dumps) and says nothing about the sound inside.
const FOLDER_FILLER: &[&str] = &[
    "pack", "packs", "ultimate", "new", "collection", "library", "vol", "volume", "audio", "wavs", "files", "elements",
    "raw", "recordings", "recording", "kit", "bundle", "pro", "edition", "design", "designed", "designer", "other",
    "small", "big", "large", "misc", "various", "assorted", "source", "utc", "soundfx", "soundeffects", "downloads",
    "download", "adds", "tracks", "track", "titles", "footages", "footage", "motion", "graphics", "corporation",
];

/// Folder words that say "everything in here is a sound effect" / "is music".
const SFX_FOLDER_MARKERS: &[&str] = &["sfx", "soundfx", "fx", "effects", "soundeffects", "foley", "whooshes", "transitions", "designed"];
const MUSIC_FOLDER_MARKERS: &[&str] = &["soundtracks", "soundtrack", "music", "songs", "song", "tracks", "score", "albums", "album"];

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
                    && best.map_or(true, |(_, best_weight, best_len)| (*weight, key.len()) > (best_weight, best_len))
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
    let lower: String = name.to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
    lower.strip_suffix('s').map(str::to_string).unwrap_or(lower)
}

fn title_case(words: &str) -> String {
    let mut characters = words.chars();
    match characters.next() {
        Some(first) => first.to_ascii_uppercase().to_string() + characters.as_str(),
        None => String::new(),
    }
}

/// The UCS category of a file name, if it starts with a CatID.
pub fn ucs_category(filename: &str) -> Option<(&'static str, String)> {
    let first = filename.split(['_', ' ', '-', '.']).next()?;
    for (prefix, category) in UCS_CATEGORIES {
        let Some(rest) = first.strip_prefix(prefix) else { continue };
        let mut rest_chars = rest.chars();
        // CatID shape: the code, then a Capitalized subcategory.
        if matches!((rest_chars.next(), rest_chars.next()), (Some(a), Some(b)) if a.is_ascii_uppercase() && b.is_ascii_lowercase()) {
            return Some((category, first.to_string()));
        }
    }
    None
}

/// Meaningful words of one folder name (`"170 Other Whooshes"` → `["whooshes"]`).
fn folder_tokens(folder: &str) -> Vec<String> {
    clean_sfx_filename(&format!("{folder}.dir"))
        .tokens
        .into_iter()
        .filter(|token| !FOLDER_FILLER.contains(&token.as_str()))
        .collect()
}

fn folder_marker_hits(folders: &[String], markers: &[&str]) -> bool {
    folders.iter().any(|folder| {
        let squashed: String = folder.to_ascii_lowercase().chars().filter(|c| c.is_ascii_alphanumeric()).collect();
        let words: Vec<String> = folder
            .to_ascii_lowercase()
            .split(|c: char| !c.is_ascii_alphanumeric())
            .filter(|w| !w.is_empty())
            .map(str::to_string)
            .collect();
        markers.iter().any(|marker| words.iter().any(|w| w == marker) || squashed == *marker)
    })
}

struct Decision {
    category: String,
    reason: CategoryReason,
    matched: String,
    /// Strength of the vocabulary evidence (3 for Existing/UCS).
    weight: u8,
    /// Whether the file's own name (not just its folder) said so.
    from_name: bool,
}

/// Decides one file's category from its name and folders, or `None`.
fn classify(
    input: &OrganizeInput,
    tokens: &[String],
    lookup: &Lookup,
    existing: &HashMap<String, String>,
) -> Option<Decision> {
    for token in tokens {
        if let Some(name) = existing.get(&normalize_category(token)) {
            return Some(Decision { category: name.clone(), reason: CategoryReason::Existing, matched: token.clone(), weight: 3, from_name: true });
        }
    }
    if let Some((category, code)) = ucs_category(&input.filename) {
        let name = existing.get(&normalize_category(category)).cloned().unwrap_or_else(|| category.to_string());
        return Some(Decision { category: name, reason: CategoryReason::Ucs, matched: code, weight: 3, from_name: true });
    }

    // (category, score, first position, matched word, from file name?)
    let mut scores: Vec<(&'static str, u32, usize, String, bool)> = Vec::new();
    let mut add = |category: &'static str, points: u32, position: usize, word: &str, from_name: bool| {
        match scores.iter_mut().find(|entry| entry.0 == category) {
            Some(entry) => {
                entry.1 += points;
                if from_name && !entry.4 {
                    entry.3 = word.to_string();
                    entry.4 = true;
                }
            }
            None => scores.push((category, points, position, word.to_string(), from_name)),
        }
    };
    for (position, token) in tokens.iter().enumerate() {
        if let Some((category, weight)) = lookup.category_for(token) {
            add(category, weight as u32 * 3, position, token, true);
        }
    }
    // Nearest folder counts double the ones above it; only the 3 nearest.
    for (depth, folder) in input.folders.iter().take(3).enumerate() {
        let multiplier = if depth == 0 { 2 } else { 1 };
        for (offset, token) in folder_tokens(folder).iter().enumerate() {
            if let Some((category, weight)) = lookup.category_for(token) {
                add(category, weight as u32 * multiplier, 100 * (depth + 1) + offset, token, false);
            }
        }
    }
    let (category, _, _, matched, from_name) = scores
        .into_iter()
        .max_by(|left, right| left.1.cmp(&right.1).then(right.2.cmp(&left.2)))?;
    let weight = VOCABULARY.iter().find(|(name, _, _)| *name == category).map_or(1, |(_, weight, _)| *weight);
    let normalized = normalize_category(category);
    let (name, reason) = match existing.get(&normalized) {
        Some(name) => (name.clone(), CategoryReason::Existing),
        None => (category.to_string(), if from_name { CategoryReason::Keyword } else { CategoryReason::Folder }),
    };
    Some(Decision { category: name, reason, matched, weight, from_name })
}

/// How strongly a track filed as something else looks like a sound effect.
/// [`propose_sfx_categories`] re-files it at 3 or more. Calibrated on a
/// real 4,471-track library: music lives under `_SoundTracks/`, sound
/// effects under `_SoundFX/`, and a music title alone ("Fire", "Rain") is
/// never enough.
pub fn sfx_evidence(input: &OrganizeInput, decision: Option<&DecisionView>) -> i32 {
    let mut score = 0;
    let has_ucs = ucs_category(&input.filename).is_some();
    if has_ucs {
        score += 3;
    }
    if folder_marker_hits(&input.folders, SFX_FOLDER_MARKERS) {
        score += 2;
    }
    if folder_marker_hits(&input.folders, MUSIC_FOLDER_MARKERS) {
        score -= 3;
    }
    let lower = input.filename.to_ascii_lowercase();
    if lower.split(|c: char| !c.is_ascii_alphanumeric()).any(|word| word == "sfx" || word == "fx") {
        score += 1;
    }
    if let Some(decision) = decision {
        let ambient_ok = input.media_type == "ambience" && AMBIENT_CATEGORIES.contains(&decision.category);
        if !ambient_ok && !has_ucs && decision.weight >= 2 {
            // "helicopter" in the name is real evidence; the same word
            // only in a pack folder's title is weaker.
            score += if decision.from_name { 2 } else { 1 };
        }
    }
    // Long tracks lean music, but real effects (landings, pass-bys) run
    // long too, so this only nudges.
    match input.duration_ms {
        Some(ms) if ms <= 15_000 => score += 1,
        Some(ms) if ms > 300_000 => score -= 2,
        Some(ms) if ms > 120_000 => score -= 1,
        _ => {}
    }
    score
}

/// The parts of a category decision [`sfx_evidence`] looks at.
pub struct DecisionView {
    pub category: &'static str,
    pub weight: u8,
    pub from_name: bool,
}

pub fn propose_sfx_categories(inputs: &[OrganizeInput], existing_categories: &[String]) -> OrganizePlan {
    let lookup = Lookup::new();
    let existing: HashMap<String, String> =
        existing_categories.iter().map(|name| (normalize_category(name), name.clone())).collect();

    // 1. Decide each track's category, and whether a non-SFX track is
    //    really a sound effect (others are dropped from the plan entirely).
    struct Candidate<'a> {
        input: &'a OrganizeInput,
        cleaned: CleanedName,
        decision: Option<Decision>,
        reclassify_from: Option<String>,
    }
    let mut candidates: Vec<Candidate> = Vec::new();
    for input in inputs {
        let cleaned = clean_sfx_filename(&input.filename);
        let decision = classify(input, &cleaned.tokens, &lookup, &existing);
        let reclassify_from = if input.is_sound_effect() {
            None
        } else {
            let view = decision.as_ref().map(|decision| DecisionView {
                category: VOCABULARY
                    .iter()
                    .map(|(name, _, _)| *name)
                    .find(|name| *name == decision.category)
                    .unwrap_or(""),
                weight: decision.weight,
                from_name: decision.from_name,
            });
            // An Ambience track whose category is itself ambient (a city
            // atmos, rain) is correctly filed — never re-file it.
            let ambient_and_filed_as_ambience = input.media_type == "ambience"
                && decision.as_ref().is_some_and(|decision| {
                    AMBIENT_CATEGORIES.iter().any(|name| normalize_category(name) == normalize_category(&decision.category))
                });
            // Re-file only clear cases that we can also place.
            if decision.is_none() || ambient_and_filed_as_ambience || sfx_evidence(input, view.as_ref()) < 3 {
                continue;
            }
            Some(input.media_type.clone())
        };
        candidates.push(Candidate { input, cleaned, decision, reclassify_from });
    }

    // 2. Series: same non-empty stem, ≥2 files, at least one numbered.
    let mut by_stem: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (index, candidate) in candidates.iter().enumerate() {
        if !candidate.cleaned.stem.is_empty() {
            by_stem.entry(candidate.cleaned.stem.as_str()).or_default().push(index);
        }
    }
    let mut series_of: HashMap<usize, String> = HashMap::new();
    for (stem, members) in &by_stem {
        if members.len() >= 2 && members.iter().any(|&index| candidates[index].cleaned.sequence.is_some()) {
            // A series follows the category most of its members got, so
            // one oddly named take can't split it.
            for &index in members {
                series_of.insert(index, stem.to_string());
            }
        }
    }
    let mut series_category: HashMap<String, (String, CategoryReason, String)> = HashMap::new();
    for (stem, members) in &by_stem {
        if !members.iter().all(|index| series_of.contains_key(index)) {
            continue;
        }
        let mut votes: BTreeMap<&str, usize> = BTreeMap::new();
        for &index in members {
            if let Some(decision) = &candidates[index].decision {
                *votes.entry(decision.category.as_str()).or_default() += 1;
            }
        }
        if let Some((winner, _)) = votes.into_iter().max_by_key(|(_, count)| *count) {
            let template = members
                .iter()
                .filter_map(|&index| candidates[index].decision.as_ref())
                .find(|decision| decision.category == winner)
                .unwrap();
            series_category.insert(stem.to_string(), (winner.to_string(), template.reason, template.matched.clone()));
        }
    }

    let mut assigned: BTreeMap<String, (bool, Vec<ProposedMember>)> = BTreeMap::new();
    let mut place = |name: String, existing_flag: bool, member: ProposedMember| {
        let entry = assigned.entry(name).or_insert((existing_flag, Vec::new()));
        entry.0 |= existing_flag;
        entry.1.push(member);
    };
    let mut fallback_series: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    let mut leftovers: Vec<usize> = Vec::new();

    for (index, candidate) in candidates.iter().enumerate() {
        let series = series_of.get(&index).cloned();
        let chosen = match series.as_ref().and_then(|stem| series_category.get(stem)) {
            Some((name, reason, matched)) => Some((name.clone(), *reason, matched.clone())),
            None => candidate
                .decision
                .as_ref()
                .map(|decision| (decision.category.clone(), decision.reason, decision.matched.clone())),
        };
        match chosen {
            Some((name, reason, matched)) => place(
                name,
                reason == CategoryReason::Existing,
                ProposedMember {
                    id: candidate.input.id.clone(),
                    reason,
                    matched,
                    series,
                    reclassify_from: candidate.reclassify_from.clone(),
                },
            ),
            None => match series {
                Some(stem) => fallback_series.entry(stem).or_default().push(index),
                None => leftovers.push(index),
            },
        }
    }

    // 3. Fallbacks, only when big enough to be worth a folder, with a short
    //    name (first two words) so pack titles don't become category names.
    let short_name = |words: &str| -> String {
        title_case(&words.split(' ').take(2).collect::<Vec<_>>().join(" "))
    };
    let mut uncategorized = Vec::new();
    for (stem, members) in fallback_series {
        if members.len() < MIN_FALLBACK_GROUP {
            leftovers.extend(members);
            continue;
        }
        let name = short_name(&stem);
        let is_existing = existing.contains_key(&normalize_category(&name));
        let name = existing.get(&normalize_category(&name)).cloned().unwrap_or(name);
        for index in members {
            place(
                name.clone(),
                is_existing,
                ProposedMember {
                    id: candidates[index].input.id.clone(),
                    reason: CategoryReason::Series,
                    matched: stem.clone(),
                    series: Some(stem.clone()),
                    reclassify_from: candidates[index].reclassify_from.clone(),
                },
            );
        }
    }
    let mut by_first: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for &index in &leftovers {
        if let Some(first) = candidates[index].cleaned.tokens.first().filter(|word| word.len() >= 3) {
            by_first.entry(first.clone()).or_default().push(index);
        }
    }
    let grouped: std::collections::HashSet<usize> = by_first
        .values()
        .filter(|group| group.len() >= MIN_FALLBACK_GROUP)
        .flatten()
        .copied()
        .collect();
    for (word, group) in &by_first {
        if group.len() < MIN_FALLBACK_GROUP {
            continue;
        }
        for &index in group {
            place(
                short_name(word),
                false,
                ProposedMember {
                    id: candidates[index].input.id.clone(),
                    reason: CategoryReason::SharedWord,
                    matched: word.clone(),
                    series: None,
                    reclassify_from: candidates[index].reclassify_from.clone(),
                },
            );
        }
    }
    for &index in &leftovers {
        if !grouped.contains(&index) {
            uncategorized.push(candidates[index].input.id.clone());
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
        names.iter().map(|name| OrganizeInput::named(*name, *name)).collect()
    }

    fn category_of(plan: &OrganizePlan, id: &str) -> Option<String> {
        plan.categories
            .iter()
            .find(|category| category.members.iter().any(|member| member.id == id))
            .map(|category| category.name.clone())
    }

    fn member<'a>(plan: &'a OrganizePlan, id: &str) -> &'a ProposedMember {
        plan.categories.iter().flat_map(|category| &category.members).find(|member| member.id == id).unwrap()
    }

    fn located(id: &str, filename: &str, folders: &[&str], media_type: &str, duration_ms: Option<i64>) -> OrganizeInput {
        OrganizeInput {
            id: id.into(),
            filename: filename.into(),
            folders: folders.iter().map(|folder| folder.to_string()).collect(),
            media_type: media_type.into(),
            duration_ms,
        }
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
    }

    #[test]
    fn keywords_group_real_world_names() {
        let plan = propose_sfx_categories(
            &inputs(&[
                "339632__nichols8917__door-closing.mp3",
                "80998__payattention__doorslam.mp3",
                "211444__harris85__man-walking-on-the-street.wav",
                "ES_Multimedia 770 - SFX Producer.mp3",
            ]),
            &[],
        );
        assert_eq!(category_of(&plan, "80998__payattention__doorslam.mp3").as_deref(), Some("Doors"));
        assert_eq!(category_of(&plan, "211444__harris85__man-walking-on-the-street.wav").as_deref(), Some("Footsteps"));
        assert_eq!(category_of(&plan, "ES_Multimedia 770 - SFX Producer.mp3").as_deref(), Some("UI & Beeps"));
    }

    #[test]
    fn specific_beats_generic_and_what_it_is_beats_material() {
        let plan = propose_sfx_categories(
            &inputs(&[
                "120229__mromero__metrojussieu-ambiancevagon.mp3",
                "192468__fridobeck__telephone-noise.wav",
                "385192__actioncircus__heavy-steel-door-opening-and-closing.mp3",
                "metal_scrape.wav",
            ]),
            &[],
        );
        assert_eq!(category_of(&plan, "120229__mromero__metrojussieu-ambiancevagon.mp3").as_deref(), Some("Vehicles"));
        assert_eq!(category_of(&plan, "192468__fridobeck__telephone-noise.wav").as_deref(), Some("Electronics"));
        assert_eq!(
            category_of(&plan, "385192__actioncircus__heavy-steel-door-opening-and-closing.mp3").as_deref(),
            Some("Doors")
        );
        assert_eq!(category_of(&plan, "metal_scrape.wav").as_deref(), Some("Metal"));
    }

    #[test]
    fn ucs_catids_decide_the_category() {
        assert_eq!(ucs_category("DSGNSynth_SMORPH Motion Graphics Synth Texture Grain 01.wav").unwrap().0, "Designed");
        assert_eq!(ucs_category("WHSHFast_Big Whoosh_ABC_01.wav").unwrap().0, "Whooshes");
        assert_eq!(ucs_category("SCIMach_SMORPH Micro.wav").unwrap().0, "Sci-Fi");
        assert!(ucs_category("SCI_FI_MACHINE_Resonator.wav").is_none(), "all caps words aren't a CatID");
        assert!(ucs_category("Doorslam.wav").is_none());
        let plan = propose_sfx_categories(&inputs(&["METLTonl_SMORPH Atonal Resonance 03.wav"]), &[]);
        assert_eq!(member(&plan, "METLTonl_SMORPH Atonal Resonance 03.wav").reason, CategoryReason::Ucs);
    }

    #[test]
    fn folders_place_files_whose_names_say_nothing() {
        let plan = propose_sfx_categories(
            &[
                located("a", "01_001.wav", &["170 Other Whooshes", "Whooshes", "400 Whooshes – CINEM8.CO", "_SoundFX"], "sound_effect", Some(2000)),
                located("b", "07.wav", &["Glitches", "NEW! - ULTIMATE TRANSITION SOUNDS PACK"], "sound_effect", Some(1500)),
                located("c", "Wobble 3.wav", &["Sub Bass", "1. Designed Sounds"], "sound_effect", Some(4000)),
            ],
            &[],
        );
        assert_eq!(category_of(&plan, "a").as_deref(), Some("Whooshes"));
        assert_eq!(member(&plan, "a").reason, CategoryReason::Folder);
        assert_eq!(category_of(&plan, "b").as_deref(), Some("Glitches"));
        assert_eq!(category_of(&plan, "c").as_deref(), Some("Drones & Rumbles"));
    }

    #[test]
    fn the_file_name_outweighs_its_folder() {
        let plan = propose_sfx_categories(
            &[located("x", "Door Slam Heavy.wav", &["Whooshes"], "sound_effect", None)],
            &[],
        );
        assert_eq!(category_of(&plan, "x").as_deref(), Some("Doors"));
    }

    #[test]
    fn misfiled_sound_effects_are_found_but_music_titles_are_not() {
        let plan = propose_sfx_categories(
            &[
                // Real cases: whooshes filed as music, under _SoundFX.
                located("w", "77_whoosh_cinem8.wav", &["170 Other Whooshes", "Whooshes", "400 Whooshes – CINEM8.CO", "_SoundFX", "01_ADDS"], "music", Some(101_000)),
                located("h", "Carlos Santa Rita - Helicopter arriving to runway.wav", &["_SoundFX", "01_ADDS"], "music", Some(179_000)),
                // A song called "Fire" in the music folder stays music.
                located("song", "Fire.wav", &["Tracks", "_SoundTracks"], "music", Some(200_000)),
                located("song2", "Fire.wav", &["Downloads"], "music", Some(200_000)),
                // A short "Rain" ambience is just an ambience.
                located("rain", "Rain on roof.wav", &["_SoundFX"], "ambience", Some(9_000)),
                // ...but a whoosh filed as ambience is an SFX.
                located("amb-whoosh", "Big Whoosh.wav", &["_SoundFX"], "ambience", Some(4_000)),
                // A UCS name is decisive wherever it's filed.
                located("ucs", "WHSHFast_Zip_01.wav", &["Downloads"], "other", Some(1_000)),
            ],
            &[],
        );
        assert_eq!(member(&plan, "w").reclassify_from.as_deref(), Some("music"));
        assert_eq!(category_of(&plan, "w").as_deref(), Some("Whooshes"));
        assert_eq!(category_of(&plan, "h").as_deref(), Some("Vehicles"));
        assert!(category_of(&plan, "song").is_none());
        assert!(category_of(&plan, "song2").is_none(), "a title alone is never enough");
        assert!(category_of(&plan, "rain").is_none(), "an ambience that is an ambience stays filed as one");
        assert_eq!(member(&plan, "amb-whoosh").reclassify_from.as_deref(), Some("ambience"));
        assert_eq!(member(&plan, "ucs").reclassify_from.as_deref(), Some("other"));
        assert!(!plan.uncategorized.contains(&"song".to_string()), "non-SFX are dropped, not listed");
    }

    #[test]
    fn numbered_series_stay_together_and_follow_their_majority() {
        let plan = propose_sfx_categories(&inputs(&["Zorp_01.wav", "Zorp_02.wav", "Zorp_03.wav"]), &[]);
        assert_eq!(plan.categories.len(), 1);
        assert_eq!(plan.categories[0].name, "Zorp");
        assert!(plan.categories[0].members.iter().all(|member| member.reason == CategoryReason::Series));
    }

    #[test]
    fn small_fallback_groups_stay_uncategorized_and_names_stay_short() {
        let plan = propose_sfx_categories(
            &inputs(&[
                "Qwzx_01.wav",
                "Qwzx_02.wav",
                "Smorph Plonk Grizzle Wibble 01.wav",
                "Smorph Plonk Grizzle Wibble 02.wav",
                "Smorph Plonk Grizzle Wibble 03.wav",
            ]),
            &[],
        );
        assert!(plan.uncategorized.contains(&"Qwzx_01.wav".to_string()), "2 files is not worth a folder");
        assert_eq!(category_of(&plan, "Smorph Plonk Grizzle Wibble 01.wav").as_deref(), Some("Plonk grizzle"));
    }

    #[test]
    fn existing_categories_are_reused_instead_of_duplicated() {
        let plan = propose_sfx_categories(&inputs(&["door_close.wav"]), &["Door".to_string()]);
        assert_eq!(category_of(&plan, "door_close.wav").as_deref(), Some("Door"));
        assert!(plan.categories.iter().all(|category| category.existing));
    }

    #[test]
    fn short_vocabulary_words_never_match_inside_longer_words() {
        let plan = propose_sfx_categories(&inputs(&["carpetshuffle.wav"]), &[]);
        assert_ne!(category_of(&plan, "carpetshuffle.wav").as_deref(), Some("Vehicles"));
    }
}
