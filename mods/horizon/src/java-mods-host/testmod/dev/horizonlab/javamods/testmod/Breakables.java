package dev.horizonlab.javamods.testmod;

import java.util.List;
import java.util.function.Supplier;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.Registries;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.BaseEntityBlock;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.BlockEntityType;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.level.storage.loot.LootParams;
import net.minecraft.world.level.storage.loot.parameters.LootContextParams;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.registries.DeferredRegister;

/** Mod blocks with a block entity whose breaks are their own code: drops from `getDrops` with no loot table, and a removal the block refuses. */
final class Breakables {
    private Breakables() {}

    private static final DeferredRegister.Blocks BLOCKS = DeferredRegister.createBlocks("horizon_java_test");
    private static final DeferredRegister<BlockEntityType<?>> TYPES = DeferredRegister.create(Registries.BLOCK_ENTITY_TYPE, "horizon_java_test");
    static final Supplier<Block> DROPPER = BLOCKS.registerBlock("dropper", p -> new Dropper(p.destroyTime(0.1f)));
    static final Supplier<Block> ANCHOR = BLOCKS.registerBlock("anchor", p -> new Anchor(p.destroyTime(0.1f)));
    static final Supplier<BlockEntityType<Entity>> TYPE = TYPES.register("breakable", () -> new BlockEntityType<>(Entity::new, DROPPER.get(), ANCHOR.get()));

    static void register(IEventBus bus, DeferredRegister.Items items) {
        items.registerSimpleBlockItem("dropper", DROPPER);
        items.registerSimpleBlockItem("anchor", ANCHOR);
        BLOCKS.register(bus);
        TYPES.register(bus);
    }

    static final class Entity extends BlockEntity {
        Entity(BlockPos pos, BlockState state) {
            super(TYPE.get(), pos, state);
        }
    }

    static final class Dropper extends BaseEntityBlock {
        static int will;
        static int destroyed;

        Dropper(BlockBehaviour.Properties properties) {
            super(properties);
        }

        @Override
        public BlockEntity newBlockEntity(BlockPos pos, BlockState state) {
            return new Entity(pos, state);
        }

        @Override
        protected List<ItemStack> getDrops(BlockState state, LootParams.Builder builder) {
            return builder.getOptionalParameter(LootContextParams.BLOCK_ENTITY) instanceof Entity ? List.of(new ItemStack(Items.DIAMOND)) : List.of();
        }

        @Override
        public BlockState playerWillDestroy(Level level, BlockPos pos, BlockState state, Player player) {
            will++;
            return super.playerWillDestroy(level, pos, state, player);
        }

        @Override
        public void destroy(net.minecraft.world.level.LevelAccessor level, BlockPos pos, BlockState state) {
            destroyed++;
        }

        @Override
        public void playerDestroy(ServerLevel level, net.minecraft.server.level.ServerPlayer player, BlockPos pos, BlockState state, BlockEntity entity, ItemStack tool) {
            super.playerDestroy(level, player, pos, state, entity, tool);
            System.out.println("CHECK a mod block's break runs willDestroy, destroy and getDrops once " + (will == 1 && destroyed == 1 && entity instanceof Entity ? "ok" : "FAILED"));
        }
    }

    static final class Anchor extends BaseEntityBlock {
        static BlockEntity kept;
        static int dropped;

        Anchor(BlockBehaviour.Properties properties) {
            super(properties);
        }

        @Override
        public BlockEntity newBlockEntity(BlockPos pos, BlockState state) {
            return new Entity(pos, state);
        }

        @Override
        public boolean onDestroyedByPlayer(BlockState state, Level level, BlockPos pos, Player player, ItemStack tool, boolean willHarvest, FluidState fluid) {
            kept = level.getBlockEntity(pos);
            level.scheduleTick(pos, this, 2);
            return false;
        }

        @Override
        public void playerDestroy(ServerLevel level, net.minecraft.server.level.ServerPlayer player, BlockPos pos, BlockState state, BlockEntity entity, ItemStack tool) {
            dropped++;
        }

        @Override
        protected void tick(BlockState state, ServerLevel level, BlockPos pos, RandomSource random) {
            System.out.println("CHECK a block that refuses removal stays with its block entity and drops nothing " + (kept != null && level.getBlockEntity(pos) == kept && dropped == 0 ? "ok" : "FAILED"));
        }
    }
}
