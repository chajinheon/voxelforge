//! Runtime UI state used by surface-free snapshot fixtures.

use super::Options;
use super::args::{HandActionSpec, InventoryCategory, UiMode as SnapshotUiMode};
use super::fixtures::Fixture;
use voxelforge::render::ui::{
    UiFrame, UiMode, UiSettingsDisplay, ViewModelLighting, catalog_records,
};
use voxelforge::ui::{ActionRequest, CreativeInventory, ItemCategory, ItemRecord, ViewModelState};

pub(super) struct SnapshotUi {
    mode: Option<UiMode>,
    inventory: CreativeInventory,
    records: Vec<ItemRecord>,
    viewmodel: ViewModelState,
}

impl SnapshotUi {
    pub(super) fn new(options: &Options) -> Self {
        let mode = match options.ui {
            SnapshotUiMode::None if options.fixture == Fixture::ViewModel => Some(UiMode::Hud),
            SnapshotUiMode::None => None,
            SnapshotUiMode::Inventory => Some(UiMode::Inventory),
            SnapshotUiMode::Hud => Some(UiMode::Hud),
            SnapshotUiMode::Pause => Some(UiMode::Pause),
        };
        let mut inventory = CreativeInventory {
            open: matches!(mode, Some(UiMode::Inventory)),
            category: category(options.inventory_category),
            query: options.inventory_query.to_ascii_lowercase(),
            ..CreativeInventory::default()
        };
        inventory.clamp_scroll(&catalog_records());
        let mut viewmodel = ViewModelState::new(options.held_item);
        let elapsed = match options.hand_action {
            HandActionSpec::Idle => 0.0,
            HandActionSpec::Break { elapsed } => {
                viewmodel.start_action(ActionRequest::Break);
                elapsed
            }
            HandActionSpec::Place { elapsed } => {
                viewmodel.start_action(ActionRequest::Place);
                elapsed
            }
            HandActionSpec::Switch { elapsed } => {
                viewmodel.start_action(ActionRequest::Switch {
                    from: 1,
                    to: options.held_item,
                });
                elapsed
            }
        };
        viewmodel.advance(elapsed, 0.0, false, false, false);
        Self {
            mode,
            inventory,
            records: catalog_records(),
            viewmodel,
        }
    }

    pub(super) fn frame(&self, width: u32, height: u32) -> Option<UiFrame<'_>> {
        self.mode.map(|mode| UiFrame {
            width,
            height,
            scale: 1.0,
            mode,
            inventory: &self.inventory,
            records: &self.records,
            viewmodel: Some(&self.viewmodel),
            viewmodel_lighting: ViewModelLighting::default(),
            settings: (mode == UiMode::Settings).then_some(UiSettingsDisplay {
                render_scale: 0.72,
                taa: true,
                gi_enabled: true,
                ui_scale: 1.0,
            }),
        })
    }
}

fn category(category: InventoryCategory) -> Option<ItemCategory> {
    Some(match category {
        InventoryCategory::All => return None,
        InventoryCategory::Terrain => ItemCategory::Terrain,
        InventoryCategory::Masonry => ItemCategory::Masonry,
        InventoryCategory::Wood => ItemCategory::Wood,
        InventoryCategory::Color => ItemCategory::Color,
        InventoryCategory::Glass => ItemCategory::GlassLight,
        InventoryCategory::Detail => ItemCategory::Detail,
        InventoryCategory::Shapes => ItemCategory::Shapes,
    })
}
