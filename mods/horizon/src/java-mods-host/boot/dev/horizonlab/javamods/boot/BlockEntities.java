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
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.util.ProblemReporter;

/** Real block entities on the views. A mod's lives here, with its saved form in Horizon's chunk; a vanilla one is a copy for one call, written back if changed. */
public final class BlockEntities {
    private BlockEntities() {}

    private static final org.slf4j.Logger LOGGER = com.mojang.logging.LogUtils.getLogger();
    private static final Map<ServerLevel, Long2ObjectOpenHashMap<BlockEntity>> HELD = new IdentityHashMap<>();
    private static final List<BlockEntity> COPIES = new ArrayList<>();
    private static final List<BlockEntity> FRESH = new ArrayList<>();
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

    static Long2ObjectOpenHashMap<BlockEntity> held(ServerLevel level) {
        return HELD.computeIfAbsent(level, l -> new Long2ObjectOpenHashMap<>());
    }

    private static boolean ours(BlockState state) {
        return !BuiltInRegistries.BLOCK.getKey(state.getBlock()).getNamespace().equals("minecraft");
    }

    /** `LevelChunk.setBlockEntity`'s steps: the entity joins the level and replaces whichever held its place. */
    private static void join(ServerLevel level, BlockEntity entity) {
        entity.setLevel(level);
        entity.clearRemoved();
        BlockEntity previous = held(level).put(entity.getBlockPos().asLong(), entity);
        if (previous != null && previous != entity) previous.setRemoved();
        Clients.joined(level, entity);
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
                join(level, entity);
                if (ours(state)) {
                    adopt(key, BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(entity.getType()).toString());
                    FRESH.add(entity);
                } else COPIES.add(entity);
            }
        }
        return entity;
    }

    public static void load(ServerLevel level, BlockPos pos, CompoundTag tag) {
        if (System.getenv("HORIZON_JAVA_DEBUG") != null) System.err.println("Horizon: loading a block entity: " + tag);
        BlockState state = level.getBlockState(pos);
        BlockEntity entity = BlockEntity.loadStatic(pos.immutable(), state, tag, level.registryAccess());
        if (entity == null) return;
        join(level, entity);
        if (!ours(state)) COPIES.add(entity);
        else FRESH.add(entity);
    }

    /** `LevelChunk.getBlockEntities` of a chunk view: the block entities Java holds in it. */
    public static Map<BlockPos, BlockEntity> inChunk(ServerLevel level, net.minecraft.world.level.ChunkPos chunk) {
        var found = new java.util.HashMap<BlockPos, BlockEntity>();
        for (BlockEntity entity : held(level).values()) {
            if (!entity.isRemoved() && net.minecraft.world.level.ChunkPos.containing(entity.getBlockPos()).equals(chunk)) found.put(entity.getBlockPos(), entity);
        }
        return found;
    }

    static CompoundTag data(ServerLevel level, BlockPos pos) {
        BlockEntity entity = held(level).get(pos.asLong());
        return entity == null || entity.isRemoved() ? null : entity.saveWithFullMetadata(level.registryAccess());
    }

    static boolean setData(ServerLevel level, BlockPos pos, CompoundTag tag) {
        BlockEntity entity = held(level).get(pos.asLong());
        if (entity == null || entity.isRemoved()) return false;
        BlockState state = level.getBlockState(pos);
        try (var reporter = new ProblemReporter.ScopedCollector(entity.problemPath(), LOGGER)) {
            entity.loadWithComponents(TagValueInput.create(reporter, level.registryAccess(), tag));
            entity.setChanged();
            level.sendBlockUpdated(pos, state, state, 3);
        }
        return true;
    }

    public static void set(ServerLevel level, BlockEntity entity) {
        long key = entity.getBlockPos().asLong();
        boolean fresh = held(level).get(key) == null;
        join(level, entity);
        if (fresh && ours(entity.getBlockState())) {
            adopt(key, BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(entity.getType()).toString());
            FRESH.add(entity);
        }
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

    /** `Level.tickBlockEntities`' first step: block entities that joined the level since get `onLoad`, before any ticker runs. */
    static void loadFresh() {
        while (!FRESH.isEmpty()) {
            var batch = new ArrayList<>(FRESH);
            FRESH.clear();
            for (BlockEntity entity : batch) {
                if (entity.isRemoved()) continue;
                try {
                    entity.onLoad();
                } catch (Throwable t) {
                    t.printStackTrace();
                }
            }
        }
    }

    /** `BlockEntityTicker.tick`, as vanilla's list does it; false when this one's saved form hasn't reached Java. */
    static boolean tick(ServerLevel level, BlockPos pos) {
        loadFresh();
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
