package dev.horizonlab.javamods.boot;

import com.google.common.collect.ImmutableList;
import dev.horizonlab.javamods.boot.mixin.AbstractArrowAccess;
import dev.horizonlab.javamods.boot.mixin.EntityAccess;
import dev.horizonlab.javamods.boot.mixin.EntityDataAccess;
import dev.horizonlab.javamods.boot.mixin.ItemAccess;
import dev.horizonlab.javamods.boot.mixin.ItemDataAccess;
import dev.horizonlab.javamods.boot.mixin.LivingAccess;
import dev.horizonlab.javamods.boot.mixin.MobAccess;
import dev.horizonlab.javamods.boot.mixin.LightningBoltAccess;
import dev.horizonlab.javamods.boot.mixin.LivingDataAccess;
import dev.horizonlab.javamods.boot.mixin.PrimedTntAccess;
import dev.horizonlab.javamods.boot.mixin.ProjectileAccess;
import dev.horizonlab.javamods.boot.mixin.SheepDataAccess;
import dev.horizonlab.javamods.boot.mixin.SynchedAccess;
import it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import java.util.Optional;
import java.util.UUID;
import java.util.function.Consumer;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.network.syncher.EntityDataAccessor;
import net.minecraft.resources.Identifier;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.AbortableIterationConsumer;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.effect.MobEffectInstance;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityReference;
import net.minecraft.world.entity.EntitySpawnReason;
import net.minecraft.world.entity.EntitySpawnRequest;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.LightningBolt;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.item.PrimedTnt;
import net.minecraft.world.level.pathfinder.PathType;
import net.minecraft.world.entity.projectile.Projectile;
import net.minecraft.world.entity.projectile.arrow.AbstractArrow;
import net.minecraft.world.entity.animal.sheep.Sheep;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.entity.EntityTypeTest;
import net.minecraft.world.level.entity.LevelEntityGetter;
import net.minecraft.world.level.storage.TagValueInput;
import net.minecraft.world.level.storage.TagValueOutput;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;

/** One live Java object per native entity from its join to its leave: refreshed raw where native says, and written through by bridges. */
public final class EntityViews implements LevelEntityGetter<Entity> {
    private static final int POSITION = 2, MOTION = 4, HEALTH = 8, EFFECTS = 16, EQUIPMENT = 32, FIRE = 64, AIR = 128, FLAGS = 256, NAME = 512, ITEM = 1024, RIDING = 2048, REMOVAL = 4096, SPECIES = 8192, AI = 32768;
    private static final Map<ServerLevel, Int2ObjectOpenHashMap<Entity>> LIVE = new IdentityHashMap<>();
    private static Entity spawning;
    private static boolean unknownLogged;

    private final ServerLevel level;

    public EntityViews(ServerLevel level) {
        this.level = level;
    }

    /** Bridged: entities with positions in the box, as count then id, known, saved form when not known, mask and group bytes. */
    private static byte[] query(double x1, double y1, double z1, double x2, double y2, double z2) {
        throw new IllegalStateException("not bridged");
    }

    /** Bridged: one entity by Horizon's id, the same way. */
    private static byte[] queryId(int id) {
        throw new IllegalStateException("not bridged");
    }

    /** Bridged: a saved entity into the level, with the species group its saved form lacks; its id, 0 when refused. */
    private static int spawn(byte[] saved, byte[] species) {
        throw new IllegalStateException("not bridged");
    }

    /** The bridge guard: whether the view is Horizon's own entity. */
    public static boolean backed(Object o) {
        return o instanceof Backed b && b.horizon$backed();
    }

    private static Int2ObjectOpenHashMap<Entity> live(ServerLevel level) {
        return LIVE.computeIfAbsent(level, l -> new Int2ObjectOpenHashMap<>());
    }

    /** The live view with this id in any level, for a bridge that answers with an entity. */
    static Entity byId(int id) {
        for (var views : LIVE.values()) {
            Entity found = views.get(id);
            if (found != null) return found;
        }
        return null;
    }

    private static CompoundTag decode(byte[] nbt) throws IOException {
        return NbtIo.read(new DataInputStream(new ByteArrayInputStream(nbt)), NbtAccounter.unlimitedHeap());
    }

    private static Entity create(ServerLevel level, int id, CompoundTag tag) {
        var input = TagValueInput.create(ProblemReporter.DISCARDING, level.registryAccess(), tag);
        Entity entity = EntityType.create(input, level, new EntitySpawnRequest(EntitySpawnReason.LOAD, true)).orElse(null);
        if (entity == null) {
            System.err.println("Horizon: no Java entity for a saved form: " + tag);
            return null;
        }
        entity.setId(id);
        ((Backed) entity).horizon$backed(true);
        live(level).put(id, entity);
        return entity;
    }

    /** `entity_view`: native made the entity `id`; Java's object for it exists from here to its leave. */
    static void join(ServerLevel level, int id, byte[] tag) throws IOException {
        Entity adopted = spawning;
        if (adopted != null) {
            spawning = null;
            adopted.setId(id);
            ((Backed) adopted).horizon$backed(true);
            live(level).put(id, adopted);
            return;
        }
        create(level, id, decode(tag));
    }

    /** `entity_left`: the view goes with its removal reason set raw; it stays backed, so a mod keeping it runs no vanilla body. */
    static void left(ServerLevel level, int id, int reason) {
        Entity view = live(level).remove(id);
        if (view != null) ((EntityAccess) view).horizon$removal(Entity.RemovalReason.values()[reason]);
    }

    /** Bridged: the groups bridges wrote since the last settle, as count then level, id, mask and group bytes; empty for none. */
    private static byte[] settled() {
        throw new IllegalStateException("not bridged");
    }

    /** Every backed view's bridge ends here: what native wrote reaches the views raw, with no request back to native. */
    public static void settle() {
        byte[] pending = settled();
        if (pending.length == 0) return;
        try {
            var in = new DataInputStream(new ByteArrayInputStream(pending));
            for (int n = in.readInt(); n > 0; n--) {
                ServerLevel level = GameBridge.level(text(in));
                int id = in.readInt(), mask = in.readInt();
                byte[] groups = in.readNBytes(in.readInt());
                Entity view = live(level).get(id);
                if (view != null) apply(view, mask, new DataInputStream(new ByteArrayInputStream(groups)));
            }
        } catch (IOException | ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    /** `Entity.save` of a view the spawning mod built, for native to take as a new entity. */
    static boolean spawn(Entity entity) {
        var output = TagValueOutput.createWithContext(ProblemReporter.DISCARDING, entity.level().registryAccess());
        // `Entity.save` refuses types that don't serialize (a lightning bolt); a spawn carries them anyway.
        output.putString("id", EntityType.getKey(entity.getType()).toString());
        entity.saveWithoutId(output);
        var bytes = new ByteArrayOutputStream();
        try {
            NbtIo.write(output.buildResult(), new DataOutputStream(bytes));
        } catch (IOException e) {
            throw new java.io.UncheckedIOException(e);
        }
        var species = new ByteArrayOutputStream();
        try {
            write(entity, SPECIES, new DataOutputStream(species));
        } catch (IOException e) {
            throw new java.io.UncheckedIOException(e);
        }
        spawning = entity;
        try {
            return spawn(bytes.toByteArray(), species.toByteArray()) != 0;
        } finally {
            spawning = null;
        }
    }

    private static <T> void data(Entity entity, EntityDataAccessor<T> accessor, T value) {
        ((SynchedAccess) entity.getEntityData()).<T>horizon$item(accessor).setValue(value);
    }

    private static ItemStack stack(DataInputStream in, ServerLevel level) throws IOException {
        byte[] nbt = in.readNBytes(in.readInt());
        if (nbt.length == 0) return ItemStack.EMPTY;
        Tag tag = decode(nbt);
        return ItemStack.CODEC.parse(level.registryAccess().createSerializationContext(NbtOps.INSTANCE), tag).getOrThrow();
    }

    private static String text(DataInputStream in) throws IOException {
        return new String(in.readNBytes(in.readInt()), java.nio.charset.StandardCharsets.UTF_8);
    }

    private static Entity viewById(Entity beside, int id) {
        if (id == 0) return null;
        Entity found = live((ServerLevel) beside.level()).get(id);
        if (found != null) return found;
        for (Entity player : GameBridge.playersIn((ServerLevel) beside.level())) if (player.getId() == id) return player;
        return null;
    }

    /** The raw write of group bytes, in bit order; no setter runs, so no vanilla logic does either. */
    static void apply(Entity entity, int mask, DataInputStream in) throws IOException {
        var access = (EntityAccess) entity;
        var level = (ServerLevel) entity.level();
        var living = entity instanceof LivingEntity l ? l : null;
        if ((mask & POSITION) != 0) {
            double x = in.readDouble(), y = in.readDouble(), z = in.readDouble();
            float yRot = in.readFloat(), xRot = in.readFloat(), head = in.readFloat(), body = in.readFloat();
            entity.setPosRaw(x, y, z);
            entity.xo = x;
            entity.yo = y;
            entity.zo = z;
            entity.xOld = x;
            entity.yOld = y;
            entity.zOld = z;
            access.horizon$yRot(yRot);
            access.horizon$xRot(xRot);
            entity.yRotO = yRot;
            entity.xRotO = xRot;
            if (living != null) {
                living.yHeadRot = head;
                living.yHeadRotO = head;
                living.yBodyRot = body;
                living.yBodyRotO = body;
            }
        }
        if ((mask & MOTION) != 0) {
            access.horizon$motion(new Vec3(in.readDouble(), in.readDouble(), in.readDouble()));
            access.horizon$onGround(in.readBoolean());
        }
        if ((mask & HEALTH) != 0 && in.readBoolean()) {
            float health = in.readFloat(), absorption = in.readFloat();
            short hurt = in.readShort(), death = in.readShort();
            if (living != null) {
                data(entity, LivingDataAccess.horizon$health(), health);
                ((LivingAccess) living).horizon$absorption(absorption);
                living.hurtTime = hurt;
                living.deathTime = death;
            }
        }
        if ((mask & EFFECTS) != 0) {
            var effects = living == null ? null : ((LivingAccess) living).horizon$effects();
            if (effects != null) effects.clear();
            for (int n = in.readInt(); n > 0; n--) {
                var id = Identifier.parse(text(in));
                int amplifier = in.readInt(), duration = in.readInt();
                boolean ambient = in.readBoolean(), visible = in.readBoolean(), icon = in.readBoolean();
                Holder<net.minecraft.world.effect.MobEffect> effect = BuiltInRegistries.MOB_EFFECT.get(id).orElse(null);
                if (effects != null && effect != null) effects.put(effect, new MobEffectInstance(effect, duration, amplifier, ambient, visible, icon));
            }
        }
        if ((mask & EQUIPMENT) != 0) {
            var equipment = living == null ? null : ((LivingAccess) living).horizon$equipment();
            var seen = new boolean[EquipmentSlot.values().length];
            for (int n = in.readInt(); n > 0; n--) {
                int slot = in.readByte();
                seen[slot] = true;
                ItemStack stack = stack(in, level);
                if (equipment != null) equipment.set(EquipmentSlot.values()[slot], stack);
            }
            if (equipment != null) for (EquipmentSlot slot : EquipmentSlot.values()) if (!seen[slot.ordinal()]) equipment.set(slot, ItemStack.EMPTY);
        }
        if ((mask & FIRE) != 0) access.horizon$fire(in.readInt());
        if ((mask & AIR) != 0) data(entity, EntityDataAccess.horizon$air(), in.readInt());
        if ((mask & FLAGS) != 0) {
            int flags = in.readInt();
            access.horizon$onGround((flags & 1) != 0);
            int shared = entity.getEntityData().get(EntityDataAccess.horizon$flags()) & 0b10000101;
            if ((flags & 2) != 0) shared |= 1 << 1;
            if ((flags & 4) != 0) shared |= 1 << 3;
            if ((flags & 8) != 0) shared |= 1 << 4;
            if ((flags & 64) != 0) shared |= 1 << 5;
            if ((flags & 32) != 0) shared |= 1 << 6;
            data(entity, EntityDataAccess.horizon$flags(), (byte) shared);
            data(entity, EntityDataAccess.horizon$silent(), (flags & 16) != 0);
            data(entity, EntityDataAccess.horizon$noGravity(), (flags & 128) != 0);
            access.horizon$inWater((flags & 256) != 0);
            if (entity instanceof Sheep) {
                byte wool = entity.getEntityData().get(SheepDataAccess.horizon$wool());
                data(entity, SheepDataAccess.horizon$wool(), (byte) (wool & 240 | (flags >> 16) & 15));
            }
        }
        if ((mask & NAME) != 0) {
            Component name = null;
            if (in.readBoolean()) {
                Tag tag = decode(in.readNBytes(in.readInt())).get("v");
                name = ComponentSerialization.CODEC.parse(level.registryAccess().createSerializationContext(NbtOps.INSTANCE), tag).getOrThrow();
            }
            data(entity, EntityDataAccess.horizon$name(), Optional.ofNullable(name));
            entity.entityTags().clear();
            for (int n = in.readInt(); n > 0; n--) entity.entityTags().add(text(in));
        }
        if ((mask & ITEM) != 0 && in.readBoolean()) {
            ItemStack held = stack(in, level);
            short delay = in.readShort();
            int age = in.readInt();
            UUID thrower = in.readBoolean() ? new UUID(in.readLong(), in.readLong()) : null;
            UUID target = in.readBoolean() ? new UUID(in.readLong(), in.readLong()) : null;
            if (entity instanceof ItemEntity item) {
                data(entity, ItemDataAccess.horizon$item(), held);
                var items = (ItemAccess) item;
                items.horizon$pickupDelay(delay);
                items.horizon$age(age);
                items.horizon$thrower(thrower == null ? null : EntityReference.of(thrower));
                items.horizon$target(target);
            }
        }
        if ((mask & RIDING) != 0) {
            access.horizon$vehicle(viewById(entity, in.readInt()));
            var passengers = ImmutableList.<Entity>builder();
            for (int n = in.readInt(); n > 0; n--) {
                Entity passenger = viewById(entity, in.readInt());
                if (passenger != null) passengers.add(passenger);
            }
            access.horizon$passengers(passengers.build());
        }
        if ((mask & REMOVAL) != 0) {
            int reason = in.readByte();
            access.horizon$removal(reason == 0 ? null : Entity.RemovalReason.values()[reason - 1]);
        }
        if ((mask & SPECIES) != 0) {
            if (entity instanceof PrimedTnt tnt) {
                int fuse = in.readInt(), state = in.readInt();
                float power = in.readFloat();
                UUID owner = in.readBoolean() ? new UUID(in.readLong(), in.readLong()) : null;
                data(entity, PrimedTntAccess.horizon$fuse(), fuse);
                data(entity, PrimedTntAccess.horizon$state(), Flat.toBlockState(state));
                var tnts = (PrimedTntAccess) tnt;
                tnts.horizon$power(power);
                tnts.horizon$owner(owner == null ? null : EntityReference.<LivingEntity>of(owner));
            } else if (entity instanceof LightningBolt bolt) {
                float damage = in.readFloat();
                boolean visualOnly = in.readBoolean();
                int life = in.readInt(), flashes = in.readInt();
                long seed = in.readLong();
                var bolts = (LightningBoltAccess) bolt;
                bolts.horizon$damage(damage);
                bolts.horizon$visualOnly(visualOnly);
                bolts.horizon$life(life);
                bolts.horizon$flashes(flashes);
                bolt.seed = seed;
            } else if (entity instanceof Projectile projectile) {
                if (projectile instanceof AbstractArrow arrow) ((AbstractArrowAccess) arrow).horizon$baseDamage(in.readDouble());
                UUID owner = in.readBoolean() ? new UUID(in.readLong(), in.readLong()) : null;
                ((ProjectileAccess) projectile).horizon$owner(owner == null ? null : EntityReference.of(owner));
            }
        }
        if ((mask & AI) != 0) {
            var malus = ((MobAccess) entity).horizon$pathfindingMalus();
            malus.clear();
            for (int n = in.readInt(); n > 0; n--) {
                var type = PathType.values()[in.readByte()];
                malus.put(type, in.readFloat());
            }
        }
    }

    /** The mirror of `apply` for the species group, as a spawned object carries what its saved form lacks. */
    static void write(Entity entity, int mask, DataOutputStream out) throws IOException {
        if ((mask & SPECIES) != 0 && entity instanceof LightningBolt bolt) {
            var bolts = (LightningBoltAccess) bolt;
            out.writeFloat(bolts.horizon$damage());
            out.writeBoolean(bolts.horizon$visualOnly());
            out.writeInt(bolts.horizon$life());
            out.writeInt(bolts.horizon$flashes());
            out.writeLong(bolt.seed);
        }
    }

    private List<Entity> read(byte[] listed) {
        var found = new ArrayList<Entity>();
        try {
            var in = new DataInputStream(new ByteArrayInputStream(listed));
            var views = live(level);
            for (int n = in.readInt(); n > 0; n--) {
                int id = in.readInt();
                boolean known = in.readBoolean();
                byte[] nbt = known ? null : in.readNBytes(in.readInt());
                int mask = in.readInt();
                Entity entity = views.get(id);
                if (entity == null) {
                    if (!unknownLogged) {
                        unknownLogged = true;
                        System.err.println("Horizon: an entity without a Java view: " + id);
                    }
                    if (known) continue;
                    entity = create(level, id, decode(nbt));
                    if (entity == null) continue;
                }
                apply(entity, mask, in);
                if (!entity.isRemoved()) found.add(entity);
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
        return found;
    }

    @Override
    public Entity get(int id) {
        for (Entity player : GameBridge.playersIn(level)) if (player.getId() == id) return player;
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

    /** `entities_save`: the Java-owned keys of each listed view, which native writes into the entity's saved form. */
    static void save(ServerLevel level, DataInputStream in, DataOutputStream out) throws IOException {
        int n = in.readInt();
        var ids = new int[n];
        for (int i = 0; i < n; i++) ids[i] = in.readInt();
        out.writeInt(n);
        for (int id : ids) {
            out.writeInt(id);
            Entity view = live(level).get(id);
            var keys = new CompoundTag();
            if (view != null) {
                if (!view.getPersistentData().isEmpty()) keys.put("NeoForgeData", view.getPersistentData().copy());
                var output = TagValueOutput.createWithContext(ProblemReporter.DISCARDING, level.registryAccess());
                view.serializeAttachments(output);
                keys.merge(output.buildResult());
            }
            if (keys.isEmpty()) {
                out.writeInt(0);
            } else {
                GameBridge.writeTag(out, keys);
            }
        }
    }

    /** A world closed: its views go with it. */
    static void reset() {
        LIVE.clear();
        spawning = null;
    }
}
