use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

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
            preview_cache_limit_mb: 16_384,
            output_device: OutputDevicePreference::SystemDefault,
            shortcuts: ShortcutMap::default_audio_workspace(),
        }
    }
}

pub fn normalize_preview_cache_limit_mb(limit_mb: u32) -> u32 {
    limit_mb.max(MIN_PREVIEW_CACHE_LIMIT_MB)
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(preferences.preview_cache_limit_mb, 16_384);
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
}
