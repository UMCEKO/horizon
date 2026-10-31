package dev.horizonlab.javamods.boot;

import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import java.util.function.Consumer;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.AbortableIterationConsumer;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntitySpawnReason;
import net.minecraft.world.entity.EntitySpawnRequest;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.level.entity.EntityTypeTest;
import net.minecraft.world.level.entity.LevelEntityGetter;
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.world.level.storage.TagValueOutput;
import net.minecraft.world.phys.AABB;

/** Horizon's entities as real Java entities: read from their saved form when a mod asks, and what the call changed written back. */
public final class EntityViews implements LevelEntityGetter<Entity> {
    private record View(Entity entity, CompoundTag before) {}

    private static final Map<Integer, View> VIEWS = new LinkedHashMap<>();

    private final ServerLevel level;

    public EntityViews(ServerLevel level) {
        this.level = level;
    }

    /** Bridged: entities with positions in the box, as count then id and saved form. */
    private static byte[] query(double x1, double y1, double z1, double x2, double y2, double z2) {
        throw new IllegalStateException("not bridged");
    }

    /** Bridged: one entity by Horizon's id, the same way. */
    private static byte[] queryId(int id) {
        throw new IllegalStateException("not bridged");
    }

    private static CompoundTag save(Entity entity) {
        var output = TagValueOutput.createWithContext(ProblemReporter.DISCARDING, entity.level().registryAccess());
        entity.saveWithoutId(output);
        return output.buildResult();
    }

    private List<Entity> read(byte[] listed) {
        var found = new ArrayList<Entity>();
        try {
            var in = new DataInputStream(new ByteArrayInputStream(listed));
            for (int n = in.readInt(); n > 0; n--) {
                int id = in.readInt();
                byte[] nbt = in.readNBytes(in.readInt());
                View view = VIEWS.get(id);
                if (view == null) {
                    CompoundTag tag = NbtIo.read(new DataInputStream(new ByteArrayInputStream(nbt)), NbtAccounter.unlimitedHeap());
                    var input = TagValueInput.create(ProblemReporter.DISCARDING, level.registryAccess(), tag);
                    Entity entity = EntityType.create(input, level, new EntitySpawnRequest(EntitySpawnReason.LOAD, true)).orElse(null);
                    if (entity == null) continue;
                    entity.setId(id);
                    view = new View(entity, save(entity));
                    VIEWS.put(id, view);
                }
                if (!view.entity.isRemoved()) found.add(view.entity);
            }
        } catch (IOException e) {
            throw new java.io.UncheckedIOException(e);
        }
        return found;
    }

    private List<Entity> within(AABB box) {
        // Horizon lists by position; a box touches entities whose position is just outside it.
        var found = read(query(box.minX - 2, box.minY - 4, box.minZ - 2, box.maxX + 2, box.maxY + 4, box.maxZ + 2));
        found.addAll(GameBridge.playersIn(level));
        found.addAll(GameBridge.spawned(level));
        return found;
    }

    @Override
    public Entity get(int id) {
        for (Entity player : GameBridge.playersIn(level)) if (player.getId() == id) return player;
        for (Entity entity : GameBridge.spawned(level)) if (entity.getId() == id) return entity;
        var found = read(queryId(id));
        return found.isEmpty() ? null : found.get(0);
    }

    @Override
    public Entity get(UUID id) {
        for (Entity entity : getAll()) if (entity.getUUID().equals(id)) return entity;
        return null;
    }

    @Override
    public Iterable<Entity> getAll() {
        return within(new AABB(-3.0E7, -2048, -3.0E7, 3.0E7, 2048, 3.0E7));
    }

    @Override
    public <U extends Entity> void get(EntityTypeTest<Entity, U> type, AbortableIterationConsumer<U> consumer) {
        for (Entity entity : getAll()) {
            U cast = type.tryCast(entity);
            if (cast != null && consumer.accept(cast).shouldAbort()) return;
        }
    }

    @Override
    public void get(AABB box, Consumer<Entity> output) {
        for (Entity entity : within(box)) if (entity.getBoundingBox().intersects(box)) output.accept(entity);
    }

    @Override
    public <U extends Entity> void get(EntityTypeTest<Entity, U> type, AABB box, AbortableIterationConsumer<U> consumer) {
        for (Entity entity : within(box)) {
            if (!entity.getBoundingBox().intersects(box)) continue;
            U cast = type.tryCast(entity);
            if (cast != null && consumer.accept(cast).shouldAbort()) return;
        }
    }

    /** At the end of a call: entities it changed and removed, then none held, since Horizon moves them between calls. */
    static void writeChanges(DataOutputStream out) throws IOException {
        var changed = new ArrayList<Map.Entry<Integer, CompoundTag>>();
        var removed = new ArrayList<Integer>();
        for (var entry : VIEWS.entrySet()) {
            View view = entry.getValue();
            if (view.entity.isRemoved()) {
                removed.add(entry.getKey());
            } else {
                CompoundTag now = save(view.entity);
                if (!now.equals(view.before)) changed.add(Map.entry(entry.getKey(), now));
            }
        }
        VIEWS.clear();
        out.writeInt(changed.size());
        for (var entry : changed) {
            out.writeInt(entry.getKey());
            GameBridge.writeTag(out, entry.getValue());
        }
        out.writeInt(removed.size());
        for (int id : removed) out.writeInt(id);
    }
}
