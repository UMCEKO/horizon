package dev.horizonlab.javamods.boot;

import it.unimi.dsi.fastutil.longs.Long2ObjectLinkedOpenHashMap;
import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.CustomSpawner;
import net.minecraft.world.level.chunk.LevelChunk;
import net.minecraft.world.level.chunk.storage.SerializableChunkData;
import net.neoforged.neoforge.attachment.AttachmentHolder;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.EventHooks;
import net.neoforged.neoforge.event.level.ChunkDataEvent;
import net.neoforged.neoforge.event.level.ChunkEvent;
import net.neoforged.neoforge.event.level.LevelEvent;

/** `ChunkMap`'s chunk events for the columns Horizon loads, saves and unloads, with the chunk data a mod reads and writes through them. */
final class ChunkEvents {
    private ChunkEvents() {}

    static final int DATA_LOAD = 0, LOAD = 1, UNLOAD = 2, SAVE = 3, UNSAVED = 4, LEVEL_SAVE = 5, UNLOADED = 6;

    // Views of columns leaving the level: their attachments are saved after Horizon has let the column go.
    private static final Map<ServerLevel, Long2ObjectLinkedOpenHashMap<LevelChunk>> LEAVING = new IdentityHashMap<>();

    static void reset() {
        LEAVING.clear();
    }

    static void run(ServerLevel level, DataInputStream in, DataOutputStream out) throws IOException {
        switch (in.readInt()) {
            case DATA_LOAD -> dataLoad(level, in);
            case LOAD -> load(level, in);
            case UNLOAD -> unload(level, in, out);
            case SAVE -> save(level, in, out);
            case UNSAVED -> unsaved(level, in, out);
            case UNLOADED -> unloaded(level);
            case LEVEL_SAVE -> post("LevelEvent.Save", new LevelEvent.Save(level));
            default -> throw new IllegalArgumentException("chunk event");
        }
    }

    private static void post(String what, net.neoforged.bus.api.Event event) {
        try {
            NeoForge.EVENT_BUS.post(event);
        } catch (Throwable t) {
            System.err.println("Horizon: a Java mod threw during " + what);
            t.printStackTrace();
        }
    }

    private static CompoundTag tag(DataInputStream in) throws IOException {
        return NbtIo.read(new DataInputStream(new ByteArrayInputStream(in.readNBytes(in.readInt()))), NbtAccounter.unlimitedHeap());
    }

    private static LevelChunk view(ServerLevel level, int x, int z) {
        var leaving = LEAVING.get(level);
        var view = leaving == null ? null : leaving.get(ChunkPos.pack(x, z));
        return view != null ? view : ChunkViews.get(level, x, z);
    }

    /** `SerializableChunkData.read` restoring the chunk's attachments, then `ChunkMap.scheduleChunkLoad`'s `ChunkDataEvent.Load`. */
    private static void dataLoad(ServerLevel level, DataInputStream in) throws IOException {
        for (int n = in.readInt(); n > 0; n--) {
            int x = in.readInt();
            int z = in.readInt();
            var data = SerializableChunkData.parse(level, level.palettedContainerFactory(), tag(in));
            var chunk = ChunkViews.get(level, x, z);
            if (data == null || chunk == null) continue;
            if (data.attachmentData() != null) chunk.readAttachmentsFromNBT(level.registryAccess(), data.attachmentData());
            post("ChunkDataEvent.Load", new ChunkDataEvent.Load(chunk, data));
        }
    }

    /** `ChunkStatusTasks.full`'s `ChunkEvent.Load`, once the column's block entities are in the level; new when it was generated rather than read. */
    private static void load(ServerLevel level, DataInputStream in) throws IOException {
        for (int n = in.readInt(); n > 0; n--) {
            int x = in.readInt();
            int z = in.readInt();
            boolean generated = in.readBoolean();
            var chunk = ChunkViews.get(level, x, z);
            if (chunk == null) continue;
            chunk.setLoaded(true);
            post("ChunkEvent.Load", new ChunkEvent.Load(chunk, generated));
        }
    }

    /** `ChunkMap.scheduleUnload`'s first step: the chunks stop being loaded; answers the columns a mod left unsaved, which are saved before `unloaded`. */
    private static void unload(ServerLevel level, DataInputStream in, DataOutputStream out) throws IOException {
        var leaving = new Long2ObjectLinkedOpenHashMap<LevelChunk>();
        LEAVING.put(level, leaving);
        var unsaved = new ArrayList<Long>();
        for (int n = in.readInt(); n > 0; n--) {
            int x = in.readInt();
            int z = in.readInt();
            var chunk = ChunkViews.existing(level, x, z);
            leaving.put(ChunkPos.pack(x, z), chunk);
            chunk.setLoaded(false);
            if (chunk.isUnsaved()) unsaved.add(ChunkPos.pack(x, z));
        }
        positions(out, unsaved);
    }

    /** `ChunkMap.scheduleUnload`'s `ChunkEvent.Unload`, which comes after the chunk's save. */
    private static void unloaded(ServerLevel level) {
        var leaving = LEAVING.remove(level);
        if (leaving != null) for (var chunk : leaving.values()) post("ChunkEvent.Unload", new ChunkEvent.Unload(chunk));
    }

    /** The loaded columns whose chunk a mod marked unsaved, which `ChunkMap.saveAllChunks` saves too. */
    private static void unsaved(ServerLevel level, DataInputStream in, DataOutputStream out) throws IOException {
        var unsaved = new ArrayList<Long>();
        for (int n = in.readInt(); n > 0; n--) {
            int x = in.readInt();
            int z = in.readInt();
            var chunk = ChunkViews.peek(level, x, z);
            if (chunk != null && chunk.isUnsaved()) unsaved.add(ChunkPos.pack(x, z));
        }
        positions(out, unsaved);
    }

    private static void positions(DataOutputStream out, List<Long> positions) throws IOException {
        out.writeInt(positions.size());
        for (long pos : positions) out.writeLong(pos);
    }

    /** `ChunkMap.save`: the chunk's data as `SerializableChunkData.copyOf` takes it, its attachments included, then `ChunkDataEvent.Save`; answers what `write` puts in the chunk's tag. */
    private static void save(ServerLevel level, DataInputStream in, DataOutputStream out) throws IOException {
        int count = in.readInt();
        out.writeInt(count);
        for (int n = 0; n < count; n++) {
            int x = in.readInt();
            int z = in.readInt();
            var tag = tag(in);
            var chunk = view(level, x, z);
            if (chunk != null) {
                chunk.tryMarkSaved();
                tag.remove(AttachmentHolder.ATTACHMENTS_NBT_KEY);
                if (chunk.hasAttachments()) tag.put(AttachmentHolder.ATTACHMENTS_NBT_KEY, chunk.writeAttachmentsToNBT(level.registryAccess()));
            }
            var data = SerializableChunkData.parse(level, level.palettedContainerFactory(), tag);
            if (data != null && chunk != null) post("ChunkDataEvent.Save", new ChunkDataEvent.Save(chunk, level, data));
            GameBridge.writeTag(out, data == null ? tag : data.write());
        }
    }

    /** `ServerLevel`'s constructor: the level's custom spawners, vanilla's for the overworld, after mods' `ModifyCustomSpawnersEvent`. */
    static void customSpawners(ServerLevel level) {
        List<CustomSpawner> spawners = level.dimension() == net.minecraft.world.level.Level.OVERWORLD
                ? List.of(new net.minecraft.world.level.levelgen.PhantomSpawner(), new net.minecraft.world.level.levelgen.PatrolSpawner(),
                        new net.minecraft.world.entity.npc.CatSpawner(), new net.minecraft.world.entity.ai.village.VillageSiege(),
                        new net.minecraft.world.entity.npc.wanderingtrader.WanderingTraderSpawner(level.getServer().getDataStorage()))
                : List.of();
        try {
            ServerView.set(ServerLevel.class, level, "customSpawners", EventHooks.getCustomSpawners(level, spawners));
        } catch (Throwable t) {
            System.err.println("Horizon: a Java mod threw during ModifyCustomSpawnersEvent");
            t.printStackTrace();
        }
    }
}
