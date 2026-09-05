use crate::world::catalog::DEFAULT_HOTBAR_ITEMS;
use std::cmp::Ordering;

pub type ItemId = u16;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ItemCategory {
    Terrain,
    Masonry,
    Wood,
    Color,
    GlassLight,
    Detail,
    Shapes,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ItemRecord {
    pub id: ItemId,
    pub name: &'static str,
    pub search_terms: &'static [&'static str],
    pub category: ItemCategory,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InventoryInput {
    Toggle,
    Escape,
    Number(u8),
    Wheel { delta: i32 },
    Backspace,
    Text(char),
    SearchFocus,
    ClickItem(ItemId),
    ClickHotbar(u8),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum InventoryEvent {
    InventoryOpened,
    InventoryClosed,
    PauseRequested,
    Switch { slot: u8, from: ItemId, to: ItemId },
    HotbarSelected(u8),
    QueryChanged,
    SearchFocused,
}

/// State for the creative inventory modal and the always-visible nine-slot HUD.
/// The catalog remains owned by the block/item registry; callers pass records to
/// `filtered_items` so this type stays independent of rendering and catalog code.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CreativeInventory {
    pub open: bool,
    pub category: Option<ItemCategory>,
    pub query: String,
    pub scroll_row: u16,
    pub hovered: Option<ItemId>,
    pub hotbar: [ItemId; 9],
    pub selected_slot: u8,
    pub search_focused: bool,
}

impl Default for CreativeInventory {
    fn default() -> Self {
        Self {
            open: false,
            category: None,
            query: String::new(),
            scroll_row: 0,
            hovered: None,
            hotbar: DEFAULT_HOTBAR_ITEMS,
            selected_slot: 0,
            search_focused: false,
        }
    }
}

impl CreativeInventory {
    pub const GRID_COLUMNS: usize = 9;
    pub const GRID_ROWS: usize = 6;
    pub const QUERY_MAX_BYTES: usize = 32;

    pub fn filtered_items<'a>(&self, records: &'a [ItemRecord]) -> Vec<&'a ItemRecord> {
        let query = self.query.as_str();
        let mut result: Vec<_> = records
            .iter()
            .filter(|item| {
                self.category
                    .is_none_or(|category| item.category == category)
            })
            .filter(|item| {
                query.is_empty()
                    || contains_ascii_case_insensitive(item.name, query)
                    || item
                        .search_terms
                        .iter()
                        .any(|term| contains_ascii_case_insensitive(term, query))
            })
            .collect();
        result.sort_unstable_by_key(|item| item.id);
        result
    }

    pub fn max_scroll_row(&self, records: &[ItemRecord]) -> u16 {
        let count = self.filtered_items(records).len();
        let rows = count.div_ceil(Self::GRID_COLUMNS);
        rows.saturating_sub(Self::GRID_ROWS).min(u16::MAX as usize) as u16
    }

    pub fn clamp_scroll(&mut self, records: &[ItemRecord]) {
        self.scroll_row = self.scroll_row.min(self.max_scroll_row(records));
    }

    pub fn scroll_by(&mut self, records: &[ItemRecord], rows: i32) {
        let max = self.max_scroll_row(records) as i32;
        self.scroll_row = (self.scroll_row as i32 + rows).clamp(0, max) as u16;
    }

    pub fn visible_items<'a>(&self, records: &'a [ItemRecord]) -> Vec<&'a ItemRecord> {
        let start = usize::from(self.scroll_row) * Self::GRID_COLUMNS;
        self.filtered_items(records)
            .into_iter()
            .skip(start)
            .take(Self::GRID_COLUMNS * Self::GRID_ROWS)
            .collect()
    }

    pub fn set_query_text(&mut self, text: &str) -> InventoryEvent {
        self.query.clear();
        for character in text.chars() {
            self.push_query_character(character);
        }
        InventoryEvent::QueryChanged
    }

    pub fn push_query_character(&mut self, character: char) -> bool {
        if !character.is_ascii() || !((' '..='~').contains(&character)) {
            return false;
        }
        let character = character.to_ascii_lowercase();
        if self.query.len() < Self::QUERY_MAX_BYTES {
            self.query.push(character);
            true
        } else {
            false
        }
    }

    pub fn backspace_query(&mut self) -> bool {
        self.query.pop().is_some()
    }

    pub fn assign_item(&mut self, item: ItemId) -> Option<InventoryEvent> {
        let slot = usize::from(self.selected_slot);
        let from = self.hotbar[slot];
        if from == item {
            return None;
        }
        self.hotbar[slot] = item;
        Some(InventoryEvent::Switch {
            slot: self.selected_slot,
            from,
            to: item,
        })
    }

    pub fn assign_hovered_to_slot(&mut self, slot: u8) -> Option<InventoryEvent> {
        let item = self.hovered?;
        if slot >= self.hotbar.len() as u8 {
            return None;
        }
        let index = usize::from(slot);
        let from = self.hotbar[index];
        if from == item {
            return None;
        }
        self.hotbar[index] = item;
        Some(InventoryEvent::Switch {
            slot,
            from,
            to: item,
        })
    }

    pub fn handle_input(
        &mut self,
        input: InventoryInput,
        records: &[ItemRecord],
    ) -> Option<InventoryEvent> {
        match input {
            InventoryInput::Toggle => {
                self.open = !self.open;
                self.search_focused = false;
                Some(if self.open {
                    InventoryEvent::InventoryOpened
                } else {
                    InventoryEvent::InventoryClosed
                })
            }
            InventoryInput::Escape if self.open => {
                self.open = false;
                self.search_focused = false;
                Some(InventoryEvent::InventoryClosed)
            }
            InventoryInput::Escape => Some(InventoryEvent::PauseRequested),
            InventoryInput::Number(number) if (1..=9).contains(&number) && self.open => {
                self.assign_hovered_to_slot(number - 1).or_else(|| {
                    self.search_focused.then(|| {
                        self.push_query_character(char::from(b'0' + number));
                        InventoryEvent::QueryChanged
                    })
                })
            }
            InventoryInput::Number(number) if (1..=9).contains(&number) => {
                self.select_slot(number - 1)
            }
            InventoryInput::Wheel { delta } if self.open => {
                self.scroll_by(records, delta.signum() * 3);
                None
            }
            InventoryInput::Wheel { delta } => self.cycle_slot(delta.signum()),
            InventoryInput::Backspace if self.search_focused => {
                self.backspace_query();
                Some(InventoryEvent::QueryChanged)
            }
            InventoryInput::Text(character) if self.search_focused => {
                self.push_query_character(character);
                Some(InventoryEvent::QueryChanged)
            }
            InventoryInput::SearchFocus => {
                self.search_focused = true;
                Some(InventoryEvent::SearchFocused)
            }
            InventoryInput::ClickItem(item) if self.open => self.assign_item(item),
            InventoryInput::ClickHotbar(slot) if usize::from(slot) < self.hotbar.len() => {
                self.click_hotbar(slot)
            }
            _ => None,
        }
    }

    pub fn select_slot(&mut self, slot: u8) -> Option<InventoryEvent> {
        if slot >= self.hotbar.len() as u8 {
            return None;
        }
        if self.selected_slot == slot {
            return None;
        }
        let from = self.hotbar[usize::from(self.selected_slot)];
        let to = self.hotbar[usize::from(slot)];
        self.selected_slot = slot;
        (from != to).then_some(InventoryEvent::Switch { slot, from, to })
    }

    pub fn click_hotbar(&mut self, slot: u8) -> Option<InventoryEvent> {
        if slot >= self.hotbar.len() as u8 || self.selected_slot == slot {
            return None;
        }
        self.selected_slot = slot;
        Some(InventoryEvent::HotbarSelected(slot))
    }

    fn cycle_slot(&mut self, direction: i32) -> Option<InventoryEvent> {
        let next = (i32::from(self.selected_slot) + direction).rem_euclid(9) as u8;
        self.select_slot(next)
    }
}

fn contains_ascii_case_insensitive(haystack: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    haystack
        .as_bytes()
        .windows(needle.len())
        .any(|window| window.eq_ignore_ascii_case(needle.as_bytes()))
}

impl Ord for ItemCategory {
    fn cmp(&self, other: &Self) -> Ordering {
        (*self as u8).cmp(&(*other as u8))
    }
}

impl PartialOrd for ItemCategory {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn records() -> Vec<ItemRecord> {
        (1..=80)
            .map(|id| ItemRecord {
                id,
                name: if id % 2 == 0 { "stone stair" } else { "oak" },
                search_terms: &["building"],
                category: if id % 2 == 0 {
                    ItemCategory::Shapes
                } else {
                    ItemCategory::Wood
                },
            })
            .collect()
    }

    #[test]
    fn inventory_key_toggles_modal_state() {
        let mut state = CreativeInventory::default();
        assert_eq!(
            state.handle_input(InventoryInput::Toggle, &[]),
            Some(InventoryEvent::InventoryOpened)
        );
        assert!(state.open);
        assert!(!state.search_focused);
        assert_eq!(
            state.handle_input(InventoryInput::Toggle, &[]),
            Some(InventoryEvent::InventoryClosed)
        );
        assert!(!state.open);
    }

    #[test]
    fn search_focus_is_explicit_and_number_precedence_is_item_first() {
        let mut state = CreativeInventory {
            open: true,
            hovered: Some(77),
            ..Default::default()
        };
        assert_eq!(
            state.handle_input(InventoryInput::SearchFocus, &[]),
            Some(InventoryEvent::SearchFocused)
        );
        assert!(state.search_focused);
        assert!(matches!(
            state.handle_input(InventoryInput::Number(9), &[]),
            Some(InventoryEvent::Switch { .. })
        ));
        state.hovered = None;
        assert_eq!(
            state.handle_input(InventoryInput::Number(9), &[]),
            Some(InventoryEvent::QueryChanged)
        );
        assert_eq!(state.query, "9");
    }

    #[test]
    fn inventory_escape_closes_before_pause() {
        let mut state = CreativeInventory::default();
        assert_eq!(
            state.handle_input(InventoryInput::Escape, &[]),
            Some(InventoryEvent::PauseRequested)
        );
        state.open = true;
        assert_eq!(
            state.handle_input(InventoryInput::Escape, &[]),
            Some(InventoryEvent::InventoryClosed)
        );
        assert!(!state.open);
    }

    #[test]
    fn inventory_filter_order_is_stable() {
        let state = CreativeInventory {
            query: "stair".into(),
            ..Default::default()
        };
        let records = records();
        let items = state.filtered_items(&records);
        assert_eq!(
            items.iter().map(|item| item.id).collect::<Vec<_>>(),
            (2..=80).step_by(2).collect::<Vec<_>>()
        );
    }

    #[test]
    fn inventory_scroll_clamps() {
        let records = records();
        let mut state = CreativeInventory {
            scroll_row: u16::MAX,
            ..Default::default()
        };
        state.clamp_scroll(&records);
        assert_eq!(state.scroll_row, 3);
        state.scroll_by(&records, -30);
        assert_eq!(state.scroll_row, 0);
    }

    #[test]
    fn inventory_click_assigns_selected_hotbar_slot() {
        let mut state = CreativeInventory {
            selected_slot: 2,
            ..Default::default()
        };
        assert_eq!(state.handle_input(InventoryInput::ClickItem(42), &[]), None);
        state.open = true;
        assert_eq!(
            state.handle_input(InventoryInput::ClickItem(42), &[]),
            Some(InventoryEvent::Switch {
                slot: 2,
                from: 14,
                to: 42
            })
        );
        assert_eq!(state.hotbar[2], 42);
    }

    #[test]
    fn inventory_number_assigns_requested_slot() {
        let mut state = CreativeInventory {
            open: true,
            hovered: Some(77),
            ..Default::default()
        };
        assert_eq!(
            state.handle_input(InventoryInput::Number(9), &[]),
            Some(InventoryEvent::Switch {
                slot: 8,
                from: 74,
                to: 77
            })
        );
        assert_eq!(state.hotbar[8], 77);
    }
}
