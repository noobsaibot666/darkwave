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
    FocusSearch,
    ExportSelected,
    ToggleLoop,
    CopyPath,
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
    /// A single folder (matching the plan's "Watched Downloads folder,"
    /// singular) the standing background worker polls for new, stable
    /// audio files and imports automatically. `None` disables watching.
    #[serde(default)]
    pub watched_folder_path: Option<String>,
    /// Which library newly-discovered watched-folder files import into —
    /// required alongside `watched_folder_path` since preferences are
    /// global but import is always library-scoped.
    #[serde(default)]
    pub watched_folder_library_id: Option<String>,
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
                ShortcutBinding {
                    command: CommandId::PreviousAsset,
                    accelerator: "ArrowUp".to_string(),
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
                    command: CommandId::FocusSearch,
                    accelerator: "Mod+F".to_string(),
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
            watched_folder_path: None,
            watched_folder_library_id: None,
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
        assert_eq!(loaded.watched_folder_path, None);
        assert_eq!(loaded.watched_folder_library_id, None);
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
