package dev.horizonlab.javamods.testmod;

import java.util.function.Supplier;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.Registries;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.BaseEntityBlock;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.BlockEntityTicker;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.StateDefinition;
import net.minecraft.world.level.block.state.properties.BooleanProperty;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.registries.DeferredRegister;
import org.jspecify.annotations.Nullable;

/** A block whose `getTicker` depends on its state: `alt` picks ticker A or B, `off` has none; it counts the asks and records each tick. */
final class Switcher extends BaseEntityBlock {
    private static final DeferredRegister.Blocks BLOCKS = DeferredRegister.createBlocks("horizon_java_test");
    private static final DeferredRegister<BlockEntityType<?>> TYPES = DeferredRegister.create(Registries.BLOCK_ENTITY_TYPE, "horizon_java_test");
    static final BooleanProperty ALT = BooleanProperty.create("alt");
    static final BooleanProperty OFF = BooleanProperty.create("off");
    static final Supplier<Block> BLOCK = BLOCKS.registerBlock("switcher", Switcher::new);
    static final Supplier<BlockEntityType<Entity>> TYPE = TYPES.register("switcher", () -> new BlockEntityType<>(Entity::new, BLOCK.get()));

    /** Server-side `getTicker` calls for this block. */
    static int asks;
    /** One letter per tick that ran: `A` or `B`, or `x` for a ticker that ran on a state it was not bound for. */
    static final StringBuilder trace = new StringBuilder();
    static long thirdA = -1;
    static long firstB = -1;

    static void register(IEventBus bus, DeferredRegister.Items items) {
        items.registerSimpleBlockItem("switcher", BLOCK);
        BLOCKS.register(bus);
        TYPES.register(bus);
    }

    Switcher(BlockBehaviour.Properties properties) {
        super(properties);
        registerDefaultState(stateDefinition.any().setValue(ALT, false).setValue(OFF, false));
    }

    @Override
    protected void createBlockStateDefinition(StateDefinition.Builder<Block, BlockState> builder) {
        builder.add(ALT, OFF);
    }

    @Override
    public BlockEntity newBlockEntity(BlockPos pos, BlockState state) {
        return new Entity(pos, state);
    }

    @Override
    public <T extends BlockEntity> @Nullable BlockEntityTicker<T> getTicker(Level level, BlockState state, BlockEntityType<T> type) {
        if (level instanceof ServerLevel) asks++;
        if (state.getValue(OFF)) return null;
        return state.getValue(ALT) ? createTickerHelper(type, TYPE.get(), Switcher::tickB) : createTickerHelper(type, TYPE.get(), Switcher::tickA);
    }

    private static void tickA(Level level, BlockPos pos, BlockState state, Entity entity) {
        if (!(level instanceof ServerLevel)) return;
        trace.append(state.getValue(ALT) || state.getValue(OFF) ? 'x' : 'A');
        if (trace.chars().filter(c -> c == 'A').count() == 3) {
            thirdA = level.getGameTime();
            level.setBlock(pos, state.setValue(ALT, true), 3);
        }
    }

    private static void tickB(Level level, BlockPos pos, BlockState state, Entity entity) {
        if (!(level instanceof ServerLevel)) return;
        trace.append(!state.getValue(ALT) || state.getValue(OFF) ? 'x' : 'B');
        if (firstB < 0) firstB = level.getGameTime();
        if (trace.chars().filter(c -> c == 'B').count() == 3) level.setBlock(pos, state.setValue(OFF, true), 3);
    }

    static final class Entity extends BlockEntity {
        Entity(BlockPos pos, BlockState state) {
            super(TYPE.get(), pos, state);
        }
    }
}
