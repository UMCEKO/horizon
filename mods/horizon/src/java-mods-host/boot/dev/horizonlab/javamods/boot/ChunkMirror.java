package dev.horizonlab.javamods.boot;

import io.netty.buffer.ByteBufUtil;
import io.netty.buffer.Unpooled;
import it.unimi.dsi.fastutil.longs.LongOpenHashSet;
import it.unimi.dsi.fastutil.longs.LongSet;
import it.unimi.dsi.fastutil.shorts.ShortList;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.Function;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.Registry;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtIo;
import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.util.Mth;
import net.minecraft.util.SimpleBitStorage;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.LevelHeightAccessor;
import net.minecraft.world.level.biome.Biome;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.ChunkAccess;
import net.minecraft.world.level.chunk.LevelChunkSection;
import net.minecraft.world.level.chunk.PalettedContainer;
import net.minecraft.world.level.chunk.PalettedContainerFactory;
import net.minecraft.world.level.chunk.ProtoChunk;
import net.minecraft.world.level.chunk.UpgradeData;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.level.levelgen.structure.Structure;
import net.minecraft.world.level.levelgen.structure.StructureStart;
import net.minecraft.world.level.levelgen.structure.pieces.StructurePieceSerializationContext;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.ticks.ProtoChunkTicks;
import net.minecraft.world.ticks.SavedTick;
import net.minecraft.world.ticks.ScheduledTick;
import net.minecraft.world.ticks.TickContainerAccess;
import net.minecraft.world.ticks.TickPriority;
import org.jspecify.annotations.Nullable;

/** Native's proto chunk as a real `ProtoChunk` and back, and a region session's writes: the layout of Rust's `worldgen/mirror.rs`. */
public final class ChunkMirror {
    private ChunkMirror() {}

    public static final int FORMAT = 1;

    /** Once per session: the level, the biome names whose indices the sections carry, the seed. */
    public record Header(String level, List<String> biomes, long seed) {
        public static Header read(byte[] bytes) {
            var in = new FriendlyByteBuf(Unpooled.wrappedBuffer(bytes));
            int format = in.readInt();
            if (format != FORMAT) throw new IllegalArgumentException("Horizon: mirror format " + format + ", not " + FORMAT);
            String level = text(in);
            int n = in.readInt();
            var biomes = new ArrayList<String>(n);
            for (int i = 0; i < n; i++) biomes.add(text(in));
            return new Header(level, List.copyOf(biomes), in.readLong());
        }

        public byte[] write() {
            var out = new FriendlyByteBuf(Unpooled.buffer());
            out.writeInt(FORMAT);
            text(out, this.level);
            out.writeInt(this.biomes.size());
            this.biomes.forEach(b -> text(out, b));
            out.writeLong(this.seed);
            return ByteBufUtil.getBytes(out);
        }
    }

    /** A region session: the header's biomes resolved, and the log its mirror chunks append to. */
    public static final class Session {
        final Header header;
        final RegistryAccess registries;
        final @Nullable StructurePieceSerializationContext structures;
        final List<Holder<Biome>> biomes = new ArrayList<>();
        final Map<Holder<Biome>, Integer> biomeIndex = new HashMap<>();
        final WriteLog log = new WriteLog(this);
        // While a session applies the other side's writes or reads a chunk, its mirrors don't log them back.
        boolean applying;

        public Session(Header header, RegistryAccess registries, @Nullable StructurePieceSerializationContext structures) {
            this.header = header;
            this.registries = registries;
            this.structures = structures;
            Registry<Biome> registry = registries.lookupOrThrow(Registries.BIOME);
            for (String name : header.biomes()) {
                Holder<Biome> biome = registry.getOrThrow(ResourceKey.create(Registries.BIOME, Identifier.parse(name)));
                this.biomeIndex.putIfAbsent(biome, this.biomes.size());
                this.biomes.add(biome);
            }
        }

        public WriteLog log() {
            return this.log;
        }
    }

    /** A real `ProtoChunk` with real sections, so `setBlockState` keeps heightmaps as vanilla does; `mirror` makes it log to the session. */
    public static ProtoChunk read(byte[] bytes, LevelHeightAccessor height, Session session, PalettedContainerFactory factory, boolean mirror) {
        var in = new FriendlyByteBuf(Unpooled.wrappedBuffer(bytes));
        var pos = new ChunkPos(in.readInt(), in.readInt());
        int minY = in.readInt();
        int count = in.readInt();
        if (minY != height.getMinY() || count != height.getSectionsCount()) throw new IllegalArgumentException("Horizon: mirrored chunk spans " + minY + " and " + count + " sections");
        var sections = new LevelChunkSection[count];
        int biomeBits = Mth.ceillog2(session.biomes.size());
        for (int i = 0; i < count; i++) {
            if (!in.readBoolean()) {
                sections[i] = new LevelChunkSection(factory);
                continue;
            }
            PalettedContainer<BlockState> states = factory.createForBlockStates();
            states.read(in);
            PalettedContainer<Holder<Biome>> biomes = factory.createForBiomes();
            int[] values = readValues(in, 64, biomeBits);
            for (int j = 0; j < 64; j++) biomes.getAndSetUnchecked(j & 3, j >> 4, (j >> 2) & 3, session.biomes.get(values[j]));
            sections[i] = new LevelChunkSection(states, biomes);
            sections[i].recalcBlockCounts();
        }
        int words = new SimpleBitStorage(Mth.ceillog2(height.getHeight() + 1), 256).getRaw().length;
        var heightmaps = new HashMap<Heightmap.Types, long[]>();
        for (int n = in.readInt(); n > 0; n--) {
            var type = Heightmap.Types.values()[in.readByte()];
            long[] raw = new long[words];
            for (int j = 0; j < words; j++) raw[j] = in.readLong();
            heightmaps.put(type, raw);
        }
        var blockEntities = new ArrayList<CompoundTag>();
        for (int n = in.readInt(); n > 0; n--) {
            var at = BlockPos.of(in.readLong());
            var tag = tag(in);
            tag.putInt("x", at.getX());
            tag.putInt("y", at.getY());
            tag.putInt("z", at.getZ());
            blockEntities.add(tag);
        }
        var entities = new ArrayList<CompoundTag>();
        for (int n = in.readInt(); n > 0; n--) entities.add(tag(in));
        var blockTicks = new ArrayList<SavedTick<Block>>();
        for (int n = in.readInt(); n > 0; n--) blockTicks.add(savedTick(in, id -> Block.BLOCK_STATE_REGISTRY.byIdOrThrow(id).getBlock()));
        var fluidTicks = new ArrayList<SavedTick<Fluid>>();
        for (int n = in.readInt(); n > 0; n--) fluidTicks.add(savedTick(in, BuiltInRegistries.FLUID::byIdOrThrow));
        ProtoChunk chunk = mirror
            ? new MirrorProtoChunk(pos, sections, ProtoChunkTicks.load(blockTicks), ProtoChunkTicks.load(fluidTicks), height, factory, session)
            : new ProtoChunk(pos, UpgradeData.EMPTY, sections, ProtoChunkTicks.load(blockTicks), ProtoChunkTicks.load(fluidTicks), height, factory, null);
        boolean was = session.applying;
        session.applying = true;
        try {
            heightmaps.forEach(chunk::setHeightmap);
            blockEntities.forEach(chunk::setBlockEntityNbt);
            entities.forEach(chunk::addEntity);
            for (int n = in.readInt(); n > 0; n--) {
                short packed = in.readShort();
                ChunkAccess.getOrCreateOffsetList(chunk.getPostProcessing(), in.readByte()).add(packed);
            }
            chunk.setPersistedStatus(ChunkStatus.getStatusList().get(in.readByte()));
            Registry<Structure> structures = session.registries.lookupOrThrow(Registries.STRUCTURE);
            var starts = new HashMap<Structure, StructureStart>();
            for (int n = in.readInt(); n > 0; n--) {
                Structure structure = structures.getValueOrThrow(ResourceKey.create(Registries.STRUCTURE, Identifier.parse(text(in))));
                var tag = tag(in);
                StructureStart start = session.structures == null ? null : StructureStart.loadStaticStart(session.structures, tag, session.header.seed());
                if (start != null) starts.put(structure, start);
            }
            chunk.setAllStarts(starts);
            var references = new HashMap<Structure, LongSet>();
            for (int n = in.readInt(); n > 0; n--) {
                Structure structure = structures.getValueOrThrow(ResourceKey.create(Registries.STRUCTURE, Identifier.parse(text(in))));
                var set = new LongOpenHashSet();
                for (int k = in.readInt(); k > 0; k--) set.add(in.readLong());
                references.put(structure, set);
            }
            chunk.setAllReferences(references);
        } finally {
            session.applying = was;
        }
        return chunk;
    }

    /** A mirror with nothing in it yet, for a region's chunk the other side doesn't send. */
    public static MirrorProtoChunk empty(ChunkPos pos, LevelHeightAccessor height, Session session, PalettedContainerFactory factory) {
        return new MirrorProtoChunk(pos, null, new ProtoChunkTicks<>(), new ProtoChunkTicks<>(), height, factory, session);
    }

    /** The whole chunk, for a reply that replaces native's. */
    public static byte[] writeChunk(ProtoChunk chunk, Session session) {
        var out = new FriendlyByteBuf(Unpooled.buffer());
        out.writeInt(chunk.getPos().x());
        out.writeInt(chunk.getPos().z());
        out.writeInt(chunk.getMinY());
        out.writeInt(chunk.getSectionsCount());
        int biomeBits = Mth.ceillog2(session.biomes.size());
        for (LevelChunkSection section : chunk.getSections()) {
            out.writeBoolean(true);
            section.getStates().write(out);
            int[] values = new int[64];
            for (int j = 0; j < 64; j++) {
                Integer index = session.biomeIndex.get(section.getBiomes().get(j & 3, j >> 4, (j >> 2) & 3));
                if (index == null) throw new IllegalStateException("Horizon: a biome the session didn't name");
                values[j] = index;
            }
            writeValues(out, values, biomeBits);
        }
        var heightmaps = chunk.getHeightmaps();
        out.writeInt(heightmaps.size());
        for (var entry : heightmaps) {
            out.writeByte(entry.getKey().ordinal());
            for (long word : entry.getValue().getRawData()) out.writeLong(word);
        }
        var positions = chunk.getBlockEntitiesPos();
        var blockEntities = new ArrayList<CompoundTag>();
        for (BlockPos at : positions) {
            CompoundTag tag = chunk.getBlockEntityNbtForSaving(at, session.registries);
            if (tag != null) blockEntities.add(tag);
        }
        out.writeInt(blockEntities.size());
        for (CompoundTag tag : blockEntities) {
            out.writeLong(BlockPos.asLong(tag.getIntOr("x", 0), tag.getIntOr("y", 0), tag.getIntOr("z", 0)));
            tag(out, withoutPosition(tag));
        }
        out.writeInt(chunk.getEntities().size());
        chunk.getEntities().forEach(tag -> tag(out, tag));
        var ticks = chunk.getTicksForSerialization(0L);
        out.writeInt(ticks.blocks().size());
        for (var tick : ticks.blocks()) tickOut(out, tick, Flat.raw((Block) tick.type()));
        out.writeInt(ticks.fluids().size());
        for (var tick : ticks.fluids()) tickOut(out, tick, BuiltInRegistries.FLUID.getId((Fluid) tick.type()));
        var marks = chunk.getPostProcessing();
        int total = 0;
        for (ShortList list : marks) total += list == null ? 0 : list.size();
        out.writeInt(total);
        for (int i = 0; i < marks.length; i++) {
            if (marks[i] == null) continue;
            for (short packed : marks[i]) {
                out.writeShort(packed);
                out.writeByte(i);
            }
        }
        out.writeByte(chunk.getPersistedStatus().getIndex());
        Registry<Structure> structures = session.registries.lookupOrThrow(Registries.STRUCTURE);
        var starts = session.structures == null ? Map.<Structure, StructureStart>of() : chunk.getAllStarts();
        out.writeInt(starts.size());
        starts.forEach((structure, start) -> {
            text(out, structures.getKey(structure).toString());
            tag(out, start.createTag(session.structures, chunk.getPos()));
        });
        var references = chunk.getAllReferences();
        out.writeInt(references.size());
        references.forEach((structure, chunks) -> {
            text(out, structures.getKey(structure).toString());
            out.writeInt(chunks.size());
            chunks.forEach(out::writeLong);
        });
        return ByteBufUtil.getBytes(out);
    }

    /** One write of a region session, in the order it happened. */
    sealed interface Write {
        record Block(BlockPos pos, BlockState state, int flags) implements Write {}

        /** A live block entity is saved when the log is sent, so later changes to it (a loot table) go too. */
        record BlockEntitySet(BlockPos pos, @Nullable BlockEntity entity, @Nullable CompoundTag tag) implements Write {}

        record BlockEntityRemoved(BlockPos pos) implements Write {}

        record EntityAdded(CompoundTag tag) implements Write {}

        record BlockTick(BlockPos pos, net.minecraft.world.level.block.Block block, TickPriority priority) implements Write {}

        record FluidTick(BlockPos pos, Fluid fluid, TickPriority priority) implements Write {}

        record PostProcess(BlockPos pos) implements Write {}
    }

    /** The session's writes since the last turn; `take` sends and clears them. */
    public static final class WriteLog {
        private final Session session;
        private final List<Write> writes = new ArrayList<>();

        WriteLog(Session session) {
            this.session = session;
        }

        void record(Write write) {
            if (!this.session.applying) this.writes.add(write);
        }

        public int size() {
            return this.writes.size();
        }

        public byte[] take() {
            var out = new FriendlyByteBuf(Unpooled.buffer());
            out.writeInt(this.writes.size());
            for (Write write : this.writes) {
                switch (write) {
                    case Write.Block b -> {
                        out.writeByte(0);
                        out.writeLong(b.pos().asLong());
                        out.writeInt(Flat.raw(b.state()));
                        out.writeInt(b.flags());
                    }
                    case Write.BlockEntitySet s -> {
                        out.writeByte(1);
                        out.writeLong(s.pos().asLong());
                        tag(out, withoutPosition(s.entity() != null ? s.entity().saveWithFullMetadata(this.session.registries) : s.tag()));
                    }
                    case Write.BlockEntityRemoved r -> {
                        out.writeByte(2);
                        out.writeLong(r.pos().asLong());
                    }
                    case Write.EntityAdded e -> {
                        out.writeByte(3);
                        tag(out, e.tag());
                    }
                    case Write.BlockTick t -> {
                        out.writeByte(4);
                        out.writeLong(t.pos().asLong());
                        out.writeInt(Flat.raw(t.block()));
                        out.writeInt(0);
                        out.writeByte(t.priority().getValue());
                    }
                    case Write.FluidTick t -> {
                        out.writeByte(5);
                        out.writeLong(t.pos().asLong());
                        out.writeInt(BuiltInRegistries.FLUID.getId(t.fluid()));
                        out.writeInt(0);
                        out.writeByte(t.priority().getValue());
                    }
                    case Write.PostProcess p -> {
                        out.writeByte(6);
                        out.writeLong(p.pos().asLong());
                    }
                }
            }
            this.writes.clear();
            return ByteBufUtil.getBytes(out);
        }

        /** Native's writes on the chunks holding their positions, without logging them back. */
        public void apply(byte[] bytes, Function<ChunkPos, ChunkAccess> chunks) {
            var in = new FriendlyByteBuf(Unpooled.wrappedBuffer(bytes));
            boolean was = this.session.applying;
            this.session.applying = true;
            try {
                for (int n = in.readInt(); n > 0; n--) {
                    int kind = in.readByte();
                    if (kind == 3) {
                        var tag = tag(in);
                        var at = tag.getList("Pos").map(list -> BlockPos.containing(list.getDoubleOr(0, 0), list.getDoubleOr(1, 0), list.getDoubleOr(2, 0)));
                        if (at.isEmpty()) continue;
                        if (!(chunk(chunks, at.get()) instanceof ProtoChunk proto)) throw new IllegalArgumentException("Horizon: a mirrored entity outside a proto chunk at " + at.get());
                        proto.addEntity(tag);
                        continue;
                    }
                    var at = BlockPos.of(in.readLong());
                    ChunkAccess chunk = chunk(chunks, at);
                    switch (kind) {
                        case 0 -> {
                            BlockState state = Block.BLOCK_STATE_REGISTRY.byIdOrThrow(in.readInt());
                            chunk.setBlockState(at, state, in.readInt());
                        }
                        case 1 -> {
                            var tag = tag(in);
                            tag.putInt("x", at.getX());
                            tag.putInt("y", at.getY());
                            tag.putInt("z", at.getZ());
                            chunk.removeBlockEntity(at);
                            chunk.setBlockEntityNbt(tag);
                        }
                        case 2 -> chunk.removeBlockEntity(at);
                        // Proto ticks are scheduled with no delay, as `ProtoChunkTicks.schedule` does.
                        case 4 -> chunk.getBlockTicks().schedule(new ScheduledTick<>(Block.BLOCK_STATE_REGISTRY.byIdOrThrow(in.readInt()).getBlock(), at, in.readInt(), TickPriority.byValue(in.readByte()), 0));
                        case 5 -> chunk.getFluidTicks().schedule(new ScheduledTick<>(BuiltInRegistries.FLUID.byIdOrThrow(in.readInt()), at, in.readInt(), TickPriority.byValue(in.readByte()), 0));
                        case 6 -> chunk.markPosForPostProcessing(at);
                        default -> throw new IllegalArgumentException("Horizon: mirror write " + kind);
                    }
                }
            } finally {
                this.session.applying = was;
            }
        }

        private static ChunkAccess chunk(Function<ChunkPos, ChunkAccess> chunks, BlockPos at) {
            ChunkAccess chunk = chunks.apply(ChunkPos.containing(at));
            if (chunk == null) throw new IllegalArgumentException("Horizon: a mirrored write outside the region at " + at);
            return chunk;
        }
    }

    /** A region session's chunk: every write lands here, whether through `WorldGenRegion` or `getChunk(pos)`, so each is logged here. */
    public static final class MirrorProtoChunk extends ProtoChunk {
        private final Session session;
        private final TickContainerAccess<net.minecraft.world.level.block.Block> blockTicks;
        private final TickContainerAccess<Fluid> fluidTicks;

        MirrorProtoChunk(ChunkPos pos, LevelChunkSection @Nullable [] sections, ProtoChunkTicks<net.minecraft.world.level.block.Block> blockTicks, ProtoChunkTicks<Fluid> fluidTicks, LevelHeightAccessor height, PalettedContainerFactory factory, Session session) {
            super(pos, UpgradeData.EMPTY, sections, blockTicks, fluidTicks, height, factory, null);
            this.session = session;
            this.blockTicks = new Logged<>(blockTicks, tick -> new Write.BlockTick(tick.pos(), tick.type(), tick.priority()));
            this.fluidTicks = new Logged<>(fluidTicks, tick -> new Write.FluidTick(tick.pos(), tick.type(), tick.priority()));
        }

        @Override
        public @Nullable BlockState setBlockState(BlockPos pos, BlockState state, int flags) {
            BlockState old = super.setBlockState(pos, state, flags);
            if (old != state && !this.isOutsideBuildHeight(pos.getY())) this.session.log.record(new Write.Block(pos.immutable(), state, flags));
            return old;
        }

        @Override
        public void setBlockEntity(BlockEntity entity) {
            super.setBlockEntity(entity);
            this.session.log.record(new Write.BlockEntitySet(entity.getBlockPos().immutable(), entity, null));
        }

        @Override
        public void setBlockEntityNbt(CompoundTag tag) {
            BlockPos at = BlockEntity.getPosFromTag(this.getPos(), tag);
            boolean live = this.getBlockEntities().containsKey(at);
            super.setBlockEntityNbt(tag);
            if (!live) this.session.log.record(new Write.BlockEntitySet(at, null, tag.copy()));
        }

        @Override
        public void removeBlockEntity(BlockPos pos) {
            boolean had = this.getBlockEntities().containsKey(pos) || this.getBlockEntityNbts().containsKey(pos);
            super.removeBlockEntity(pos);
            if (had) this.session.log.record(new Write.BlockEntityRemoved(pos.immutable()));
        }

        // `addEntity(Entity)` saves the entity and comes here with its tag.
        @Override
        public void addEntity(CompoundTag tag) {
            super.addEntity(tag);
            this.session.log.record(new Write.EntityAdded(tag.copy()));
        }

        @Override
        public void markPosForPostProcessing(BlockPos pos) {
            super.markPosForPostProcessing(pos);
            if (this.isInsideBuildHeight(pos)) this.session.log.record(new Write.PostProcess(pos.immutable()));
        }

        @Override
        public TickContainerAccess<net.minecraft.world.level.block.Block> getBlockTicks() {
            return this.blockTicks;
        }

        @Override
        public TickContainerAccess<Fluid> getFluidTicks() {
            return this.fluidTicks;
        }

        /** Logs a tick only when the container took it: `ProtoChunkTicks` keeps one per type and position. */
        private final class Logged<T> implements TickContainerAccess<T> {
            private final ProtoChunkTicks<T> ticks;
            private final Function<ScheduledTick<T>, Write> write;

            Logged(ProtoChunkTicks<T> ticks, Function<ScheduledTick<T>, Write> write) {
                this.ticks = ticks;
                this.write = write;
            }

            @Override
            public void schedule(ScheduledTick<T> tick) {
                int before = this.ticks.count();
                this.ticks.schedule(tick);
                if (this.ticks.count() != before) MirrorProtoChunk.this.session.log.record(this.write.apply(tick));
            }

            @Override
            public boolean hasScheduledTick(BlockPos pos, T type) {
                return this.ticks.hasScheduledTick(pos, type);
            }

            @Override
            public int count() {
                return this.ticks.count();
            }
        }
    }

    private static CompoundTag withoutPosition(CompoundTag tag) {
        var copy = tag.copy();
        copy.remove("x");
        copy.remove("y");
        copy.remove("z");
        return copy;
    }

    private static <T> SavedTick<T> savedTick(FriendlyByteBuf in, java.util.function.IntFunction<T> type) {
        var at = BlockPos.of(in.readLong());
        T value = type.apply(in.readInt());
        int delay = in.readInt();
        return new SavedTick<>(value, at, delay, TickPriority.byValue(in.readByte()));
    }

    private static void tickOut(FriendlyByteBuf out, SavedTick<?> tick, int id) {
        out.writeLong(tick.pos().asLong());
        out.writeInt(id);
        out.writeInt(tick.delay());
        out.writeByte(tick.priority().getValue());
    }

    /** `PalettedContainer.read` of a biome container into palette-resolved values (Rust's indices, not Java's ids). */
    static int[] readValues(FriendlyByteBuf in, int entries, int globalBits) {
        int bits = in.readByte();
        int[] palette = null;
        int storageBits;
        if (bits == 0) {
            palette = new int[] {in.readVarInt()};
            storageBits = 0;
        } else if (bits <= 3) {
            palette = new int[in.readVarInt()];
            for (int i = 0; i < palette.length; i++) palette[i] = in.readVarInt();
            storageBits = bits;
        } else {
            storageBits = globalBits;
        }
        int[] values = new int[entries];
        if (storageBits == 0) {
            java.util.Arrays.fill(values, palette[0]);
            return values;
        }
        var storage = new SimpleBitStorage(storageBits, entries);
        long[] raw = storage.getRaw();
        for (int i = 0; i < raw.length; i++) raw[i] = in.readLong();
        for (int i = 0; i < entries; i++) values[i] = palette == null ? storage.get(i) : palette[storage.get(i)];
        return values;
    }

    /** The biome container as `PalettedContainer.write` would, palette in first-occurrence order. */
    static void writeValues(FriendlyByteBuf out, int[] values, int globalBits) {
        var palette = new ArrayList<Integer>();
        var ids = new int[values.length];
        for (int i = 0; i < values.length; i++) {
            int at = palette.indexOf(values[i]);
            if (at < 0) {
                at = palette.size();
                palette.add(values[i]);
            }
            ids[i] = at;
        }
        int bits = Mth.ceillog2(palette.size());
        if (bits == 0) {
            out.writeByte(0);
            out.writeVarInt(palette.get(0));
            return;
        }
        boolean global = bits > 3;
        int storageBits = global ? globalBits : bits;
        out.writeByte(storageBits);
        if (!global) {
            out.writeVarInt(palette.size());
            palette.forEach(out::writeVarInt);
        }
        var storage = new SimpleBitStorage(storageBits, values.length);
        for (int i = 0; i < values.length; i++) storage.set(i, global ? values[i] : ids[i]);
        for (long word : storage.getRaw()) out.writeLong(word);
    }

    private static String text(FriendlyByteBuf in) {
        byte[] bytes = new byte[in.readInt()];
        in.readBytes(bytes);
        return new String(bytes, StandardCharsets.UTF_8);
    }

    private static void text(FriendlyByteBuf out, String value) {
        byte[] bytes = value.getBytes(StandardCharsets.UTF_8);
        out.writeInt(bytes.length);
        out.writeBytes(bytes);
    }

    private static CompoundTag tag(FriendlyByteBuf in) {
        byte[] bytes = new byte[in.readInt()];
        in.readBytes(bytes);
        try {
            return NbtIo.read(new DataInputStream(new ByteArrayInputStream(bytes)));
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    private static void tag(FriendlyByteBuf out, CompoundTag tag) {
        try {
            var bytes = new ByteArrayOutputStream();
            NbtIo.write(tag, new DataOutputStream(bytes));
            out.writeInt(bytes.size());
            out.writeBytes(bytes.toByteArray());
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }
}
