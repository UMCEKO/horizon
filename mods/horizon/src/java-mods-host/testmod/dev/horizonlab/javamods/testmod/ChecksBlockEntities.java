package dev.horizonlab.javamods.testmod;

import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.Container;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.ChestBlockEntity;
import net.neoforged.neoforge.capabilities.BlockCapabilityCache;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.transfer.ResourceHandler;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.neoforge.transfer.energy.EnergyHandler;
import net.neoforged.neoforge.transfer.transaction.Transaction;

final class ChecksBlockEntities {
    private ChecksBlockEntities() {}

    private static BlockPos area;
    private static ChestBlockEntity chest;
    private static ChestBlockEntity replacedChest;
    private static ChestBlockEntity mergedChest;
    private static ChestBlockEntity removedChest;
    private static BlockCapabilityCache<ResourceHandler<ItemResource>, Direction> cache;
    private static BlockCapabilityCache<ResourceHandler<ItemResource>, Direction> placedCache;
    private static BlockCapabilityCache<ResourceHandler<ItemResource>, Direction> brokenCache;

    private static int cobblestone(ResourceHandler<ItemResource> handler) {
        ItemResource cobblestone = ItemResource.of(Items.COBBLESTONE);
        int total = 0;
        for (int slot = 0; slot < handler.size(); slot++) {
            if (handler.getResource(slot).equals(cobblestone)) total += handler.getAmountAsInt(slot);
        }
        return total;
    }

    private static void move(ResourceHandler<ItemResource> handler, boolean insert, int amount, boolean commit) {
        ItemResource cobblestone = ItemResource.of(Items.COBBLESTONE);
        try (var transaction = Transaction.openRoot()) {
            if (insert) handler.insert(cobblestone, amount, transaction);
            else handler.extract(cobblestone, amount, transaction);
            if (commit) transaction.commit();
        }
    }

    /** The probe clicks a marker two blocks south of where the wand clicked. */
    private static BlockPos areaOf(UseOnContext context) {
        return context.getClickedPos().offset(8 + 6 * 4, 0, -2);
    }

    static void run(UseOnContext context, BlockPos at) {
        Level level = context.getLevel();
        area = at;
        level.setBlock(at.above(5), Blocks.DIRT.defaultBlockState(), 3);
        level.setBlock(at, Battery.BLOCK.get().defaultBlockState(), 3);
        level.setBlock(at.east(), Battery.BLOCK.get().defaultBlockState(), 3);
        Checks.check("be_created", () -> level.getBlockEntity(at) instanceof Battery.Entity);
        Checks.check("be_same_object", () -> level.getBlockEntity(at) == level.getBlockEntity(at));
        Checks.check("be_none_on_stone", () -> level.getBlockEntity(at.below(2)) == null || !(level.getBlockEntity(at.below(2)) instanceof Battery.Entity));
        Checks.check("energy_cap", () -> {
            EnergyHandler handler = level.getCapability(Capabilities.Energy.BLOCK, at, Direction.UP);
            if (handler == null) return false;
            try (var transaction = Transaction.openRoot()) {
                handler.insert(100000, transaction);
                transaction.commit();
            }
            return handler.getAmountAsInt() == 100000;
        });
        Checks.check("energy_rollback", () -> {
            EnergyHandler handler = level.getCapability(Capabilities.Energy.BLOCK, at, Direction.UP);
            try (var transaction = Transaction.openRoot()) {
                handler.insert(5, transaction);
            }
            return handler.getAmountAsInt() == 100000;
        });
        level.setBlock(at.above(2), Blocks.CHEST.defaultBlockState(), 3);
        Checks.check("vanilla_be_copy", () -> {
            if (!(level.getBlockEntity(at.above(2)) instanceof ChestBlockEntity chest)) return false;
            ((Container) chest).setItem(0, new ItemStack(Items.DIAMOND, 3));
            chest.setChanged();
            return true;
        });
        for (int up : new int[] {6, 7, 9}) {
            level.setBlock(at.above(up), Blocks.CHEST.defaultBlockState(), 3);
            Checks.check("chest_made_" + up, () -> {
                if (!(level.getBlockEntity(at.above(up)) instanceof ChestBlockEntity found)) return false;
                found.setItem(0, new ItemStack(Items.DIAMOND, 1));
                found.setChanged();
                switch (up) {
                    case 6 -> replacedChest = found;
                    case 7 -> mergedChest = found;
                    default -> removedChest = found;
                }
                return true;
            });
        }
        BlockPos chestAt = at.above(4);
        level.setBlock(chestAt, Blocks.CHEST.defaultBlockState(), 3);
        Checks.check("chest_cache_made", () -> {
            if (!(level.getBlockEntity(chestAt) instanceof ChestBlockEntity found)) return false;
            chest = found;
            found.setItem(0, new ItemStack(Items.COBBLESTONE, 10));
            found.setChanged();
            cache = BlockCapabilityCache.create(Capabilities.Item.BLOCK, (ServerLevel) level, chestAt, Direction.UP);
            return cache.getCapability() != null && cobblestone(cache.getCapability()) == 10;
        });
        Checks.check("chest_simulated_extract", () -> {
            move(cache.getCapability(), false, 4, false);
            return cobblestone(cache.getCapability()) == 10 && chest.getItem(0).getCount() == 10;
        });
        Checks.check("chest_committed_extract", () -> {
            move(cache.getCapability(), false, 4, true);
            return cobblestone(cache.getCapability()) == 6 && chest.getItem(0).getCount() == 6;
        });
        level.setBlock(at.above(10), Blocks.CHEST.defaultBlockState(), 3);
        placedCache = BlockCapabilityCache.create(Capabilities.Item.BLOCK, (ServerLevel) level, at.above(8), Direction.UP);
        brokenCache = BlockCapabilityCache.create(Capabilities.Item.BLOCK, (ServerLevel) level, at.above(10), Direction.UP);
        Checks.check("native_chest_caches_made", () -> placedCache.getCapability() == null && brokenCache.getCapability() != null);
        level.setBlock(at.below(), Blocks.DIRT.defaultBlockState(), 3);
        Checks.check("server_view", () -> level.getServer() != null && level.getServer().overworld() != null && level.getServer().getRecipeManager() != null);
        Checks.check("connection_view", () -> {
            var player = (net.minecraft.server.level.ServerPlayer) context.getPlayer();
            player.connection.send(new net.minecraft.network.protocol.game.ClientboundSetTimePacket(0L, java.util.Map.of()));
            return true;
        });
    }

    /** The chest the first session filled, changed natively in between: the object, its cached handler and the saved form agree. */
    private static void chestLater(Level level, BlockPos chestAt, boolean reloaded) {
        if (!reloaded) {
            Checks.check("chest_same_object", () -> level.getBlockEntity(chestAt) == chest);
            Checks.check("chest_cache_sees_native_change", () -> cobblestone(cache.getCapability()) == 20 && chest.getItem(0).getCount() == 20);
            Checks.check("chest_committed_extract_after_native", () -> {
                move(cache.getCapability(), false, 5, true);
                return cobblestone(cache.getCapability()) == 15 && chest.getItem(0).getCount() == 15;
            });
            Checks.check("chest_simulated_extract_after_native", () -> {
                move(cache.getCapability(), false, 3, false);
                return cobblestone(cache.getCapability()) == 15 && chest.getItem(0).getCount() == 15;
            });
            Checks.check("chest_committed_insert", () -> {
                move(cache.getCapability(), true, 3, true);
                return cobblestone(cache.getCapability()) == 18 && chest.getItem(0).getCount() == 18;
            });
            Checks.check("be_replaced_natively_is_new_object", () -> level.getBlockEntity(chestAt.above(2)) instanceof ChestBlockEntity fresh && fresh != replacedChest && fresh.getItem(0).isEmpty());
            Checks.check("be_data_merge_keeps_object", () -> level.getBlockEntity(chestAt.above(3)) == mergedChest && mergedChest.getItem(0).is(Items.DIAMOND) && mergedChest.getItem(0).getCount() == 5);
            Checks.check("be_removed_natively_is_dropped", () -> level.getBlockEntity(chestAt.above(5)) == null && removedChest.isRemoved());
            Checks.check("cache_sees_chest_placed_natively", () -> placedCache.getCapability() != null);
            Checks.check("cache_sees_chest_broken_natively", () -> brokenCache.getCapability() == null);
            return;
        }
        Checks.check("chest_persisted", () -> level.getBlockEntity(chestAt) instanceof ChestBlockEntity found && found.getItem(0).is(Items.COBBLESTONE) && found.getItem(0).getCount() == 18);
        var before = level.getBlockEntity(chestAt);
        cache = BlockCapabilityCache.create(Capabilities.Item.BLOCK, (ServerLevel) level, chestAt, Direction.UP);
        Checks.check("chest_cache_reads_saved", () -> cobblestone(cache.getCapability()) == 18);
        level.setBlock(chestAt, Blocks.AIR.defaultBlockState(), 3);
        level.setBlock(chestAt, Blocks.CHEST.defaultBlockState(), 3);
        Checks.check("chest_cache_sees_new_chest", () -> {
            if (!(level.getBlockEntity(chestAt) instanceof ChestBlockEntity fresh) || fresh == before) return false;
            fresh.setItem(0, new ItemStack(Items.COBBLESTONE, 7));
            fresh.setChanged();
            return cobblestone(cache.getCapability()) == 7;
        });
    }

    /** After the world has run: the same machine, saved and loaded if this JVM didn't make it. */
    static void later(UseOnContext context) {
        Level level = context.getLevel();
        BlockPos at = areaOf(context);
        boolean reloaded = area == null;
        ChecksChunks.later(reloaded);
        BlockPos entities = context.getClickedPos().offset(8 + 5 * 4, 0, -2);
        if (reloaded) ChecksEntities.afterChange(context, entities);
        chestLater(level, at.above(4), reloaded);
        ChecksEntities.later(context, entities);
        Checks.check("be_reloaded_or_held", () -> level.getBlockEntity(at) instanceof Battery.Entity);
        if (!(level.getBlockEntity(at) instanceof Battery.Entity first) || !(level.getBlockEntity(at.east()) instanceof Battery.Entity second)) return;
        Checks.check("be_ticked", () -> first.ticks >= 20);
        Checks.check("be_pushed_energy", () -> second.energy.getAmountAsInt() >= 100 && second.energy.getAmountAsInt() + first.energy.getAmountAsInt() == 100000);
        Checks.check("block_scheduled_tick", () -> level.getBlockState(at.south()).is(Blocks.EMERALD_BLOCK));
        Checks.check("block_neighbor_changed", () -> level.getBlockState(at.north()).is(Blocks.LAPIS_BLOCK));
        Checks.check("chest_written_back", () -> level.getBlockEntity(at.above(2)) instanceof ChestBlockEntity chest && chest.getItem(0).is(Items.DIAMOND) && chest.getItem(0).getCount() == 3);
        if (reloaded) Checks.check("be_persisted", () -> second.energy.getAmountAsInt() > 0 && first.ticks > 0);
    }
}
