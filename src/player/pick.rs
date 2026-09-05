use crate::world::{block, catalog};

pub fn pick_block(
    hotbar: &mut [catalog::ItemId; 9],
    selected: &mut usize,
    block_id: block::BlockId,
) -> bool {
    if block_id == block::AIR {
        return false;
    }
    let item = catalog::all_items()
        .find(|x| {
            x.icon_block == block_id
                || block::canonical_base(x.icon_block) == block::canonical_base(block_id)
        })
        .map(|x| x.id);
    let Some(item) = item else {
        return false;
    };
    if let Some(slot) = hotbar.iter().position(|&v| v == item) {
        *selected = slot;
    } else {
        hotbar[*selected] = item;
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pick_block_selects_existing_hotbar_slot() {
        let mut hotbar = catalog::DEFAULT_HOTBAR_ITEMS;
        let mut selected = 4;
        assert!(pick_block(&mut hotbar, &mut selected, block::STONE));
        assert_eq!(selected, 0);
        assert_eq!(hotbar[selected], 4);
    }

    #[test]
    fn pick_block_replaces_selected_slot_when_absent() {
        let mut hotbar = catalog::DEFAULT_HOTBAR_ITEMS;
        let mut selected = 2;
        assert!(pick_block(&mut hotbar, &mut selected, block::BRICK));
        assert_eq!(selected, 2);
        assert_eq!(hotbar[selected], 75);
    }
}
