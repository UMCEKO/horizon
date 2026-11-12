package dev.horizonlab.javamods.boot;

import dev.horizonlab.javamods.boot.mixin.LivingAccess;
import dev.horizonlab.javamods.boot.mixin.SynchedAccess;
import io.netty.buffer.Unpooled;
import it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap;
import java.io.IOException;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import java.util.function.Predicate;
import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.protocol.game.ClientboundSetEntityDataPacket;
import net.minecraft.network.protocol.game.ClientboundSetEquipmentPacket;
import net.minecraft.network.syncher.SynchedEntityData;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntitySpawnReason;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.boss.enderdragon.EnderDragon;
import net.minecraft.world.level.entity.EntityTypeTest;
import net.minecraft.world.phys.AABB;
import net.neoforged.neoforge.network.connection.ConnectionType;

/** The entities of the Java client's level: views made from what the window applied of the server's entity packets, written raw; Java never ticks, moves or draws them. */
public final class ClientEntities {
    private ClientEntities() {}

    static final EntityFeed FEED = new EntityFeed();
    private static final Int2ObjectOpenHashMap<Entity> VIEWS = new Int2ObjectOpenHashMap<>();
    private static final Map<UUID, Entity> BY_UUID = new HashMap<>();

    /** The window's changes up to its play packet at `stamp`, before a packet the server sent after that one; they wait for the client's level. */
    static void applyUpTo(long stamp) throws IOException {
        if (Minecraft.getInstance().level == null) return;
        for (var change : FEED.upTo(stamp)) apply(change);
    }

    static void applyAll() throws IOException {
        applyUpTo(Long.MAX_VALUE);
    }

    /** The level changed: its views went with it. */
    static void levelChanged() {
        VIEWS.values().forEach(view -> view.setRemoved(Entity.RemovalReason.DISCARDED));
        VIEWS.clear();
        BY_UUID.clear();
    }

    /** The window left its world. */
    static void left() {
        levelChanged();
        FEED.clear();
    }

    private static void apply(EntityFeed.Change change) {
        ClientLevel level = Minecraft.getInstance().level;
        if (level == null) return;
        switch (change) {
            case EntityFeed.Add add -> add(level, add);
            case EntityFeed.Data data -> {
                Entity view = VIEWS.get(data.id());
                if (view != null) data(level, view, data.packet());
            }
            case EntityFeed.Equip equip -> {
                if (VIEWS.get(equip.id()) instanceof LivingEntity living) {
                    var packet = ClientboundSetEquipmentPacket.STREAM_CODEC.decode(buffer(level, equip.packet()));
                    for (var slot : packet.getSlots()) ((LivingAccess) living).horizon$equipment().set(slot.getFirst(), slot.getSecond());
                }
            }
            case EntityFeed.Passengers passengers -> passengers(passengers);
            case EntityFeed.Move move -> {
                Entity view = VIEWS.get(move.id());
                if (view == null) return;
                view.setOldPosAndRot();
                view.setPos(move.x(), move.y(), move.z());
                view.setYRot(move.yaw());
                view.setXRot(move.pitch());
                view.setYHeadRot(move.head());
                view.setDeltaMovement(move.dx(), move.dy(), move.dz());
                view.setOnGround(move.onGround());
            }
            // `handleEntityEvent` is client logic the window runs.
            case EntityFeed.Event event -> {}
            case EntityFeed.Remove remove -> {
                for (int id : remove.ids()) remove(id);
            }
        }
    }

    private static void add(ClientLevel level, EntityFeed.Add add) {
        remove(add.id());
        EntityType<?> type = net.minecraft.core.registries.BuiltInRegistries.ENTITY_TYPE.getOptional(net.minecraft.resources.Identifier.tryParse(add.type())).orElse(null);
        if (type == null) {
            System.err.println("Horizon: the Java client has no entity type " + add.type());
            return;
        }
        // Players are the client's player list's.
        Entity view = type.create(level, EntitySpawnReason.LOAD);
        if (view == null) return;
        view.setId(add.id());
        view.setUUID(add.uuid());
        view.setPos(add.x(), add.y(), add.z());
        view.setYRot(add.yaw());
        view.setXRot(add.pitch());
        view.setYHeadRot(add.head());
        view.setDeltaMovement(add.dx(), add.dy(), add.dz());
        view.setOldPosAndRot();
        VIEWS.put(add.id(), view);
        BY_UUID.put(add.uuid(), view);
    }

    private static RegistryFriendlyByteBuf buffer(ClientLevel level, byte[] packet) {
        return new RegistryFriendlyByteBuf(Unpooled.wrappedBuffer(packet), level.registryAccess(), ConnectionType.NEOFORGE);
    }

    /** Each value into its data item, without `onSyncedDataUpdated`: what vanilla's client does on an update the window already did. */
    private static void data(ClientLevel level, Entity view, byte[] packet) {
        var values = ClientboundSetEntityDataPacket.STREAM_CODEC.decode(buffer(level, packet)).packedItems();
        var items = ((SynchedAccess) view.getEntityData()).horizon$items();
        for (var value : values) {
            var item = value.id() < items.length ? items[value.id()] : null;
            if (item == null || !item.getAccessor().serializer().equals(value.serializer())) {
                System.err.println("Horizon: entity data " + value.id() + " does not fit " + EntityType.getKey(view.getType()));
                continue;
            }
            write(item, value.value());
        }
    }

    @SuppressWarnings("unchecked")
    private static <T> void write(SynchedEntityData.DataItem<T> item, Object value) {
        item.setValue((T) value);
    }

    /** `SetPassengers`: those listed ride the vehicle, those no longer listed got off. */
    private static void passengers(EntityFeed.Passengers passengers) {
        Entity vehicle = VIEWS.get(passengers.vehicle());
        if (vehicle == null) return;
        for (Entity rider : List.copyOf(vehicle.getPassengers())) {
            if (!java.util.Arrays.stream(passengers.riders()).anyMatch(id -> id == rider.getId())) rider.removeVehicle();
        }
        for (int id : passengers.riders()) {
            Entity rider = VIEWS.get(id);
            if (rider != null && rider.getVehicle() != vehicle) rider.startRiding(vehicle, true, false);
        }
    }

    private static void remove(int id) {
        Entity view = VIEWS.remove(id);
        if (view == null) return;
        BY_UUID.remove(view.getUUID(), view);
        view.setRemoved(Entity.RemovalReason.DISCARDED);
    }

    public static Entity get(int id) {
        return VIEWS.get(id);
    }

    public static Entity get(UUID uuid) {
        return BY_UUID.get(uuid);
    }

    public static Iterable<Entity> all() {
        return VIEWS.values();
    }

    public static int count() {
        return VIEWS.size();
    }

    /** `Level.getEntities(except, box, selector)` over the views, their dragon parts included. */
    public static void collect(Entity except, AABB box, Predicate<? super Entity> selector, List<Entity> out) {
        for (Entity view : VIEWS.values()) {
            if (view != except && view.getBoundingBox().intersects(box) && selector.test(view)) out.add(view);
            if (!(view instanceof EnderDragon dragon)) continue;
            for (var part : dragon.getSubEntities()) {
                if (part != except && part.parentMob != except && selector.test(part) && box.intersects(part.getBoundingBox())) out.add(part);
            }
        }
    }

    /** `Level.getEntities(type, box, selector, output, maxResults)` over the views; true once `output` holds `max`. */
    public static <T extends Entity> boolean collect(EntityTypeTest<Entity, T> type, AABB box, Predicate<? super T> selector, List<? super T> output, int max) {
        for (Entity view : VIEWS.values()) {
            if (!view.getBoundingBox().intersects(box)) continue;
            T cast = type.tryCast(view);
            if (cast != null && selector.test(cast)) {
                output.add(cast);
                if (output.size() >= max) return true;
            }
            if (!(view instanceof EnderDragon dragon)) continue;
            for (var part : dragon.getSubEntities()) {
                T castPart = type.tryCast(part);
                if (castPart != null && selector.test(castPart)) {
                    output.add(castPart);
                    if (output.size() >= max) return true;
                }
            }
        }
        return false;
    }

    /** The level ticks none of the views: the window ticks the entities. */
    public static boolean noneTicking(ClientLevel level) {
        for (Entity view : VIEWS.values()) if (level.isTickingEntity(view)) return false;
        return true;
    }
}
