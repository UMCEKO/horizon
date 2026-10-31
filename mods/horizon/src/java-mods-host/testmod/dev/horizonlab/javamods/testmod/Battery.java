package dev.horizonlab.javamods.testmod;

import java.util.function.Supplier;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.RandomSource;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.BaseEntityBlock;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.BlockEntityTicker;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.redstone.Orientation;
import net.minecraft.world.level.storage.ValueInput;
import net.minecraft.world.level.storage.ValueOutput;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.core.registries.Registries;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.capabilities.RegisterCapabilitiesEvent;
import net.neoforged.neoforge.registries.DeferredRegister;
import net.neoforged.neoforge.transfer.energy.EnergyHandler;
import net.neoforged.neoforge.transfer.energy.SimpleEnergyHandler;
import net.neoforged.neoforge.transfer.transaction.Transaction;
import org.jspecify.annotations.Nullable;

/** A machine as mods write one: its own block class, a block entity that ticks and saves, and real Forge Energy to its east. */
public final class Battery extends BaseEntityBlock {
    private static final DeferredRegister.Blocks BLOCKS = DeferredRegister.createBlocks("horizon_java_test");
    private static final DeferredRegister<BlockEntityType<?>> TYPES = DeferredRegister.create(Registries.BLOCK_ENTITY_TYPE, "horizon_java_test");
    public static final Supplier<Block> BLOCK = BLOCKS.registerBlock("battery", Battery::new);
    public static final Supplier<BlockEntityType<Entity>> TYPE = TYPES.register("battery", () -> new BlockEntityType<>(Entity::new, BLOCK.get()));

    static void register(IEventBus bus, DeferredRegister.Items items) {
        items.registerSimpleBlockItem("battery", BLOCK);
        BLOCKS.register(bus);
        TYPES.register(bus);
        bus.addListener(RegisterCapabilitiesEvent.class, e -> e.registerBlockEntity(Capabilities.Energy.BLOCK, TYPE.get(), (entity, side) -> entity.energy));
    }

    public Battery(BlockBehaviour.Properties properties) {
        super(properties);
    }

    @Override
    public BlockEntity newBlockEntity(BlockPos pos, BlockState state) {
        return new Entity(pos, state);
    }

    @Override
    public <T extends BlockEntity> @Nullable BlockEntityTicker<T> getTicker(Level level, BlockState state, BlockEntityType<T> type) {
        return createTickerHelper(type, TYPE.get(), Entity::tick);
    }

    @Override
    protected void onPlace(BlockState state, Level level, BlockPos pos, BlockState old, boolean moved) {
        level.scheduleTick(pos, this, 2);
    }

    @Override
    protected void tick(BlockState state, ServerLevel level, BlockPos pos, RandomSource random) {
        level.setBlock(pos.south(), Blocks.EMERALD_BLOCK.defaultBlockState(), 3);
    }

    @Override
    protected void neighborChanged(BlockState state, Level level, BlockPos pos, Block block, @Nullable Orientation orientation, boolean moved) {
        if (level.getBlockState(pos.below()).is(Blocks.DIRT)) level.setBlock(pos.north(), Blocks.LAPIS_BLOCK.defaultBlockState(), 3);
    }

    @Override
    protected InteractionResult useWithoutItem(BlockState state, Level level, BlockPos pos, Player player, BlockHitResult hit) {
        level.setBlock(pos.above(), Blocks.GOLD_BLOCK.defaultBlockState(), 3);
        return InteractionResult.SUCCESS;
    }

    public static final class Entity extends BlockEntity {
        final SimpleEnergyHandler energy = new SimpleEnergyHandler(1000000) {
            @Override
            protected void onEnergyChanged(int previous) {
                setChanged();
            }
        };
        int ticks;

        public Entity(BlockPos pos, BlockState state) {
            super(TYPE.get(), pos, state);
        }

        static void tick(Level level, BlockPos pos, BlockState state, Entity entity) {
            entity.ticks++;
            entity.setChanged();
            if (level.getBlockState(pos.east()).is(BLOCK.get())) {
                EnergyHandler next = level.getCapability(Capabilities.Energy.BLOCK, pos.east(), Direction.WEST);
                if (next != null) {
                    try (var transaction = Transaction.openRoot()) {
                        next.insert(entity.energy.extract(10, transaction), transaction);
                        transaction.commit();
                    }
                }
            }
        }

        @Override
        protected void saveAdditional(ValueOutput output) {
            super.saveAdditional(output);
            output.putInt("ticks", ticks);
            energy.serialize(output.child("energy"));
        }

        @Override
        protected void loadAdditional(ValueInput input) {
            super.loadAdditional(input);
            ticks = input.getIntOr("ticks", 0);
            input.child("energy").ifPresent(energy::deserialize);
        }
    }
}
