use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub enum CommandId {
    Import,
    ApplyTag,
    AddToCollection,
    Export,
    Reveal,
    Convert,
    Rescan,
    OpenSettings,
    RunMaintenance,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PaletteCommand {
    pub id: CommandId,
    pub title: String,
    pub category: String,
    pub keywords: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct CommandRegistry {
    commands: Vec<PaletteCommand>,
}

impl CommandRegistry {
    pub fn default_audio_workspace() -> Self {
        Self {
            commands: vec![
                command(
                    CommandId::Import,
                    "Import Folder",
                    "Import",
                    &["files", "folder", "add"],
                ),
                command(
                    CommandId::ApplyTag,
                    "Apply Tag",
                    "Organize",
                    &["tag", "classify", "label"],
                ),
                command(
                    CommandId::AddToCollection,
                    "Add to Collection",
                    "Organize",
                    &["collection", "project", "group"],
                ),
                command(
                    CommandId::Export,
                    "Export Selected",
                    "Export",
                    &["copy", "project", "drag"],
                ),
                command(
                    CommandId::Reveal,
                    "Reveal Original",
                    "File",
                    &["finder", "explorer", "source"],
                ),
                command(
                    CommandId::Convert,
                    "Convert to WAV",
                    "Export",
                    &["wav", "render", "preset"],
                ),
                command(
                    CommandId::Rescan,
                    "Rescan Library",
                    "Maintenance",
                    &["sync", "refresh", "nas"],
                ),
                command(
                    CommandId::OpenSettings,
                    "Open Settings",
                    "Settings",
                    &["preferences", "shortcuts", "audio"],
                ),
                command(
                    CommandId::RunMaintenance,
                    "Run Maintenance",
                    "Maintenance",
                    &["repair", "cleanup", "validate"],
                ),
            ],
        }
    }

    pub fn command_ids(&self) -> Vec<CommandId> {
        self.commands.iter().map(|command| command.id).collect()
    }

    pub fn search(&self, query: &str) -> Vec<PaletteCommand> {
        let normalized_query = query.trim().to_lowercase();
        if normalized_query.is_empty() {
            return self.commands.clone();
        }

        let mut matches = self
            .commands
            .iter()
            .filter_map(|command| {
                let score = command.match_score(&normalized_query);
                (score > 0).then_some((score, command.clone()))
            })
            .collect::<Vec<_>>();

        matches.sort_by_key(|(score, command)| (std::cmp::Reverse(*score), command.id));
        matches.into_iter().map(|(_, command)| command).collect()
    }
}

impl PaletteCommand {
    fn match_score(&self, query: &str) -> u8 {
        let title = self.title.to_lowercase();
        let category = self.category.to_lowercase();
        let keyword_match = self
            .keywords
            .iter()
            .any(|keyword| keyword.to_lowercase().contains(query));

        if title.contains(query) {
            3
        } else if keyword_match {
            2
        } else if category.contains(query) {
            1
        } else {
            0
        }
    }
}

fn command(id: CommandId, title: &str, category: &str, keywords: &[&str]) -> PaletteCommand {
    PaletteCommand {
        id,
        title: title.to_string(),
        category: category.to_string(),
        keywords: keywords.iter().map(|keyword| keyword.to_string()).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_registry_contains_major_plan_actions() {
        let registry = CommandRegistry::default_audio_workspace();
        let ids = registry.command_ids();

        assert!(ids.contains(&CommandId::Import));
        assert!(ids.contains(&CommandId::ApplyTag));
        assert!(ids.contains(&CommandId::AddToCollection));
        assert!(ids.contains(&CommandId::Export));
        assert!(ids.contains(&CommandId::Reveal));
        assert!(ids.contains(&CommandId::Convert));
        assert!(ids.contains(&CommandId::Rescan));
        assert!(ids.contains(&CommandId::OpenSettings));
        assert!(ids.contains(&CommandId::RunMaintenance));
    }

    #[test]
    fn search_matches_title_keywords_and_category() {
        let registry = CommandRegistry::default_audio_workspace();
        let results = registry.search("tag");

        assert_eq!(results[0].id, CommandId::ApplyTag);
    }

    #[test]
    fn search_returns_keyboard_friendly_order_for_empty_query() {
        let registry = CommandRegistry::default_audio_workspace();
        let results = registry.search("");

        assert_eq!(results[0].id, CommandId::Import);
        assert_eq!(results[1].id, CommandId::ApplyTag);
    }
}
