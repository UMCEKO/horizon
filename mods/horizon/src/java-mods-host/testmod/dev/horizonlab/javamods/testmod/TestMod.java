package dev.horizonlab.javamods.testmod;

import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.network.chat.Component;
import net.minecraft.stats.Stats;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.gameevent.GameEvent;
import net.minecraft.world.level.levelgen.Heightmap;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.common.Mod;
import net.neoforged.neoforge.registries.DeferredRegister;

/** Exercises the world bridges from real mod code, one observable block per bridge, for `java_mods.py bridges`. */
@Mod("horizon_java_test")
public final class TestMod {
    public static final DeferredRegister.Items ITEMS = DeferredRegister.createItems("horizon_java_test");

    public TestMod(IEventBus bus) {
        ITEMS.registerItem("wand", Wand::new);
        ITEMS.register(bus);
    }

    /** On a clicked block: builds a column above it whose blocks say what each call answered. */
    public static final class Wand extends Item {
        public Wand(Properties properties) {
            super(properties);
        }

        @Override
        public InteractionResult useOn(UseOnContext context) {
            Level level = context.getLevel();
            BlockPos base = context.getClickedPos();
            BlockPos at = base.above();
            boolean read = level.getBlockState(base).is(Blocks.STONE);
            level.setBlock(at, read ? Blocks.GOLD_BLOCK.defaultBlockState() : Blocks.COAL_BLOCK.defaultBlockState(), 3);
            boolean written = level.getBlockState(at).is(Blocks.GOLD_BLOCK);
            level.setBlock(at.above(), written ? Blocks.REDSTONE_BLOCK.defaultBlockState() : Blocks.COAL_BLOCK.defaultBlockState(), 3);
            boolean signal = level.hasNeighborSignal(at) && level.getBestNeighborSignal(at) == 15 && level.getSignal(at.above(), Direction.DOWN) == 15;
            level.setBlock(at.above(2), signal ? Blocks.EMERALD_BLOCK.defaultBlockState() : Blocks.COAL_BLOCK.defaultBlockState(), 3);
            level.setBlock(at.north(), Blocks.DIRT.defaultBlockState(), 3);
            boolean destroyed = level.destroyBlock(at.north(), false) && level.getBlockState(at.north()).isAir();
            level.setBlock(at.above(3), destroyed ? Blocks.DIAMOND_BLOCK.defaultBlockState() : Blocks.COAL_BLOCK.defaultBlockState(), 3);
            level.setBlock(at.south(), Blocks.DIRT.defaultBlockState(), 3);
            boolean removed = level.removeBlock(at.south(), false) && level.getBlockState(at.south()).isAir();
            level.setBlock(at.above(4), removed ? Blocks.LAPIS_BLOCK.defaultBlockState() : Blocks.COAL_BLOCK.defaultBlockState(), 3);
            level.setBlock(at.above(8), Blocks.SAND.defaultBlockState(), 2);
            level.scheduleTick(at.above(8), Blocks.SAND, 2);
            boolean fluid = level.getFluidState(at).isEmpty();
            level.setBlock(at.above(5), fluid ? Blocks.IRON_BLOCK.defaultBlockState() : Blocks.COAL_BLOCK.defaultBlockState(), 3);
            boolean loaded = level.isLoaded(at) && !level.isLoaded(at.offset(100000, 0, 0));
            level.setBlock(at.above(6), loaded ? Blocks.QUARTZ_BLOCK.defaultBlockState() : Blocks.COAL_BLOCK.defaultBlockState(), 3);
            boolean height = level.getHeight(Heightmap.Types.WORLD_SURFACE, at.getX(), at.getZ()) > at.above(6).getY();
            level.setBlock(at.east(), height ? Blocks.OBSIDIAN.defaultBlockState() : Blocks.COAL_BLOCK.defaultBlockState(), 3);
            boolean light = level.getMaxLocalRawBrightness(at.above(9)) > 0;
            level.setBlock(at.west(), light ? Blocks.GLOWSTONE.defaultBlockState() : Blocks.COAL_BLOCK.defaultBlockState(), 3);
            level.gameEvent(context.getPlayer(), GameEvent.BLOCK_PLACE, at);
            context.getPlayer().awardStat(Stats.ITEM_USED.get(this));
            context.getPlayer().sendSystemMessage(Component.literal("wand ok"));
            Checks.runAll(context);
            return InteractionResult.SUCCESS;
        }
    }
}
