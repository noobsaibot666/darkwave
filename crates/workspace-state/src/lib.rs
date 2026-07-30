use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum SelectionMode {
    Replace,
    Toggle,
    Range,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum BrowserCommand {
    MoveSelection { delta: isize },
    FocusRow { index: usize },
    SelectFocused { mode: SelectionMode },
    SelectAllVisible,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DragTarget {
    Tag(String),
    Collection(Uuid),
    Project(Uuid),
    Favorite,
    Trash,
    ExternalExport,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct DragPayload {
    pub asset_ids: Vec<Uuid>,
    pub target: DragTarget,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct BrowserState {
    visible_asset_ids: Vec<Uuid>,
    focused_index: usize,
    anchor_index: usize,
    selected_indices: BTreeSet<usize>,
}

impl BrowserState {
    pub fn new(visible_asset_ids: Vec<Uuid>) -> Self {
        let selected_indices = (!visible_asset_ids.is_empty())
            .then_some(0)
            .into_iter()
            .collect();

        Self {
            visible_asset_ids,
            focused_index: 0,
            anchor_index: 0,
            selected_indices,
        }
    }

    pub fn apply(&mut self, command: BrowserCommand) {
        match command {
            BrowserCommand::MoveSelection { delta } => {
                self.focused_index = self.clamped_index(self.focused_index as isize + delta);
                self.selected_indices.clear();
                self.selected_indices.insert(self.focused_index);
                self.anchor_index = self.focused_index;
            }
            BrowserCommand::FocusRow { index } => {
                self.focused_index = self.clamped_index(index as isize);
            }
            BrowserCommand::SelectFocused { mode } => match mode {
                SelectionMode::Replace => {
                    self.selected_indices.clear();
                    self.selected_indices.insert(self.focused_index);
                    self.anchor_index = self.focused_index;
                }
                SelectionMode::Toggle => {
                    if !self.selected_indices.remove(&self.focused_index) {
                        self.selected_indices.insert(self.focused_index);
                    }
                    self.anchor_index = self.focused_index;
                }
                SelectionMode::Range => {
                    self.selected_indices.clear();
                    let start = self.anchor_index.min(self.focused_index);
                    let end = self.anchor_index.max(self.focused_index);
                    self.selected_indices.extend(start..=end);
                }
            },
            BrowserCommand::SelectAllVisible => {
                self.selected_indices = (0..self.visible_asset_ids.len()).collect();
            }
        }
    }

    pub fn focused_asset_id(&self) -> Option<Uuid> {
        self.visible_asset_ids.get(self.focused_index).copied()
    }

    pub fn selected_asset_ids(&self) -> Vec<Uuid> {
        self.selected_indices
            .iter()
            .filter_map(|index| self.visible_asset_ids.get(*index).copied())
            .collect()
    }

    pub fn drag_payload(&self, target: DragTarget) -> DragPayload {
        DragPayload {
            asset_ids: self.selected_asset_ids(),
            target,
        }
    }

    fn clamped_index(&self, index: isize) -> usize {
        let max_index = self.visible_asset_ids.len().saturating_sub(1) as isize;
        index.clamp(0, max_index) as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    fn asset_ids(count: usize) -> Vec<Uuid> {
        (0..count).map(|_| Uuid::new_v4()).collect()
    }

    #[test]
    fn arrow_navigation_moves_focus_without_overlapping_selection() {
        let ids = asset_ids(3);
        let mut browser = BrowserState::new(ids.clone());

        browser.apply(BrowserCommand::MoveSelection { delta: 1 });

        assert_eq!(browser.focused_asset_id(), Some(ids[1]));
        assert_eq!(browser.selected_asset_ids(), vec![ids[1]]);
    }

    #[test]
    fn shift_selection_extends_from_anchor_to_focused_row() {
        let ids = asset_ids(5);
        let mut browser = BrowserState::new(ids.clone());

        browser.apply(BrowserCommand::SelectFocused {
            mode: SelectionMode::Replace,
        });
        browser.apply(BrowserCommand::FocusRow { index: 3 });
        browser.apply(BrowserCommand::SelectFocused {
            mode: SelectionMode::Range,
        });

        assert_eq!(browser.selected_asset_ids(), ids[0..=3].to_vec());
    }

    #[test]
    fn additive_selection_toggles_without_losing_existing_rows() {
        let ids = asset_ids(4);
        let mut browser = BrowserState::new(ids.clone());

        browser.apply(BrowserCommand::SelectFocused {
            mode: SelectionMode::Replace,
        });
        browser.apply(BrowserCommand::FocusRow { index: 2 });
        browser.apply(BrowserCommand::SelectFocused {
            mode: SelectionMode::Toggle,
        });

        assert_eq!(browser.selected_asset_ids(), vec![ids[0], ids[2]]);
    }

    #[test]
    fn select_all_visible_uses_current_result_order() {
        let ids = asset_ids(4);
        let mut browser = BrowserState::new(ids.clone());

        browser.apply(BrowserCommand::SelectAllVisible);

        assert_eq!(browser.selected_asset_ids(), ids);
    }

    #[test]
    fn drag_payload_preserves_ordered_selected_assets_and_target() {
        let ids = asset_ids(3);
        let mut browser = BrowserState::new(ids.clone());
        browser.apply(BrowserCommand::SelectAllVisible);

        assert_eq!(
            browser.drag_payload(DragTarget::Tag("impact".to_string())),
            DragPayload {
                asset_ids: ids,
                target: DragTarget::Tag("impact".to_string()),
            }
        );
    }
}
