use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::path::Path;

pub const MIN_PREVIEW_CACHE_LIMIT_MB: u32 = 512;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum CommandId {
    TogglePlayback,
    PreviewSelected,
    NextAsset,
    PreviousAsset,
    ToggleFavorite,
    CommandPalette,
    Import,
    ExportSelected,
    ToggleLoop,
    CopyPath,
    ToggleReviewed,
    ClassifySoundtrack,
    ClassifyVoiceover,
    ClassifySoundEffect,
    ClassifyFoley,
    ClassifyAmbience,
    ClassifyOther,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ShortcutBinding {
    pub command: CommandId,
    pub accelerator: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ShortcutMap {
    pub bindings: Vec<ShortcutBinding>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum ShortcutError {
    Conflict {
        accelerator: String,
        commands: Vec<CommandId>,
    },
}

#[derive(Debug)]
pub enum PreferenceStoreError {
    Io(io::Error),
    Json(serde_json::Error),
    Shortcut(ShortcutError),
}

impl From<io::Error> for PreferenceStoreError {
    fn from(error: io::Error) -> Self {
        PreferenceStoreError::Io(error)
    }
}

impl From<serde_json::Error> for PreferenceStoreError {
    fn from(error: serde_json::Error) -> Self {
        PreferenceStoreError::Json(error)
    }
}

impl From<ShortcutError> for PreferenceStoreError {
    fn from(error: ShortcutError) -> Self {
        PreferenceStoreError::Shortcut(error)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum BrowserDensity {
    Compact,
    Comfortable,
    Expanded,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum OutputDevicePreference {
    SystemDefault,
    DeviceId(String),
}

/// Dark is the standard, default appearance for this app (a deliberate
/// product choice, not an oversight — light mode is an option, not the
/// baseline). `System` follows the OS light/dark setting.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
pub enum ThemePreference {
    #[default]
    Dark,
    Light,
    System,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppPreferences {
    pub browser_density: BrowserDensity,
    pub preview_cache_limit_mb: u32,
    pub output_device: OutputDevicePreference,
    pub shortcuts: ShortcutMap,
    #[serde(default)]
    pub reduced_motion: bool,
    #[serde(default)]
    pub reduced_transparency: bool,
    #[serde(default)]
    pub theme: ThemePreference,
    /// Whether the desktop app should hold a system idle-sleep assertion
    /// (caffeinate -i on macOS, SetThreadExecutionState on Windows) while
    /// there's unpaused background-job work pending, so a large overnight
    /// analysis batch isn't cut short by the machine sleeping. Defaults to
    /// `true` — including for preference files saved before this field
    /// existed — since that's the behavior an overnight batch needs; the
    /// preference exists as an opt-out, not an opt-in.
    #[serde(default = "default_prevent_sleep_during_analysis")]
    pub prevent_sleep_during_analysis: bool,
    /// Project files (`.darkwave`) opened before, most-recently-opened
    /// first, capped at `MAX_RECENT_LIBRARY_FILES` — surfaced on the first-run
    /// screen so reopening a known project doesn't require the native file
    /// picker every time.
    #[serde(default)]
    pub recent_library_files: Vec<RecentLibraryFileEntry>,
    /// The project file path to reopen automatically on next launch,
    /// skipping the first-run screen entirely (mirrors reopening the last
    /// document in an editor like After Effects). `None` before any project
    /// has ever been opened, or after the app is told to always ask.
    #[serde(default)]
    pub last_active_library_file_path: Option<String>,
}

/// One entry in the recent-projects list shown on the first-run screen.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RecentLibraryFileEntry {
    pub path: String,
    pub name: String,
    pub last_opened_at_ms: u64,
}

/// How many recent projects to remember — enough to be useful, small enough
/// to stay a quick glance rather than its own scrollable archive.
pub const MAX_RECENT_LIBRARY_FILES: usize = 10;

fn default_prevent_sleep_during_analysis() -> bool {
    true
}

impl AppPreferences {
    /// Records a project as just-opened: moves it to the front of
    /// `recent_library_files` (de-duplicating by path) and sets it as the
    /// project to reopen automatically next launch. Call this on every
    /// successful project open, whether via New Setup, Open Library, a
    /// recent-projects click, or a double-clicked `.darkwave` file.
    pub fn record_library_file_opened(&mut self, path: impl AsRef<str>, name: impl AsRef<str>, now_ms: u64) {
        let path = path.as_ref().to_string();
        self.add_recent_library_file(&path, name, now_ms);
        self.last_active_library_file_path = Some(path);
    }

    /// Adds a library file to the recent list without making it the
    /// active/reopen-on-launch project — used when a library file now
    /// exists but wasn't actually opened this session, e.g. right after
    /// migrating a legacy shared-catalog library into its own `.darkwave`
    /// file: it belongs on the first-run screen's recents list so the
    /// user's next click can open it, but nothing should silently open it
    /// on their behalf before they choose to.
    pub fn add_recent_library_file(&mut self, path: impl AsRef<str>, name: impl AsRef<str>, now_ms: u64) {
        let path = path.as_ref().to_string();
        self.recent_library_files.retain(|entry| entry.path != path);
        self.recent_library_files.insert(
            0,
            RecentLibraryFileEntry {
                path,
                name: name.as_ref().to_string(),
                last_opened_at_ms: now_ms,
            },
        );
        self.recent_library_files.truncate(MAX_RECENT_LIBRARY_FILES);
    }

    /// Drops a project from the recent list — call this when a
    /// remembered path no longer resolves (moved, deleted, or its volume
    /// is unmounted), so a stale entry doesn't keep reappearing forever.
    pub fn forget_library_file(&mut self, path: impl AsRef<str>) {
        let path = path.as_ref();
        self.recent_library_files.retain(|entry| entry.path != path);
        if self.last_active_library_file_path.as_deref() == Some(path) {
            self.last_active_library_file_path = None;
        }
    }
}

impl ShortcutMap {
    pub fn default_audio_workspace() -> Self {
        Self {
            bindings: vec![
                ShortcutBinding {
                    command: CommandId::TogglePlayback,
                    accelerator: "Space".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::PreviewSelected,
                    accelerator: "Enter".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::NextAsset,
                    accelerator: "ArrowDown".to_string(),
                },
                // A second accelerator for the same command — one command
                // can have more than one binding, `validate()` only rejects
                // the same accelerator mapping to *different* commands.
                // Keeps the arrow keys while adding WASD-style D/A so a
                // hand already on the keyboard doesn't need to reach for
                // arrows during a long review session.
                ShortcutBinding {
                    command: CommandId::NextAsset,
                    accelerator: "D".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::PreviousAsset,
                    accelerator: "ArrowUp".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::PreviousAsset,
                    accelerator: "A".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::ToggleFavorite,
                    accelerator: "F".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::CommandPalette,
                    accelerator: "Mod+K".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::Import,
                    accelerator: "Mod+I".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::ExportSelected,
                    accelerator: "Mod+E".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::ToggleLoop,
                    accelerator: "L".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::CopyPath,
                    accelerator: "Mod+Shift+C".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::ToggleReviewed,
                    accelerator: "Mod+R".to_string(),
                },
                // Classify shortcuts: top-row digits plus their numeric-keypad
                // equivalents (full keyboards only) both fire the same
                // command — each pair shares a command like NextAsset's
                // ArrowDown/D does above, so either key works interchangeably.
                ShortcutBinding {
                    command: CommandId::ClassifySoundtrack,
                    accelerator: "1".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::ClassifySoundtrack,
                    accelerator: "Numpad1".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::ClassifyVoiceover,
                    accelerator: "2".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::ClassifyVoiceover,
                    accelerator: "Numpad2".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::ClassifySoundEffect,
                    accelerator: "3".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::ClassifySoundEffect,
                    accelerator: "Numpad3".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::ClassifyFoley,
                    accelerator: "4".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::ClassifyFoley,
                    accelerator: "Numpad4".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::ClassifyAmbience,
                    accelerator: "5".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::ClassifyAmbience,
                    accelerator: "Numpad5".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::ClassifyOther,
                    accelerator: "0".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::ClassifyOther,
                    accelerator: "Numpad0".to_string(),
                },
            ],
        }
    }

    pub fn binding_for(&self, command: CommandId) -> Option<&str> {
        self.bindings
            .iter()
            .find(|binding| binding.command == command)
            .map(|binding| binding.accelerator.as_str())
    }

    pub fn validate(&self) -> Result<(), ShortcutError> {
        let mut commands_by_accelerator: BTreeMap<&str, Vec<CommandId>> = BTreeMap::new();

        for binding in &self.bindings {
            commands_by_accelerator
                .entry(binding.accelerator.as_str())
                .or_default()
                .push(binding.command);
        }

        for (accelerator, commands) in commands_by_accelerator {
            if commands.len() > 1 {
                return Err(ShortcutError::Conflict {
                    accelerator: accelerator.to_string(),
                    commands,
                });
            }
        }

        Ok(())
    }
}

impl AppPreferences {
    pub fn default_for_editorial_audio() -> Self {
        Self {
            browser_density: BrowserDensity::Compact,
            preview_cache_limit_mb: 2_048,
            output_device: OutputDevicePreference::SystemDefault,
            shortcuts: ShortcutMap::default_audio_workspace(),
            reduced_motion: false,
            reduced_transparency: false,
            theme: ThemePreference::default(),
            prevent_sleep_during_analysis: true,
            recent_library_files: Vec::new(),
            last_active_library_file_path: None,
        }
    }
}

pub fn normalize_preview_cache_limit_mb(limit_mb: u32) -> u32 {
    limit_mb.max(MIN_PREVIEW_CACHE_LIMIT_MB)
}

pub fn load_preferences(path: impl AsRef<Path>) -> Result<AppPreferences, PreferenceStoreError> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(AppPreferences::default_for_editorial_audio());
    }

    let contents = fs::read_to_string(path)?;
    let mut preferences: AppPreferences = serde_json::from_str(&contents)?;
    preferences.preview_cache_limit_mb =
        normalize_preview_cache_limit_mb(preferences.preview_cache_limit_mb);
    preferences.shortcuts.validate()?;

    Ok(preferences)
}

pub fn save_preferences(
    path: impl AsRef<Path>,
    preferences: &AppPreferences,
) -> Result<(), PreferenceStoreError> {
    preferences.shortcuts.validate()?;
    let mut normalized = preferences.clone();
    normalized.preview_cache_limit_mb =
        normalize_preview_cache_limit_mb(normalized.preview_cache_limit_mb);

    let path = path.as_ref();
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, serde_json::to_string_pretty(&normalized)?)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn default_shortcuts_cover_keyboard_led_auditioning() {
        let shortcuts = ShortcutMap::default_audio_workspace();

        assert_eq!(
            shortcuts.binding_for(CommandId::TogglePlayback),
            Some("Space")
        );
        assert_eq!(
            shortcuts.binding_for(CommandId::NextAsset),
            Some("ArrowDown")
        );
        assert_eq!(
            shortcuts.binding_for(CommandId::PreviousAsset),
            Some("ArrowUp")
        );
        assert_eq!(
            shortcuts.binding_for(CommandId::CommandPalette),
            Some("Mod+K")
        );
        assert_eq!(shortcuts.binding_for(CommandId::Import), Some("Mod+I"));
    }

    #[test]
    fn default_shortcuts_have_no_internal_conflicts() {
        assert_eq!(ShortcutMap::default_audio_workspace().validate(), Ok(()));
    }

    #[test]
    fn next_and_previous_asset_each_have_a_second_letter_key_binding() {
        let shortcuts = ShortcutMap::default_audio_workspace();

        // binding_for only returns the first match — the arrow keys stay
        // primary — but both accelerators must be present in the full list
        // for D/A to actually fire NextAsset/PreviousAsset at runtime.
        assert!(shortcuts
            .bindings
            .iter()
            .any(|binding| binding.command == CommandId::NextAsset && binding.accelerator == "D"));
        assert!(shortcuts
            .bindings
            .iter()
            .any(|binding| binding.command == CommandId::PreviousAsset && binding.accelerator == "A"));
    }

    #[test]
    fn toggle_reviewed_is_bound_to_mod_r() {
        assert_eq!(
            ShortcutMap::default_audio_workspace().binding_for(CommandId::ToggleReviewed),
            Some("Mod+R")
        );
    }

    #[test]
    fn classify_shortcuts_cover_digits_zero_through_five() {
        let shortcuts = ShortcutMap::default_audio_workspace();

        assert_eq!(
            shortcuts.binding_for(CommandId::ClassifySoundtrack),
            Some("1")
        );
        assert_eq!(
            shortcuts.binding_for(CommandId::ClassifyVoiceover),
            Some("2")
        );
        assert_eq!(
            shortcuts.binding_for(CommandId::ClassifySoundEffect),
            Some("3")
        );
        assert_eq!(shortcuts.binding_for(CommandId::ClassifyFoley), Some("4"));
        assert_eq!(
            shortcuts.binding_for(CommandId::ClassifyAmbience),
            Some("5")
        );
        assert_eq!(shortcuts.binding_for(CommandId::ClassifyOther), Some("0"));
    }

    #[test]
    fn classify_shortcuts_also_bind_the_numeric_keypad() {
        let shortcuts = ShortcutMap::default_audio_workspace();
        let numpad_pairs = [
            (CommandId::ClassifySoundtrack, "Numpad1"),
            (CommandId::ClassifyVoiceover, "Numpad2"),
            (CommandId::ClassifySoundEffect, "Numpad3"),
            (CommandId::ClassifyFoley, "Numpad4"),
            (CommandId::ClassifyAmbience, "Numpad5"),
            (CommandId::ClassifyOther, "Numpad0"),
        ];

        for (command, accelerator) in numpad_pairs {
            assert!(
                shortcuts
                    .bindings
                    .iter()
                    .any(|binding| binding.command == command && binding.accelerator == accelerator),
                "missing numpad binding {accelerator} for {command:?}"
            );
        }
    }

    #[test]
    fn shortcut_validation_reports_conflicting_bindings() {
        let shortcuts = ShortcutMap {
            bindings: vec![
                ShortcutBinding {
                    command: CommandId::TogglePlayback,
                    accelerator: "Space".to_string(),
                },
                ShortcutBinding {
                    command: CommandId::PreviewSelected,
                    accelerator: "Space".to_string(),
                },
            ],
        };

        assert_eq!(
            shortcuts.validate(),
            Err(ShortcutError::Conflict {
                accelerator: "Space".to_string(),
                commands: vec![CommandId::TogglePlayback, CommandId::PreviewSelected],
            })
        );
    }

    #[test]
    fn app_preferences_keep_cache_density_and_output_device_together() {
        let preferences = AppPreferences::default_for_editorial_audio();

        assert_eq!(preferences.browser_density, BrowserDensity::Compact);
        assert_eq!(preferences.preview_cache_limit_mb, 2_048);
        assert_eq!(
            preferences.output_device,
            OutputDevicePreference::SystemDefault
        );
        assert_eq!(
            preferences.shortcuts.binding_for(CommandId::ToggleFavorite),
            Some("F")
        );
    }

    #[test]
    fn preview_cache_limit_has_a_practical_floor() {
        assert_eq!(
            normalize_preview_cache_limit_mb(128),
            MIN_PREVIEW_CACHE_LIMIT_MB
        );
    }

    #[test]
    fn missing_preferences_file_loads_editorial_defaults() {
        let preferences =
            load_preferences(unique_preferences_path("missing")).expect("load defaults");

        assert_eq!(preferences.browser_density, BrowserDensity::Compact);
        assert_eq!(preferences.preview_cache_limit_mb, 2_048);
        assert!(!preferences.reduced_motion);
        assert!(!preferences.reduced_transparency);
    }

    #[test]
    fn accessibility_toggles_round_trip_through_saved_preferences() {
        let path = unique_preferences_path("accessibility");
        let mut preferences = AppPreferences::default_for_editorial_audio();
        preferences.reduced_motion = true;
        preferences.reduced_transparency = true;

        save_preferences(&path, &preferences).expect("save");
        let loaded = load_preferences(&path).expect("load");

        assert!(loaded.reduced_motion);
        assert!(loaded.reduced_transparency);
    }

    #[test]
    fn preferences_without_accessibility_fields_default_to_false() {
        let path = unique_preferences_path("legacy-file");
        std::fs::write(
            &path,
            r#"{"browser_density":"Compact","preview_cache_limit_mb":16384,"output_device":"SystemDefault","shortcuts":{"bindings":[]}}"#,
        )
        .expect("write legacy preferences file");

        let loaded = load_preferences(&path).expect("load legacy file");

        assert!(!loaded.reduced_motion);
        assert!(!loaded.reduced_transparency);
        assert_eq!(loaded.theme, ThemePreference::Dark);
        assert!(loaded.recent_library_files.is_empty());
        assert_eq!(loaded.last_active_library_file_path, None);
    }

    #[test]
    fn recording_a_project_opened_moves_it_to_front_and_sets_last_active() {
        let mut preferences = AppPreferences::default_for_editorial_audio();
        preferences.record_library_file_opened("/a/One.darkwave", "One", 100);
        preferences.record_library_file_opened("/b/Two.darkwave", "Two", 200);

        assert_eq!(preferences.recent_library_files.len(), 2);
        assert_eq!(preferences.recent_library_files[0].path, "/b/Two.darkwave");
        assert_eq!(preferences.recent_library_files[1].path, "/a/One.darkwave");
        assert_eq!(
            preferences.last_active_library_file_path,
            Some("/b/Two.darkwave".to_string())
        );

        // Reopening an already-recent project de-duplicates instead of
        // appending a second entry, and moves it back to the front.
        preferences.record_library_file_opened("/a/One.darkwave", "One", 300);
        assert_eq!(preferences.recent_library_files.len(), 2);
        assert_eq!(preferences.recent_library_files[0].path, "/a/One.darkwave");
    }

    #[test]
    fn recent_library_files_are_capped() {
        let mut preferences = AppPreferences::default_for_editorial_audio();
        for i in 0..(MAX_RECENT_LIBRARY_FILES + 5) {
            preferences.record_library_file_opened(format!("/p{i}.darkwave"), format!("P{i}"), i as u64);
        }

        assert_eq!(preferences.recent_library_files.len(), MAX_RECENT_LIBRARY_FILES);
        // Most recently opened stays first.
        assert_eq!(
            preferences.recent_library_files[0].path,
            format!("/p{}.darkwave", MAX_RECENT_LIBRARY_FILES + 4)
        );
    }

    #[test]
    fn adding_a_recent_library_file_does_not_set_it_as_last_active() {
        let mut preferences = AppPreferences::default_for_editorial_audio();
        preferences.add_recent_library_file("/a/Migrated.darkwave", "Migrated", 100);

        assert_eq!(preferences.recent_library_files.len(), 1);
        assert_eq!(preferences.recent_library_files[0].path, "/a/Migrated.darkwave");
        assert_eq!(preferences.last_active_library_file_path, None);
    }

    #[test]
    fn forgetting_a_project_removes_it_and_clears_last_active_if_matched() {
        let mut preferences = AppPreferences::default_for_editorial_audio();
        preferences.record_library_file_opened("/a/One.darkwave", "One", 100);

        preferences.forget_library_file("/a/One.darkwave");

        assert!(preferences.recent_library_files.is_empty());
        assert_eq!(preferences.last_active_library_file_path, None);
    }

    #[test]
    fn theme_preference_round_trips_through_saved_preferences() {
        let path = unique_preferences_path("theme");
        let mut preferences = AppPreferences::default_for_editorial_audio();
        assert_eq!(preferences.theme, ThemePreference::Dark);

        preferences.theme = ThemePreference::Light;
        save_preferences(&path, &preferences).expect("save");
        let loaded = load_preferences(&path).expect("load");

        assert_eq!(loaded.theme, ThemePreference::Light);
    }

    #[test]
    fn preferences_round_trip_with_normalized_cache_floor() {
        let path = unique_preferences_path("round-trip");
        let mut preferences = AppPreferences::default_for_editorial_audio();
        preferences.browser_density = BrowserDensity::Comfortable;
        preferences.preview_cache_limit_mb = 128;

        save_preferences(&path, &preferences).expect("save");
        let loaded = load_preferences(&path).expect("load");

        assert_eq!(loaded.browser_density, BrowserDensity::Comfortable);
        assert_eq!(loaded.preview_cache_limit_mb, MIN_PREVIEW_CACHE_LIMIT_MB);
        assert_eq!(
            loaded.shortcuts.binding_for(CommandId::ExportSelected),
            Some("Mod+E")
        );
    }

    fn unique_preferences_path(name: &str) -> PathBuf {
        let mut path = std::env::temp_dir();
        path.push(format!(
            "darkwave-preferences-{name}-{}.json",
            std::process::id()
        ));
        let _ = fs::remove_file(&path);
        path
    }
}
