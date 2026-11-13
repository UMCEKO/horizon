// game-module-cfg: feature java-mods
//! Container loot, blocked chests and brushing that mods' mixins can patch (Lootr's per-player loot).

use super::java_routing::{
    ContainerArg, FilledPlayerEntity, LevelArg, LootParamsArg, NoPlayer, Recv, route,
};
use super::*;
use horizon_client::items::ItemStack;
use horizon_core::position::BlockPos;

type Pos = (i32, i32, i32);

fn block_pos(key: Pos) -> BlockPos {
    BlockPos::new(key.0, key.1, key.2)
}

impl Game<'_> {
    /// `RandomizableContainer.unpackLootTable(player)` of the container at `key`, for the acting player or for none (a hopper, a command, a break).
    pub(in crate::game) fn unpack_container_loot(&mut self, key: Pos, player: bool) {
        if player {
            self.unpack_container_loot_for_player(key);
        } else {
            self.unpack_container_loot_alone(key);
        }
    }

    fn unpack_container_loot_for_player(&mut self, key: Pos) {
        route!(
            self,
            "net/minecraft/world/RandomizableContainer#unpackLootTable(Lnet/minecraft/world/entity/player/Player;)V",
            Recv::BlockEntity(block_pos(key)),
            (self.java_player_arg()),
            block = self.world.block(key.0, key.1, key.2)
        );
        self.unpack_container_loot_body(key, true);
    }

    fn unpack_container_loot_alone(&mut self, key: Pos) {
        route!(
            self,
            "net/minecraft/world/RandomizableContainer#unpackLootTable(Lnet/minecraft/world/entity/player/Player;)V",
            Recv::BlockEntity(block_pos(key)),
            (NoPlayer),
            block = self.world.block(key.0, key.1, key.2)
        );
        self.unpack_container_loot_body(key, false);
    }

    /// `LootTable.fill` of `table` into the container at `key`, from `unpackLootTable`'s body.
    pub(in crate::game) fn loot_table_fill(
        &mut self,
        key: Pos,
        table: &str,
        seed: i64,
        luck: f32,
        player: bool,
    ) {
        route!(
            self,
            "net/minecraft/world/level/storage/loot/LootTable#fill(Lnet/minecraft/world/Container;Lnet/minecraft/world/level/storage/loot/LootParams;J)V",
            Recv::LootTable(table),
            (
                ContainerArg {
                    dim: self.dim.id(),
                    pos: block_pos(key)
                },
                LootParamsArg {
                    dim: self.dim.id(),
                    origin: [key.0, key.1, key.2].map(|v| f64::from(v) + 0.5),
                    luck,
                    player: player.then(|| self.java_player_arg()),
                },
                seed
            )
        );
        self.loot_table_fill_body(key, table, seed, luck);
    }

    /// `ChestBlock.isBlockedChestByBlock` for the chest at `key`.
    pub(in crate::game) fn chest_blocked_by_block(&mut self, key: Pos) -> bool {
        route!(
            self,
            "net/minecraft/world/level/block/ChestBlock#isBlockedChestByBlock(Lnet/minecraft/world/level/BlockGetter;Lnet/minecraft/core/BlockPos;)Z",
            Recv::Static,
            (LevelArg(self.dim.id()), block_pos(key)),
            block = self.world.block(key.0, key.1, key.2)
        );
        self.chest_blocked_by_block_body(key)
    }

    /// `BrushItem.onUseTick` of the acting player's brush, used `ticks` ticks so far from `slot`.
    pub(in crate::game) fn brush_use_tick(&mut self, stack: &ItemStack, slot: usize, ticks: u32) {
        route!(
            self,
            "net/minecraft/world/item/BrushItem#onUseTick(Lnet/minecraft/world/level/Level;Lnet/minecraft/world/entity/LivingEntity;Lnet/minecraft/world/item/ItemStack;I)V",
            Recv::UsedItem,
            (
                LevelArg(self.dim.id()),
                FilledPlayerEntity,
                stack,
                super::brush::ticks_remaining(ticks)
            ),
            item = &*stack.item
        );
        self.brush_use_tick_body(stack, slot, ticks);
    }
}

#[cfg(test)]
mod tests {
    use super::super::java_routing::{
        ContainerArg, FilledPlayerEntity, LootParamsArg, NoPlayer, PlayerArg, RouteArg,
    };
    use horizon_core::position::BlockPos;
    use horizon_java_host::wire::{In, Out};

    #[test]
    fn loot_args_write_as_routing_reads_them() {
        let mut out = Out::default();
        RouteArg::put(&NoPlayer, &mut out);
        RouteArg::put(&FilledPlayerEntity, &mut out);
        RouteArg::put(
            &ContainerArg {
                dim: "minecraft:overworld",
                pos: BlockPos::new(1, 64, -2),
            },
            &mut out,
        );
        RouteArg::put(
            &LootParamsArg {
                dim: "minecraft:overworld",
                origin: [1.5, 64.5, -1.5],
                luck: 2.0,
                player: Some(PlayerArg(Some(vec![9, 9]))),
            },
            &mut out,
        );
        let mut r = In(&out.0);
        // Absent player, the filled player's id, then the container.
        assert_eq!(r.bool(), Some(false));
        assert_eq!(r.int(), Some(-1));
        assert_eq!(r.text(), Some("minecraft:overworld"));
        assert_eq!(r.long(), Some(BlockPos::new(1, 64, -2).pack()));
        // The params: level, origin, luck, a present player and its fill.
        assert_eq!(r.text(), Some("minecraft:overworld"));
        assert_eq!(
            [r.double(), r.double(), r.double()],
            [Some(1.5), Some(64.5), Some(-1.5)]
        );
        assert_eq!(r.float(), Some(2.0));
        assert_eq!(r.bool(), Some(true));
        assert_eq!((r.byte(), r.byte(), r.byte()), (Some(9), Some(9), None));
    }
}
