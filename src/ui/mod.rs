//! Pure UI state and first-person viewmodel contracts.
//!
//! Rendering backends should consume these small, deterministic values.  This
//! module deliberately has no font, window, or GPU dependency.

pub mod inventory;
pub mod layout;
pub mod viewmodel;

pub use inventory::{
    CreativeInventory, InventoryEvent, InventoryInput, ItemCategory, ItemId, ItemRecord,
};
pub use layout::{DesignRect, HudLayout, UiLayout, UiScale};
pub use viewmodel::{ActionRequest, HandAction, HandTransform, ViewModelState};
