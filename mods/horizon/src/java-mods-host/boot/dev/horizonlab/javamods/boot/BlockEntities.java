package dev.horizonlab.javamods.boot;

import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import java.util.ArrayList;
import java.util.Collections;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.BaseRailBlock;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.EntityBlock;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.entity.BlockEntityTicker;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.util.ProblemReporter;

/** Real block entities on the views, one object per position from load or placement until removal or chunk unload. A mod's saved form lives in Horizon's chunk; a vanilla one is written through to it as it changes. */
public final class BlockEntities {
    private BlockEntities() {}

    private static final org.slf4j.Logger LOGGER = com.mojang.logging.LogUtils.getLogger();
    private static final Map<ServerLevel, Long2ObjectOpenHashMap<BlockEntity>> HELD = new IdentityHashMap<>();
    private static final Set<BlockEntity> VANILLA = Collections.newSetFromMap(new IdentityHashMap<>());
    private static final List<BlockEntity> FRESH = new ArrayList<>();
    private static boolean refreshing;
    /** `ChunkAccess.markUnsaved` by level: chunks whose block entities Java changed since Horizon last took their saved forms. */
    private static final Map<ServerLevel, LongOpenHashSet> UNSAVED = new IdentityHashMap<>();

    /** Bridged: sends what Horizon holds at this position to `load`. */
    private static void fetch(long pos) {
        throw new IllegalStateException("not bridged");
    }

    /** Bridged: Horizon keeps a place for a new block entity Java made. */
    private static void adopt(long pos, String id) {
        throw new IllegalStateException("not bridged");
    }

    /** Bridged: Horizon's record of a vanilla block entity becomes this saved form. */
    private static void store(long pos, byte[] tag) {
        throw new IllegalStateException("not bridged");
    }

    /** Bridged: the block entity at this position is gone. */
    private static void forget(long pos) {
        throw new IllegalStateException("not bridged");
    }

    static Long2ObjectOpenHashMap<BlockEntity> held(ServerLevel level) {
        return HELD.computeIfAbsent(level, l -> new Long2ObjectOpenHashMap<>());
    }

    static void reset() {
        HELD.clear();
        VANILLA.clear();
        FRESH.clear();
        UNSAVED.clear();
    }

    static boolean ours(BlockState state) {
        return BootFacts.ownsBlock(state.getBlock());
    }

    /** `LevelChunk.setBlockEntity`'s steps: the entity joins the level and replaces whichever held its place. */
    private static void join(ServerLevel level, BlockEntity entity) {
        entity.setLevel(level);
        entity.clearRemoved();
        BlockEntity previous = held(level).put(entity.getBlockPos().asLong(), entity);
        if (previous != null && previous != entity) {
            previous.setRemoved();
            VANILLA.remove(previous);
        }
        if (previous != entity) level.invalidateCapabilities(entity.getBlockPos());
        Clients.joined(level, entity);
    }

    /** `LevelChunk.getBlockEntity` with `IMMEDIATE`: a removed one leaves the map and answers null, never reloaded from Horizon's record. */
    public static BlockEntity get(ServerLevel level, BlockPos pos) {
        var held = held(level);
        long key = pos.asLong();
        BlockEntity entity = held.get(key);
        if (entity != null) {
            if (!entity.isRemoved()) return entity;
            held.remove(key);
            VANILLA.remove(entity);
            return null;
        }
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
                } else VANILLA.add(entity);
            }
        }
        return entity;
    }

    public static void load(ServerLevel level, BlockPos pos, CompoundTag tag) {
        if (System.getenv("HORIZON_JAVA_DEBUG") != null) System.err.println("Horizon: loading a block entity: " + tag);
        BlockState state = level.getBlockState(pos);
        BlockEntity held = held(level).get(pos.asLong());
        if (held != null && !held.isRemoved() && BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(held.getType()).toString().equals(tag.getStringOr("id", ""))) {
            refresh(level, held, tag);
            return;
        }
        BlockEntity entity = BlockEntity.loadStatic(pos.immutable(), state, tag, level.registryAccess());
        if (entity == null) return;
        join(level, entity);
        if (!ours(state)) VANILLA.add(entity);
        else FRESH.add(entity);
    }

    /** The saved form Horizon holds is newer than the object: it reads it in place, so what points at the object stays right. */
    private static void refresh(ServerLevel level, BlockEntity entity, CompoundTag tag) {
        boolean before = refreshing;
        refreshing = true;
        try (var reporter = new ProblemReporter.ScopedCollector(entity.problemPath(), LOGGER)) {
            entity.loadWithComponents(TagValueInput.create(reporter, level.registryAccess(), tag));
        } finally {
            refreshing = before;
        }
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
        refresh(level, entity, tag);
        entity.setChanged();
        level.sendBlockUpdated(pos, state, state, 3);
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
        if (ours(entity.getBlockState())) markUnsaved(level, entity.getBlockPos());
        else {
            VANILLA.add(entity);
            write(level, entity);
        }
    }

    /** Write-through: Horizon's record of a vanilla block entity takes its saved form now. */
    private static void write(ServerLevel level, BlockEntity entity) {
        var bytes = new java.io.ByteArrayOutputStream();
        try {
            net.minecraft.nbt.NbtIo.write(entity.saveWithFullMetadata(level.registryAccess()), new java.io.DataOutputStream(bytes));
        } catch (java.io.IOException e) {
            throw new java.io.UncheckedIOException(e);
        }
        store(entity.getBlockPos().asLong(), bytes.toByteArray());
    }

    private static void markUnsaved(ServerLevel level, BlockPos pos) {
        UNSAVED.computeIfAbsent(level, l -> new LongOpenHashSet()).add(ChunkPos.pack(pos));
    }

    public static void remove(ServerLevel level, BlockPos pos) {
        if (drop(level, pos)) forget(pos.asLong());
    }

    /** `LevelChunk.removeBlockEntity` without telling Horizon, for a block entity it already lost. */
    static boolean drop(ServerLevel level, BlockPos pos) {
        BlockEntity entity = held(level).remove(pos.asLong());
        if (entity == null) return false;
        entity.setRemoved();
        VANILLA.remove(entity);
        level.invalidateCapabilities(pos);
        return true;
    }

    /** `Level.blockEntityChanged`: a mod's saves when the world does, a vanilla one goes to Horizon's record at once. */
    public static void changed(ServerLevel level, BlockPos pos) {
        BlockEntity entity = held(level).get(pos.asLong());
        if (entity == null) return;
        if (!VANILLA.contains(entity)) markUnsaved(level, pos);
        else if (!refreshing) write(level, entity);
    }

    /** `ChunkMap.save` of the chunk's unsaved block entities, then `ServerLevel.unload`'s `clearAllBlockEntities` (every one removed, then the map cleared); the chunk's own record of them goes with Horizon's unload. */
    static void unload(ServerLevel level, int x, int z, List<CompoundTag> saved) {
        long chunk = ChunkPos.pack(x, z);
        var held = held(level);
        var here = new ArrayList<BlockEntity>();
        for (BlockEntity entity : held.values()) {
            if (ChunkPos.pack(entity.getBlockPos()) == chunk) here.add(entity);
        }
        var unsaved = UNSAVED.get(level);
        if (unsaved != null && unsaved.remove(chunk)) {
            for (BlockEntity entity : here) {
                if (!entity.isRemoved() && !VANILLA.contains(entity)) saved.add(entity.saveWithFullMetadata(level.registryAccess()));
            }
        }
        for (BlockEntity entity : here) {
            try {
                entity.onChunkUnloaded();
            } catch (Throwable t) {
                t.printStackTrace();
            }
        }
        for (BlockEntity entity : here) entity.setRemoved();
        for (BlockEntity entity : here) {
            VANILLA.remove(entity);
            held.remove(entity.getBlockPos().asLong(), entity);
        }
    }

    static List<CompoundTag> saveAll(ServerLevel level) {
        UNSAVED.remove(level);
        var saved = new ArrayList<CompoundTag>();
        for (BlockEntity entity : held(level).values()) {
            if (!VANILLA.contains(entity)) saved.add(entity.saveWithFullMetadata(level.registryAccess()));
        }
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
        if (ticker == null) return;
        long started = Profile.ON ? System.nanoTime() : 0;
        ticker.tick(level, entity.getBlockPos(), state, entity);
        if (Profile.ON) Profile.tick(type, System.nanoTime() - started);
    }

    /** `LevelChunk.getBlockEntity(pos, CHECK)`: the one held, or the saved form Horizon keeps for it, never a new one. */
    private static BlockEntity check(ServerLevel level, BlockPos pos) {
        long key = pos.asLong();
        BlockEntity entity = held(level).get(key);
        if (entity == null) {
            fetch(key);
            entity = held(level).get(key);
        }
        if (entity != null && entity.isRemoved()) {
            held(level).remove(key);
            return null;
        }
        return entity;
    }

    /** `LevelChunk.setBlockState` after the storage write, in its order, with the flags of the `setBlock` that made it; Horizon ran the steps of vanilla's own blocks. */
    static void blockChanged(ServerLevel level, BlockPos pos, BlockState old, BlockState now, int flags) {
        if ((flags & 2) != 0) Clients.blockChanged(level, pos);
        Block newBlock = now.getBlock();
        boolean blockChanged = !old.is(newBlock);
        boolean movedByPiston = (flags & 64) != 0;
        boolean sideEffects = (flags & 256) == 0;
        if (blockChanged && old.hasBlockEntity()) {
            if (!now.shouldChangedStateKeepBlockEntity(old)) {
                BlockEntity entity = held(level).get(pos.asLong());
                if (entity != null && !entity.isRemoved() && sideEffects) entity.preRemoveSideEffects(pos, old);
                if (!held(level).containsKey(pos.asLong())) level.invalidateCapabilities(pos);
                remove(level, pos);
            } else {
                // Horizon's chunk let its record go with the old block, which only vanilla's kept-entity blocks keep.
                BlockEntity entity = held(level).get(pos.asLong());
                if (entity != null && !entity.isRemoved()) adopt(pos.asLong(), BuiltInRegistries.BLOCK_ENTITY_TYPE.getKey(entity.getType()).toString());
            }
        }
        if ((blockChanged || newBlock instanceof BaseRailBlock) && ((flags & 1) != 0 || movedByPiston) && ours(old)) {
            old.affectNeighborsAfterRemoval(level, pos, movedByPiston);
        }
        if (!level.getBlockState(pos).is(newBlock)) return;
        if (!ours(now)) {
            if (blockChanged && now.hasBlockEntity()) get(level, pos);
            return;
        }
        if ((flags & 512) == 0) now.onPlace(level, pos, old, movedByPiston);
        if (now.hasBlockEntity() && level.getBlockState(pos).is(newBlock)) {
            BlockEntity entity = check(level, pos);
            if (entity != null && !entity.isValidBlockState(now)) {
                LOGGER.warn("Found mismatched block entity @ {}: type = {}, state = {}", pos, entity.typeHolder().getRegisteredName(), now);
                remove(level, pos);
                entity = null;
            }
            if (entity == null) {
                entity = ((EntityBlock) newBlock).newBlockEntity(pos, now);
                if (entity != null) set(level, entity);
            } else {
                entity.setBlockState(now);
            }
        }
    }
}
