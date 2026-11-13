package dev.horizonlab.javamods.boot;

import dev.horizonlab.javamods.boot.mixin.DebugOverlayAccess;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collection;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Objects;
import java.util.TreeMap;
import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.components.debug.DebugScreenDisplayer;
import net.minecraft.client.gui.components.debug.DebugScreenEntries;
import net.minecraft.client.gui.components.debug.DebugScreenEntry;
import net.minecraft.client.gui.components.debug.DebugScreenEntryStatus;
import net.minecraft.client.gui.components.debug.DebugScreenProfile;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.resources.Identifier;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.chunk.LevelChunk;

/** The F3 entries mods registered: exported once for native's debug screen, and their lines recorded each tick for the ids native has enabled. */
final class DebugLines {
    private DebugLines() {}

    /** One entry's displayer calls in order, as native replays them. */
    private static final class Recorded implements DebugScreenDisplayer {
        final List<String> priority = new ArrayList<>();
        final List<String> lines = new ArrayList<>();
        final Map<String, List<String>> groups = new LinkedHashMap<>();

        @Override
        public void addPriorityLine(String line) {
            priority.add(line);
        }

        @Override
        public void addLine(String line) {
            lines.add(line);
        }

        @Override
        public void addToGroup(Identifier group, Collection<String> added) {
            groups.computeIfAbsent(group.toString(), key -> new ArrayList<>()).addAll(added);
        }

        @Override
        public void addToGroup(Identifier group, String line) {
            groups.computeIfAbsent(group.toString(), key -> new ArrayList<>()).add(line);
        }
    }

    private static volatile boolean reduced;
    private static volatile List<Identifier> wanted = List.of();
    private static Map<Identifier, Recorded> recorded = Map.of();
    private static boolean gapReported;

    private static void text(DataOutputStream out, String value) throws IOException {
        byte[] bytes = value.getBytes(StandardCharsets.UTF_8);
        out.writeInt(bytes.length);
        out.write(bytes);
    }

    /** 0 absent from the profile, then 1 `ALWAYS_ON`, 2 `IN_OVERLAY`, 3 `NEVER`. */
    private static byte status(Map<Identifier, DebugScreenEntryStatus> profile, Identifier id) {
        DebugScreenEntryStatus status = profile == null ? null : profile.get(id);
        return status == null ? 0 : (byte) (status.ordinal() + 1);
    }

    /** `client_debug_entries`: every entry a mod registered, in `Identifier` order, with its category, reduced-info rule and profile statuses. */
    static byte[] export() throws IOException {
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        var mods = new TreeMap<Identifier, DebugScreenEntry>();
        DebugScreenEntries.allEntries().forEach((id, entry) -> {
            if (!id.getNamespace().equals("minecraft")) mods.put(id, entry);
        });
        out.writeInt(mods.size());
        var profiles = DebugScreenEntries.PROFILES;
        for (var entry : mods.entrySet()) {
            var id = entry.getKey();
            var category = entry.getValue().category();
            text(out, id.toString());
            text(out, ComponentSerialization.CODEC.encodeStart(com.mojang.serialization.JsonOps.INSTANCE, category.label()).getOrThrow().toString());
            out.writeFloat(category.sortKey());
            out.writeBoolean(entry.getValue().isAllowed(true));
            out.writeByte(status(profiles.get(DebugScreenProfile.DEFAULT), id));
            out.writeByte(status(profiles.get(DebugScreenProfile.PERFORMANCE), id));
        }
        if (System.getenv("HORIZON_JAVA_DEBUG") != null) System.err.println("Horizon: F3 entries from mods: " + mods.keySet());
        return bytes.toByteArray();
    }

    /** The `DebugLines` input: whether debug info is reduced, then the ids native has enabled. */
    static void wanted(DataInputStream in) throws IOException {
        boolean reducedNow = in.readBoolean();
        var ids = new ArrayList<Identifier>();
        for (int n = in.readInt(); n > 0; n--) {
            byte[] utf = in.readNBytes(in.readInt());
            ids.add(Identifier.parse(new String(utf, StandardCharsets.UTF_8)));
        }
        reduced = reducedNow;
        wanted = ids;
        if (ids.isEmpty()) recorded = Map.of();
    }

    /** After `minecraft.tick()`: each wanted entry's `display`, with the overlay's own level and chunk views. */
    static void record(Minecraft minecraft) {
        var ids = wanted;
        if (ids.isEmpty()) return;
        var overlay = (DebugOverlayAccess) minecraft.getDebugOverlay();
        var camera = minecraft.getCameraEntity();
        ChunkPos chunk = camera != null && minecraft.level != null ? ChunkPos.containing(camera.blockPosition()) : null;
        if (!Objects.equals(overlay.horizon$lastPos(), chunk)) {
            overlay.horizon$lastPos(chunk);
            minecraft.getDebugOverlay().clearChunkCache();
        }
        var level = overlay.horizon$level();
        var clientChunk = overlay.horizon$clientChunk();
        LevelChunk serverChunk = null;
        try {
            serverChunk = overlay.horizon$serverChunk();
            var server = minecraft.getSingleplayerServer();
            if (serverChunk == null && chunk != null && server != null && server.getLevel(minecraft.level.dimension()) != null) {
                reportGap("the server chunk " + chunk + " is not in the Java server view");
            }
        } catch (Throwable t) {
            reportGap("the server chunk of " + chunk + " failed: " + t);
        }
        var now = new LinkedHashMap<Identifier, Recorded>();
        for (Identifier id : ids) {
            var entry = DebugScreenEntries.getEntry(id);
            if (entry == null || !entry.isAllowed(reduced)) continue;
            var displayer = new Recorded();
            var view = serverChunk;
            GameBridge.safely("DebugScreenEntry " + id, () -> entry.display(displayer, level, clientChunk, view));
            now.put(id, displayer);
        }
        recorded = now;
    }

    private static void reportGap(String what) {
        if (gapReported) return;
        gapReported = true;
        System.err.println("Horizon: F3 entries of mods get no server chunk: " + what);
    }

    /** The status block `debugLines`: for each wanted id, its priority lines, lines and groups as recorded. */
    static void write(DataOutputStream out) throws IOException {
        var ids = wanted;
        out.writeInt(ids.size());
        for (Identifier id : ids) {
            var entry = recorded.get(id);
            text(out, id.toString());
            var priority = entry == null ? List.<String>of() : entry.priority;
            out.writeInt(priority.size());
            for (String line : priority) text(out, line);
            var lines = entry == null ? List.<String>of() : entry.lines;
            out.writeInt(lines.size());
            for (String line : lines) text(out, line);
            var groups = entry == null ? Map.<String, List<String>>of() : entry.groups;
            out.writeInt(groups.size());
            for (var group : groups.entrySet()) {
                text(out, group.getKey());
                out.writeInt(group.getValue().size());
                for (String line : group.getValue()) text(out, line);
            }
        }
    }
}
