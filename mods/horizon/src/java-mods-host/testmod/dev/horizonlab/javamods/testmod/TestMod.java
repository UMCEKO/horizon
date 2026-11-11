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
    public static final DeferredRegister<net.minecraft.core.particles.ParticleType<?>> PARTICLE_TYPES = DeferredRegister.create(net.minecraft.core.registries.Registries.PARTICLE_TYPE, "horizon_java_test");
    public static final java.util.function.Supplier<net.minecraft.core.particles.SimpleParticleType> SPARK = PARTICLE_TYPES.register("spark", () -> new net.minecraft.core.particles.SimpleParticleType(false));

    public TestMod(IEventBus bus) {
        ITEMS.registerItem("wand", Wand::new);
        ITEMS.registerItem("probe", Probe::new);
        ITEMS.registerItem("tonic", Tonic::new);
        ITEMS.registerItem("labelled", Labelled::new);
        ITEMS.registerItem("digger", Digger::new);
        ITEMS.registerItem("miner", Miner::new);
        net.neoforged.neoforge.common.NeoForge.EVENT_BUS.addListener((net.neoforged.neoforge.event.entity.player.PlayerEvent.HarvestCheck event) -> {
            if (event.getTargetBlock().is(Blocks.STONE) && event.getEntity().getMainHandItem().getItem() instanceof Digger) event.setCanHarvest(true);
        });
        net.neoforged.neoforge.common.NeoForge.EVENT_BUS.addListener((net.neoforged.neoforge.event.level.block.BreakBlockEvent event) -> {
            if (event.getState().is(Blocks.GILDED_BLACKSTONE)) event.setCanceled(true);
            else if (event.getState().is(Blocks.IRON_BLOCK) && event.getLevel() instanceof Level level) level.destroyBlock(event.getPos().above(), false);
        });
        net.neoforged.neoforge.common.NeoForge.EVENT_BUS.addListener((net.neoforged.neoforge.event.entity.player.PlayerEvent.BreakSpeed event) -> {
            if (event.getState().is(Blocks.GRAVEL)) event.setNewSpeed(event.getNewSpeed() * 0.5f);
        });
        // A plain food, so the boot's facts carry a food item.
        ITEMS.registerSimpleItem("snack", p -> p.food(new net.minecraft.world.food.FoodProperties.Builder().nutrition(5).saturationModifier(0.7f).build()).stacksTo(16));
        ITEMS.register(bus);
        Battery.register(bus, ITEMS);
        Goo.register(bus, ITEMS);
        Breakables.register(bus, ITEMS);
        Switcher.register(bus, ITEMS);
        ChunkData.register(bus);
        PARTICLE_TYPES.register(bus);
        if (net.neoforged.fml.loading.FMLEnvironment.getDist().isClient()) {
            ClientKeys.register(bus);
            ClientTextures.register(bus);
            ClientWorldBox.register();
            ClientProfiles.register();
        }
    }

    /** The second look, after the world has run: what a machine's ticks did. */
    public static final class Probe extends Item {
        public Probe(Properties properties) {
            super(properties);
        }

        @Override
        public InteractionResult useOn(UseOnContext context) {
            if (!context.getLevel().isClientSide()) ChecksBlockEntities.later(context);
            return InteractionResult.SUCCESS;
        }
    }

    /** Counts the blocks broken with it, as a tool spending its energy in `mineBlock` does. */
    public static final class Miner extends Item {
        public static int mined;

        public Miner(Properties properties) {
            super(properties);
        }

        @Override
        public boolean mineBlock(net.minecraft.world.item.ItemStack stack, Level level, net.minecraft.world.level.block.state.BlockState state, BlockPos pos, net.minecraft.world.entity.LivingEntity owner) {
            mined++;
            return true;
        }
    }

    /** Mines everything at 100, as a tool whose speed its code computes does. */
    public static final class Digger extends Item {
        public Digger(Properties properties) {
            super(properties);
        }

        @Override
        public float getDestroySpeed(net.minecraft.world.item.ItemStack stack, net.minecraft.world.level.block.state.BlockState state) {
            return 100.0f;
        }
    }

    /** A name and a tooltip line only its code computes, as Mekanism's upgrades have. */
    public static final class Labelled extends Item {
        public Labelled(Properties properties) {
            super(properties);
        }

        @Override
        public Component getName(net.minecraft.world.item.ItemStack stack) {
            return Component.literal("Labelled by code");
        }

        @Override
        public void appendHoverText(net.minecraft.world.item.ItemStack stack, TooltipContext context, net.minecraft.world.item.component.TooltipDisplay display, java.util.function.Consumer<Component> lines, net.minecraft.world.item.TooltipFlag flag) {
            lines.accept(Component.literal("Tooltip line from code"));
        }
    }

    /** In the air: gives the player speed from mod code, so the effect's write-through shows in the player's state. */
    public static final class Tonic extends Item {
        public Tonic(Properties properties) {
            super(properties);
        }

        @Override
        public InteractionResult use(Level level, net.minecraft.world.entity.player.Player player, net.minecraft.world.InteractionHand hand) {
            player.addEffect(new net.minecraft.world.effect.MobEffectInstance(net.minecraft.world.effect.MobEffects.SPEED, 600, 1));
            return InteractionResult.SUCCESS;
        }
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
            if (!level.isClientSide()) Checks.runAll(context);
            return InteractionResult.SUCCESS;
        }
    }
}
