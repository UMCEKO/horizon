// game-module-cfg: feature java-mods
//! The tool of a native loot roll with the enchantment levels Java gives it (`Loot.effective`).

use super::java_crafting::craft_facts;
use super::*;
use horizon_java_host::wire::In;

impl Game<'_> {
    /// A copy of the tool with Java's enchantment levels, for one roll; None unless item code or a mod's event answers them.
    pub(super) fn java_tool_for_loot(&mut self, tool: &ItemStack) -> Option<ItemStack> {
        if !craft_facts()?.tool_levels_from_java(&tool.item) {
            return None;
        }
        let nbt = super::java_mods::write_tag(horizon_items::item_nbt::item_to_nbt(tool))?;
        let key = self.java_level_key();
        let time = self.world.game_time;
        let levels = self.java_call_with(
            "tool_enchantments",
            |out| {
                out.int(key as i32);
                out.long(time);
                out.bytes(&nbt);
            },
            read_levels,
        )?;
        let mut swapped = tool.clone();
        swapped.data_mut().enchantments = levels;
        Some(swapped)
    }
}

fn read_levels(input: &mut In<'_>) -> Option<Vec<(Arc<str>, u32)>> {
    let mut levels = Vec::new();
    for _ in 0..input.int()? {
        let id = input.text()?;
        levels.push((Arc::from(id), input.int()? as u32));
    }
    Some(levels)
}

#[cfg(test)]
mod tests {
    use super::*;
    use horizon_java_host::wire::Out;

    #[test]
    fn the_reply_reads_in_order() {
        let mut out = Out::default();
        out.int(2);
        out.text("minecraft:fortune");
        out.int(3);
        out.text("minecraft:silk_touch");
        out.int(1);
        let levels = read_levels(&mut In(&out.0)).unwrap();
        assert_eq!(
            levels,
            vec![
                (Arc::from("minecraft:fortune"), 3),
                (Arc::from("minecraft:silk_touch"), 1)
            ]
        );
    }
}
