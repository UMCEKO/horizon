// game-module-cfg: feature java-mods
//! The use paths mods patch: `ItemStack.use` and `finishUsingItem`, `BucketItem.use`, `BlockTransformer.transformBlock` and `useWithoutItem`.

use super::java_routing::{
    ActingLivingArg, HandArg, HitArg, LevelArg, Recv, UseOnArg, UseResult, route,
};
use super::*;
use crate::game::player_use::EmptyHand;
use horizon_client::items::ItemInfo;
use horizon_client::first_person::UseResult as HandResult;
use std::cell::{Cell, RefCell};

/// A click's native tail, held while Java's merged `ItemStack.use` may reach for it through the `Item.use` bridge.
struct Tail {
    stack: ItemStack,
    info: ItemInfo,
    block_hit: Option<Hit>,
    sneaking: bool,
    hands_full: bool,
}

thread_local! {
    static TAIL: RefCell<Option<Tail>> = const { RefCell::new(None) };
    static TAIL_RAN: Cell<bool> = const { Cell::new(false) };
    static FINISH: RefCell<Option<(usize, ItemStack, ItemInfo)>> = const { RefCell::new(None) };
    static FINISH_RAN: Cell<bool> = const { Cell::new(false) };
    static EMPTY_RAN: Cell<Option<EmptyHand>> = const { Cell::new(None) };
    /// An error a native body met inside a call from Java, for the caller to raise once the call is back.
    static ERROR: RefCell<Option<Box<dyn std::error::Error>>> = const { RefCell::new(None) };
}

/// The value of a native body that may fail inside a call from Java: the error waits for [`raised`].
fn keep<T>(outcome: Result<T>) -> Option<T> {
    match outcome {
        Ok(value) => Some(value),
        Err(error) => {
            ERROR.with_borrow_mut(|slot| *slot = Some(error));
            None
        }
    }
}

/// The error a native body left during the call that just ended.
fn raised() -> Result<()> {
    ERROR.with_borrow_mut(Option::take).map_or(Ok(()), Err)
}

fn use_result(result: HandResult) -> UseResult {
    match result {
        HandResult::Success { swing } => UseResult::Success(swing),
        HandResult::Fail => UseResult::Fail,
        HandResult::Pass => UseResult::Pass,
    }
}

pub(in crate::game) fn hand_result(result: UseResult) -> HandResult {
    match result {
        UseResult::Success(swing) => HandResult::Success { swing },
        UseResult::Fail => HandResult::Fail,
        UseResult::Pass | UseResult::TryEmptyHand => HandResult::Pass,
    }
}

/// `UseResult` as the int `Flat.toInteractionResult` reads: the kind, then the swing source's ordinal times 16.
pub(in crate::game) fn encode(result: UseResult) -> i32 {
    match result {
        UseResult::Success(swing) => i32::from(swing) << 4,
        UseResult::Fail => 1,
        UseResult::Pass => 2,
        UseResult::TryEmptyHand => 3,
    }
}

impl Game<'_> {
    /// The hand acting now as `InteractionHand`'s ordinal.
    fn hand_ordinal(&self) -> i32 {
        i32::from(matches!(self.hand, Hand::Off))
    }

    /// A click with the held item once mods had their say before it: `ItemStack.use` in Java's merged method when a mod patches it.
    pub(in crate::game) fn java_item_stack_use(
        &mut self,
        stack: ItemStack,
        info: ItemInfo,
        block_hit: Option<&Hit>,
        sneaking: bool,
        hands_full: bool,
    ) -> Result<bool> {
        // An item with code of its own ran its `use` in Java, which called the merged method.
        // A click on a block reaches `ItemStack.use` only after `useOn` passed, which the tail orders itself.
        let java_item =
            horizon_java_host::get().is_some_and(|h| h.item_code_runs(&stack.item, false));
        if block_hit.is_some() || java_item {
            return self.use_item_tail(&stack, &info, block_hit, false, sneaking, hands_full);
        }
        let held = stack.clone();
        TAIL.with_borrow_mut(|slot| {
            *slot = Some(Tail {
                stack,
                info,
                block_hit: None,
                sneaking,
                hands_full,
            });
        });
        TAIL_RAN.set(false);
        let routed = self.item_stack_use(&held);
        TAIL.with_borrow_mut(Option::take);
        raised()?;
        // The native tail swung for itself when it ran; a mod that answered without it swings here.
        let tail_ran = TAIL_RAN.replace(false);
        match routed {
            UseResult::Success(swing) => {
                if swing && !tail_ran {
                    self.swing_arm();
                }
                Ok(true)
            }
            UseResult::Fail => {
                self.use_failed = true;
                Ok(true)
            }
            UseResult::Pass | UseResult::TryEmptyHand => Ok(false),
        }
    }

    fn item_stack_use(&mut self, stack: &ItemStack) -> UseResult {
        route!(
            self,
            "net/minecraft/world/item/ItemStack#use(Lnet/minecraft/world/level/Level;Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/InteractionHand;)Lnet/minecraft/world/InteractionResult;",
            Recv::Stack(self.hand_ordinal()),
            (
                LevelArg(self.dim.id()),
                self.java_player_arg(),
                HandArg(self.hand_ordinal())
            ),
            item = &*stack.item
        );
        self.item_use_body()
    }

    /// `Item.use` of the held item on its native path: what Java's merged `ItemStack.use` reaches for a vanilla item.
    pub(in crate::game) fn item_use_body(&mut self) -> UseResult {
        let Some(tail) = TAIL.with_borrow_mut(Option::take) else {
            return UseResult::Pass;
        };
        TAIL_RAN.set(true);
        match keep(self.use_item_tail(
            &tail.stack,
            &tail.info,
            tail.block_hit.as_ref(),
            false,
            tail.sneaking,
            tail.hands_full,
        )) {
            Some(true) if self.use_failed => UseResult::Fail,
            Some(true) => UseResult::Success(false),
            Some(false) => UseResult::Pass,
            None => UseResult::Fail,
        }
    }

    /// `ItemStack.finishUsingItem` of the stack in use: in Java's merged method when a mod patches it, else native's completion.
    pub(in crate::game) fn java_finish_using(
        &mut self,
        slot: usize,
        stack: &ItemStack,
        info: &ItemInfo,
    ) {
        FINISH.with_borrow_mut(|pending| *pending = Some((slot, stack.clone(), info.clone())));
        FINISH_RAN.set(false);
        let result = self.item_stack_finish_using(stack);
        FINISH.with_borrow_mut(Option::take);
        if !FINISH_RAN.replace(false) {
            // A mod answered without the finish: the use still ends.
            self.using = None;
            self.use_vibration(stack, "minecraft:item_interact_finish");
        }
        // `LivingEntity.completeUsingItem` puts the result in the hand.
        if result.as_ref() != self.held() {
            *self.held_mut() = result.filter(|s| s.count > 0);
        }
    }

    fn item_stack_finish_using(&mut self, stack: &ItemStack) -> Option<ItemStack> {
        route!(
            self,
            "net/minecraft/world/item/ItemStack#finishUsingItem(Lnet/minecraft/world/level/Level;Lnet/minecraft/world/entity/LivingEntity;)Lnet/minecraft/world/item/ItemStack;",
            Recv::Stack(self.hand_ordinal()),
            (
                LevelArg(self.dim.id()),
                ActingLivingArg(self.java_player_arg())
            ),
            item = &*stack.item
        );
        self.finish_using_body()
    }

    /// `ItemStack.finishUsingItem`'s native body: the use completes (a consumable is eaten, a spyglass lowered); the hand's stack after.
    pub(in crate::game) fn finish_using_body(&mut self) -> Option<ItemStack> {
        if let Some((slot, stack, info)) = FINISH.with_borrow_mut(Option::take) {
            FINISH_RAN.set(true);
            self.complete_using_item_native(slot, &stack, &info);
        }
        self.held().cloned()
    }

    /// `BucketItem.use` of the held bucket: in Java's merged method when a mod patches it, else native's.
    pub(in crate::game) fn java_bucket_use(&mut self, stack: &ItemStack) -> Result<HandResult> {
        let routed = self.bucket_use(stack);
        raised()?;
        Ok(hand_result(routed))
    }

    fn bucket_use(&mut self, stack: &ItemStack) -> UseResult {
        route!(
            self,
            "net/minecraft/world/item/BucketItem#use(Lnet/minecraft/world/level/Level;Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/InteractionHand;)Lnet/minecraft/world/InteractionResult;",
            Recv::Stack(self.hand_ordinal()),
            (
                LevelArg(self.dim.id()),
                self.java_player_arg(),
                HandArg(self.hand_ordinal())
            ),
            item = &*stack.item
        );
        self.bucket_use_body(stack)
    }

    /// `BucketItem.use`'s native body.
    pub(in crate::game) fn bucket_use_body(&mut self, stack: &ItemStack) -> UseResult {
        keep(self.use_bucket_native(stack)).map_or(UseResult::Fail, use_result)
    }

    /// `BlockTransformer.transformBlock` of the held axe, hoe or shovel: in Java's merged method when a mod patches it, else native's.
    pub(in crate::game) fn java_block_transform(
        &mut self,
        stack: &ItemStack,
        hit: &Hit,
    ) -> Result<HandResult> {
        let transformer = self
            .items()
            .get(&stack.item)
            .is_some_and(|info| info.block_transformer.is_some());
        if !transformer {
            return self.use_block_transformer_result(stack, hit);
        }
        let routed = self.block_transformer(stack, hit);
        raised()?;
        Ok(hand_result(routed))
    }

    fn block_transformer(&mut self, stack: &ItemStack, hit: &Hit) -> UseResult {
        route!(
            self,
            "net/minecraft/core/component/BlockTransformer#transformBlock(Lnet/minecraft/world/item/context/UseOnContext;)Lnet/minecraft/world/InteractionResult;",
            Recv::Transformer(self.hand_ordinal()),
            (UseOnArg {
                player: self.java_player_arg(),
                hand: self.hand_ordinal(),
                hit: HitArg::of(hit)
            })
        );
        self.block_transform_body(stack, hit)
    }

    /// `BlockTransformer.transformBlock`'s native body.
    pub(in crate::game) fn block_transform_body(
        &mut self,
        stack: &ItemStack,
        hit: &Hit,
    ) -> UseResult {
        keep(self.use_block_transformer_result(stack, hit)).map_or(UseResult::Fail, use_result)
    }

    /// `BlockStateBase.useWithoutItem` of the clicked block: in Java's merged method when a mod patches it for that state, else native's.
    pub(in crate::game) fn java_use_without_item(
        &mut self,
        state: BlockStateId,
        hit: &Hit,
        block_hit: horizon_core::geometry::BlockHit,
    ) -> Result<EmptyHand> {
        use horizon_world::level::InteractionResult;
        EMPTY_RAN.set(None);
        let routed = self.use_without_item_step(state, hit, block_hit);
        raised()?;
        let answer = match routed {
            UseResult::Success(true) => InteractionResult::Success,
            UseResult::Success(false) => InteractionResult::Consume,
            UseResult::Fail | UseResult::Pass | UseResult::TryEmptyHand => InteractionResult::Pass,
        };
        Ok(match EMPTY_RAN.replace(None) {
            Some(EmptyHand::Fence) => EmptyHand::Fence,
            Some(EmptyHand::Used(_, vanilla)) => EmptyHand::Used(answer, vanilla),
            None => EmptyHand::Used(answer, false),
        })
    }

    fn use_without_item_step(
        &mut self,
        state: BlockStateId,
        hit: &Hit,
        block_hit: horizon_core::geometry::BlockHit,
    ) -> UseResult {
        route!(
            self,
            "net/minecraft/world/level/block/state/BlockBehaviour$BlockStateBase#useWithoutItem(Lnet/minecraft/world/level/Level;Lnet/minecraft/world/entity/player/Player;Lnet/minecraft/world/phys/BlockHitResult;)Lnet/minecraft/world/InteractionResult;",
            Recv::Block(state),
            (
                LevelArg(self.dim.id()),
                self.java_player_arg(),
                HitArg::of(hit)
            ),
            block = state
        );
        self.use_without_item_body(state, hit, block_hit)
    }

    /// `BlockStateBase.useWithoutItem`'s native body.
    pub(in crate::game) fn use_without_item_body(
        &mut self,
        state: BlockStateId,
        hit: &Hit,
        block_hit: horizon_core::geometry::BlockHit,
    ) -> UseResult {
        use horizon_world::level::InteractionResult;
        let Some(done) = keep(self.use_without_item_native(state, hit, block_hit)) else {
            return UseResult::Fail;
        };
        EMPTY_RAN.set(Some(done));
        match done {
            EmptyHand::Used(InteractionResult::Success, _) => UseResult::Success(true),
            EmptyHand::Used(InteractionResult::Consume, _) | EmptyHand::Fence => {
                UseResult::Success(false)
            }
            EmptyHand::Used(InteractionResult::Pass, _) => UseResult::Pass,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::java_routing::{HitArg, RouteArg};

    #[test]
    fn a_use_result_encodes_as_the_ints_java_decodes() {
        // Flat.toInteractionResult: the low nibble is the kind, the rest the swing source (1 predicted).
        assert_eq!(encode(UseResult::Success(false)), 0);
        assert_eq!(encode(UseResult::Success(true)), 16);
        assert_eq!(encode(UseResult::Fail), 1);
        assert_eq!(encode(UseResult::Pass), 2);
        assert_eq!(encode(UseResult::TryEmptyHand), 3);
    }

    #[test]
    fn a_hit_and_a_hand_write_as_java_reads_them() {
        // The same bytes `RoutingArgsCheck.uses` reads: x y z, the face's 3DDataValue, inside, the block's x y z, then the hand.
        let hit = Hit {
            pos: [3, -64, 7],
            face: [0, 1, 0],
            point: [3.25, -63.5, 7.75],
            inside: true,
        };
        let mut out = horizon_java_host::wire::Out::default();
        RouteArg::put(&HitArg::of(&hit), &mut out);
        RouteArg::put(&crate::game::java_routing::HandArg(1), &mut out);
        let hex: String = out.0.iter().map(|b| format!("{b:02x}")).collect();
        let expected = [
            "400a000000000000",
            "c04fc00000000000",
            "401f000000000000",
            "3ff0000000000000",
            "3ff0000000000000",
            "4008000000000000",
            "c050000000000000",
            "401c000000000000",
            "00000001",
        ]
        .concat();
        assert_eq!(hex, expected);
    }

    #[test]
    fn a_hit_survives_the_wire_for_every_face() {
        // Direction.get3DDataValue: down, up, north, south, west, east.
        let faces = [[0, -1, 0], [0, 1, 0], [0, 0, -1], [0, 0, 1], [-1, 0, 0], [1, 0, 0]];
        for (value, face) in faces.into_iter().enumerate() {
            let hit = Hit {
                pos: [3, -64, 7],
                face,
                point: [3.25, -63.5, 7.75],
                inside: value % 2 == 0,
            };
            let wire = HitArg::of(&hit).0;
            assert_eq!(wire[3], value as f64);
            let back = HitArg::read(&wire).expect("eight numbers");
            assert_eq!(
                (back.pos, back.face, back.point, back.inside),
                (hit.pos, hit.face, hit.point, hit.inside)
            );
        }
    }
}
