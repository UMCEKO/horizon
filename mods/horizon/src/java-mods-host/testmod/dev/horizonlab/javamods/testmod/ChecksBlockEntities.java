package dev.horizonlab.javamods.testmod;

import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.Container;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.ChestBlockEntity;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.transfer.energy.EnergyHandler;
import net.neoforged.neoforge.transfer.transaction.Transaction;

final class ChecksBlockEntities {
    private ChecksBlockEntities() {}

    private static BlockPos area;

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
        level.setBlock(at.below(), Blocks.DIRT.defaultBlockState(), 3);
        Checks.check("server_view", () -> level.getServer() != null && level.getServer().overworld() != null && level.getServer().getRecipeManager() != null);
        Checks.check("connection_view", () -> {
            var player = (net.minecraft.server.level.ServerPlayer) context.getPlayer();
            player.connection.send(new net.minecraft.network.protocol.game.ClientboundSetTimePacket(0L, java.util.Map.of()));
            return true;
        });
    }

    /** After the world has run: the same machine, saved and loaded if this JVM didn't make it. */
    static void later(UseOnContext context) {
        Level level = context.getLevel();
        BlockPos at = areaOf(context);
        boolean reloaded = area == null;
        BlockPos entities = context.getClickedPos().offset(8 + 5 * 4, 0, -2);
        if (reloaded) ChecksEntities.afterChange(context, entities);
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
