package dev.horizonlab.javamods.boot;

import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.EntityBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.BlockEntityTicker;
import net.minecraft.world.level.block.state.BlockState;

/** Real block entities on the views. A mod's lives here, with its saved form in Horizon's chunk; a vanilla one is a copy for one call, written back if changed. */
public final class BlockEntities {
    private BlockEntities() {}

    private static final Map<ServerLevel, Long2ObjectOpenHashMap<BlockEntity>> HELD = new IdentityHashMap<>();
    private static final List<BlockEntity> COPIES = new ArrayList<>();
    private static final List<BlockEntity> CHANGED_COPIES = new ArrayList<>();

    /** Bridged: sends what Horizon holds at this position to `load`. */
    private static void fetch(long pos) {
        throw new IllegalStateException("not bridged");
    }

    /** Bridged: Horizon keeps a place for a new block entity Java made. */
    private static void adopt(long pos, String id) {
        throw new IllegalStateException("not bridged");
    }

    /** Bridged: the block entity at this position is gone. */
    private static void forget(long pos) {
        throw new IllegalStateException("not bridged");
    }

    private static Long2ObjectOpenHashMap<BlockEntity> held(ServerLevel level) {
        return HELD.computeIfAbsent(level, l -> new Long2ObjectOpenHashMap<>());
    }

    private static boolean ours(BlockState state) {
        return !BuiltInRegistries.BLOCK.getKey(state.getBlock()).getNamespace().equals("minecraft");
    }

    public static BlockEntity get(ServerLevel level, BlockPos pos) {
        var held = held(level);
        long key = pos.asLong();
        BlockEntity entity = held.get(key);
        if (entity != null && !entity.isRemoved()) return entity;
        BlockState state = level.getBlockState(pos);
        if (!state.hasBlockEntity()) return null;
        fetch(key);
        entity = held.get(key);
        if (entity == null && state.getBlock() instanceof EntityBlock block) {
            entity = block.newBlockEntity(pos.immutable(), state);
            if (entity != null) {
                entity.setLevel(level);
                held.put(key, entity);
                if (ours(state)) adopt(key, BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(entity.getType()).toString());
                else COPIES.add(entity);
            }
        }
        return entity;
    }

    public static void load(ServerLevel level, BlockPos pos, CompoundTag tag) {
        if (System.getenv("HORIZON_JAVA_DEBUG") != null) System.err.println("Horizon: loading a block entity: " + tag);
        BlockState state = level.getBlockState(pos);
        BlockEntity entity = BlockEntity.loadStatic(pos.immutable(), state, tag, level.registryAccess());
        if (entity == null) return;
        entity.setLevel(level);
        held(level).put(pos.asLong(), entity);
        if (!ours(state)) COPIES.add(entity);
    }

    public static void set(ServerLevel level, BlockEntity entity) {
        entity.setLevel(level);
        long key = entity.getBlockPos().asLong();
        boolean fresh = held(level).put(key, entity) == null;
        if (fresh && ours(entity.getBlockState())) adopt(key, BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(entity.getType()).toString());
        if (!ours(entity.getBlockState())) CHANGED_COPIES.add(entity);
    }

    public static void remove(ServerLevel level, BlockPos pos) {
        BlockEntity entity = held(level).remove(pos.asLong());
        if (entity != null) {
            entity.setRemoved();
            forget(pos.asLong());
        }
    }

    /** `Level.blockEntityChanged`: a mod's saves when the world does, a vanilla copy goes back after this call. */
    public static void changed(ServerLevel level, BlockPos pos) {
        BlockEntity entity = held(level).get(pos.asLong());
        if (entity != null && COPIES.contains(entity) && !CHANGED_COPIES.contains(entity)) CHANGED_COPIES.add(entity);
    }

    /** The vanilla copies this call changed, as their saved form, and every copy dropped. */
    static List<CompoundTag> takeChangedCopies() {
        var saved = new ArrayList<CompoundTag>();
        for (BlockEntity entity : CHANGED_COPIES) saved.add(entity.saveWithFullMetadata(entity.getLevel().registryAccess()));
        CHANGED_COPIES.clear();
        for (BlockEntity entity : COPIES) held((ServerLevel) entity.getLevel()).remove(entity.getBlockPos().asLong());
        COPIES.clear();
        return saved;
    }

    static void abandonCopies() {
        CHANGED_COPIES.clear();
        for (BlockEntity entity : COPIES) held((ServerLevel) entity.getLevel()).remove(entity.getBlockPos().asLong());
        COPIES.clear();
    }

    static List<CompoundTag> saveAll(ServerLevel level) {
        var saved = new ArrayList<CompoundTag>();
        for (BlockEntity entity : held(level).values()) saved.add(entity.saveWithFullMetadata(level.registryAccess()));
        return saved;
    }

    static boolean ticks(net.minecraft.world.level.block.Block block, ServerLevel level) {
        if (!(block instanceof EntityBlock entityBlock)) return false;
        BlockState state = block.defaultBlockState();
        BlockEntity entity = entityBlock.newBlockEntity(BlockPos.ZERO, state);
        return entity != null && entityBlock.getTicker(level, state, entity.getType()) != null;
    }

    /** `BlockEntityTicker.tick`, as vanilla's list does it; false when this one's saved form hasn't reached Java. */
    static boolean tick(ServerLevel level, BlockPos pos) {
        BlockEntity entity = held(level).get(pos.asLong());
        if (entity == null) return false;
        if (entity.isRemoved()) return true;
        BlockState state = level.getBlockState(pos);
        if (state.getBlock() instanceof EntityBlock block) tickWith(block, level, state, entity);
        return true;
    }

    private static <T extends BlockEntity> void tickWith(EntityBlock block, ServerLevel level, BlockState state, T entity) {
        @SuppressWarnings("unchecked")
        var type = (net.minecraft.world.level.block.entity.BlockEntityType<T>) entity.getType();
        BlockEntityTicker<T> ticker = block.getTicker(level, state, type);
        if (ticker != null) ticker.tick(level, entity.getBlockPos(), state, entity);
    }

    /** `LevelChunk.setBlockState`'s steps after the storage write, in its order, for a mod's block. */
    static void blockChanged(ServerLevel level, BlockPos pos, BlockState old, BlockState now) {
        if (old.getBlock() != now.getBlock()) {
            BlockEntity entity = held(level).get(pos.asLong());
            if (entity != null) {
                entity.preRemoveSideEffects(pos, old);
                remove(level, pos);
            }
            if (ours(old)) old.affectNeighborsAfterRemoval(level, pos, false);
        }
        if (ours(now)) {
            now.onPlace(level, pos, old, false);
            if (now.hasBlockEntity()) {
                BlockEntity entity = get(level, pos);
                if (entity != null) entity.setBlockState(now);
            }
        }
    }
}
