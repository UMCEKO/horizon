// game-module-cfg: feature java-mods
//! A Java mod's block entity's item handler (`Capabilities.Item.BLOCK`) as the game's `ITEMS` part: hoppers move through it in their own transaction, which a Java one follows.

use super::Game;
use super::api_values::{TryIntoApi, TryIntoEngine};
use super::java_mods::{java_read_effects, read_tag, write_tag};
use horizon_core::position::BlockPos;
use horizon_java_host::wire::{In, Out};
use mod_api::{BlockSide, Capability, ItemResource, ResourceHandler, ServerHost, Transaction};

/// What an `item_handler` call asks, as `ItemHandlers.call` reads it.
#[derive(Clone, Copy)]
enum Ask {
    Size = 0,
    Slot = 1,
    Capacity = 2,
    Valid = 3,
    Insert = 4,
    Extract = 5,
}

/// A write with no index: the handler's own `insert(resource, amount, tx)` or `extract`.
const ANY_INDEX: i32 = -1;

/// The handler Java's `Level.getCapability(Capabilities.Item.BLOCK, pos, side)` gives for `.0`, asked anew on each call.
struct JavaItems(BlockSide);

impl JavaItems {
    fn ask<R>(
        &self,
        host: &mut dyn ServerHost,
        f: impl FnOnce(&mut Game<'static>, BlockSide) -> Option<R>,
    ) -> Option<R> {
        super::java_listener::with_game(host, |game| f(game, self.0)).flatten()
    }

    fn write(
        &self,
        host: &mut dyn ServerHost,
        tx: &Transaction,
        ask: Ask,
        index: Option<usize>,
        resource: &ItemResource,
        amount: u32,
    ) -> u32 {
        self.ask(host, |game, at| {
            game.java_items_write(at, tx, ask, index, resource, amount)
        })
        .unwrap_or(0)
    }
}

impl ResourceHandler<ItemResource> for JavaItems {
    fn size(&self, host: &mut dyn ServerHost) -> usize {
        self.ask(host, |game, at| game.java_items_size(at))
            .unwrap_or(0)
    }
    fn resource(&self, host: &mut dyn ServerHost, index: usize) -> Option<ItemResource> {
        let stack = self.ask(host, |game, at| game.java_items_slot(at, index)?.0)?;
        Some(ItemResource::of(&stack.try_into_api().ok()?))
    }
    fn amount(&self, host: &mut dyn ServerHost, index: usize) -> u32 {
        self.ask(host, |game, at| Some(game.java_items_slot(at, index)?.1))
            .unwrap_or(0)
    }
    fn capacity(&self, host: &mut dyn ServerHost, index: usize, resource: &ItemResource) -> u32 {
        self.ask(host, |game, at| {
            game.java_items_query(at, Ask::Capacity, index, resource)
        })
        .unwrap_or(0)
    }
    fn is_valid(&self, host: &mut dyn ServerHost, index: usize, resource: &ItemResource) -> bool {
        self.ask(host, |game, at| {
            game.java_items_query(at, Ask::Valid, index, resource)
        })
        .is_some_and(|valid| valid != 0)
    }
    fn insert(
        &self,
        host: &mut dyn ServerHost,
        tx: &Transaction,
        index: usize,
        resource: &ItemResource,
        amount: u32,
    ) -> u32 {
        self.write(host, tx, Ask::Insert, Some(index), resource, amount)
    }
    fn extract(
        &self,
        host: &mut dyn ServerHost,
        tx: &Transaction,
        index: usize,
        resource: &ItemResource,
        amount: u32,
    ) -> u32 {
        self.write(host, tx, Ask::Extract, Some(index), resource, amount)
    }
    fn insert_any(
        &self,
        host: &mut dyn ServerHost,
        tx: &Transaction,
        resource: &ItemResource,
        amount: u32,
    ) -> u32 {
        self.write(host, tx, Ask::Insert, None, resource, amount)
    }
    fn extract_any(
        &self,
        host: &mut dyn ServerHost,
        tx: &Transaction,
        resource: &ItemResource,
        amount: u32,
    ) -> u32 {
        self.write(host, tx, Ask::Extract, None, resource, amount)
    }
}

impl Game<'_> {
    /// `ITEMS` of a Java mod's block entity at `at`, when its code gives a handler for that side.
    pub(super) fn java_item_part(
        &mut self,
        at: BlockSide,
    ) -> Option<Capability<dyn ResourceHandler<ItemResource>>> {
        let [x, y, z] = at.pos;
        if !horizon_java_host::bridge::java_block(self.world.block(x, y, z))
            || !self.is_java_block_entity(at.pos)
        {
            return None;
        }
        self.java_items_size(at)?;
        Some(mod_api::__private::game::layer_capability(
            Box::new(JavaItems(at)),
            super::java_listener::ID,
        ))
    }

    /// How many indices the handler at `at` has; None when there is none.
    fn java_items_size(&mut self, at: BlockSide) -> Option<usize> {
        self.java_items_call(at, Ask::Size, |_| {}, |reply| usize::try_from(reply.int()?).ok())
    }

    /// What index `index` holds, and how much of it.
    fn java_items_slot(
        &mut self,
        at: BlockSide,
        index: usize,
    ) -> Option<(Option<horizon_client::items::ItemStack>, u32)> {
        let index = i32::try_from(index).ok()?;
        self.java_items_call(
            at,
            Ask::Slot,
            |out| out.int(index),
            |reply| {
                let stack = horizon_client::save::item_from_nbt(&read_tag(reply.bytes()?)?);
                Some((stack, clamp(reply.long()?)))
            },
        )
    }

    /// `getCapacityAsLong` or `isValid` (1 or 0) of `resource` at `index`.
    fn java_items_query(
        &mut self,
        at: BlockSide,
        ask: Ask,
        index: usize,
        resource: &ItemResource,
    ) -> Option<u32> {
        let index = i32::try_from(index).ok()?;
        let stack = self.java_resource_bytes(resource)?;
        self.java_items_call(
            at,
            ask,
            |out| {
                out.int(index);
                out.bytes(&stack);
            },
            |reply| Some(clamp(reply.long()?)),
        )
    }

    /// An insert or extract of up to `amount` in the Java transaction that follows `tx`: how much moved.
    fn java_items_write(
        &mut self,
        at: BlockSide,
        tx: &Transaction,
        ask: Ask,
        index: Option<usize>,
        resource: &ItemResource,
        amount: u32,
    ) -> Option<u32> {
        if amount == 0 {
            return Some(0);
        }
        let index = index.map_or(Some(ANY_INDEX), |i| i32::try_from(i).ok())?;
        let stack = self.java_resource_bytes(resource)?;
        let depth = i32::try_from(self.java_transaction_depth(tx)?).ok()?;
        let amount = i32::try_from(amount).unwrap_or(i32::MAX);
        self.java_items_call(
            at,
            ask,
            |out| {
                out.int(depth);
                out.int(index);
                out.bytes(&stack);
                out.int(amount);
            },
            |reply| u32::try_from(reply.int()?).ok(),
        )
    }

    /// Horizon closed the transaction at `depth` that a Java one follows: Java's commits or aborts with it.
    pub(super) fn java_items_closed(&mut self, depth: usize, commit: bool) {
        let Ok(depth) = i32::try_from(depth) else {
            return;
        };
        self.java_level_call("item_handler_close", |out| {
            out.int(depth);
            out.bool(commit);
        });
    }

    /// One of `resource` in the saved form Java decodes.
    fn java_resource_bytes(&self, resource: &ItemResource) -> Option<Vec<u8>> {
        let mut stack = resource.as_stack().try_into_engine(self.items()).ok()?;
        stack.count = 1;
        write_tag(horizon_client::save::item_to_nbt(&stack))
    }

    /// One `item_handler` call about the handler at `at`; None when the block has none for that side, or the level isn't in place.
    fn java_items_call<R>(
        &mut self,
        at: BlockSide,
        ask: Ask,
        body: impl FnOnce(&mut Out),
        read: impl FnOnce(&mut In<'_>) -> Option<R>,
    ) -> Option<R> {
        if !self.level_in_place(at.level) {
            return None;
        }
        horizon_java_host::get()?;
        self.java_bind_states();
        let (key, time) = (self.java_level_key(), self.world.game_time);
        let pos = BlockPos::new(at.pos[0], at.pos[1], at.pos[2]).pack();
        let side = at.side.map_or(-1, |side| side as i32);
        let (answer, sounds) = self.java_call_with(
            "item_handler",
            |out| {
                out.int(key as i32);
                out.long(time);
                out.long(pos);
                out.int(side);
                out.int(ask as i32);
                body(out);
            },
            |reply| {
                let answer = if reply.bool()? { read(reply) } else { None };
                Some((answer, java_read_effects(reply).unwrap_or_default()))
            },
        )?;
        self.java_play_effects(sounds);
        answer
    }
}

/// A Java `long` amount as the API's `u32`.
fn clamp(amount: i64) -> u32 {
    u32::try_from(amount.max(0)).unwrap_or(u32::MAX)
}
