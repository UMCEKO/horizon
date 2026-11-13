// game-module-cfg: feature java-mods
//! The native bodies Java's merged use methods call back for: `Item.use` of a vanilla item, `finishUsingItem` and `useWithoutItem`.

use super::*;
use crate::game::java_routing::HitArg;
use crate::game::java_use::encode;
use horizon_java_host::bridge::Doubles;

horizon_java_host::java_bridges! { table for Game<'static> {
    "dev/horizonlab/javamods/boot/NativeUse".nativeUse
    "()I" =>
    fn native_item_use(game) -> i32 {
        let result = game.item_use_body();
        game.java_refresh_player();
        encode(result)
    }

    route "net/minecraft/world/item/ItemStack".finishUsingItem
    "(Lnet/minecraft/world/level/Level;Lnet/minecraft/world/entity/LivingEntity;)Lnet/minecraft/world/item/ItemStack;"
    if "@dev/horizonlab/javamods/boot/Routing.servedStack+" =>
    fn route_item_stack_finish_using(game, _level: Skip, _entity: Skip) -> Bytes {
        let stack = game.finish_using_body();
        game.java_refresh_player();
        Bytes(
            stack
                .and_then(|s| java_mods::write_tag(horizon_client::save::item_to_nbt(&s)))
                .unwrap_or_default(),
        )
    }

    route "net/minecraft/world/level/block/state/BlockBehaviour$BlockStateBase".useWithoutItem
    "(Lnet/minecraft/world/level/Level;Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/phys/BlockHitResult;)Lnet/minecraft/world/InteractionResult;"
    if "@dev/horizonlab/javamods/boot/Routing.servedBlock+" =>
    fn route_block_use_without_item(game, _level: Skip, _player: Skip, hit: Doubles) -> i32 {
        let Some(hit) = HitArg::read(&hit.0) else {
            panic!("Horizon: useWithoutItem got a hit of {} numbers", hit.0.len());
        };
        let state = game.world.block(hit.pos[0], hit.pos[1], hit.pos[2]);
        let result = game.use_without_item_body(state, &hit, crate::game::player_use::geometry_hit(&hit));
        game.java_refresh_player();
        encode(result)
    }
}}
