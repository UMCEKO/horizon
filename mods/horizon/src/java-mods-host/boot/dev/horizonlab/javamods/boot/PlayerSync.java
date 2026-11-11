package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.syncher.EntityDataAccessor;
import net.minecraft.resources.Identifier;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.effect.MobEffect;
import net.minecraft.world.effect.MobEffectInstance;
import net.minecraft.network.protocol.game.ClientboundRemoveMobEffectPacket;
import net.minecraft.network.protocol.game.ClientboundUpdateAttributesPacket;
import net.minecraft.network.protocol.game.ClientboundUpdateMobEffectPacket;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.EntityFluidInteraction;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.food.FoodData;

/** The acting player's view writes its food, health, absorption and effects through to Horizon's player as mod code changes them; the empty bodies below are served from Rust. */
public final class PlayerSync {
    private PlayerSync() {}

    /** The player whose call this is, once its view is filled; changes to any other view stay in Java. */
    private static ServerPlayer acting;

    static void food(int food, float saturation, float exhaustion) {}

    static void health(float health) {}

    static void absorption(float amount) {}

    static void effect(MobEffectInstance effect) {}

    static void effectRemoved(Holder<MobEffect> effect) {}

    public static void health(Object entity) {
        if (acting(entity)) health(((LivingEntity) entity).getHealth());
    }

    public static void absorption(Object entity) {
        if (acting(entity)) absorption(((LivingEntity) entity).getAbsorptionAmount());
    }

    public static void effect(Object entity, MobEffectInstance effect) {
        if (acting(entity)) effect(effect);
    }

    public static void removed(Object entity, java.util.Collection<MobEffectInstance> effects) {
        if (acting(entity)) for (var effect : effects) effectRemoved(effect.getEffect());
    }

    public static void food(FoodData food) {
        if (actingFood(food)) food(food.getFoodLevel(), food.getSaturationLevel(), exhaustion(food));
    }

    static void acting(ServerPlayer player) {
        acting = player;
    }

    private static boolean acting(Object entity) {
        return entity != null && entity == acting;
    }

    private static boolean actingFood(FoodData food) {
        var player = acting;
        return player != null && player.getFoodData() == food;
    }

    private static final Field EXHAUSTION = field(FoodData.class, "exhaustionLevel");
    private static final Field FOOD = field(FoodData.class, "foodLevel");
    private static final Field SATURATION = field(FoodData.class, "saturationLevel");

    private static Field field(Class<?> owner, String name) {
        try {
            Field f = owner.getDeclaredField(name);
            f.setAccessible(true);
            return f;
        } catch (NoSuchFieldException e) {
            throw new IllegalStateException(e);
        }
    }

    private static float exhaustion(FoodData food) {
        try {
            return EXHAUSTION.getFloat(food);
        } catch (IllegalAccessException e) {
            throw new IllegalStateException(e);
        }
    }

    @SuppressWarnings("unchecked")
    private static <T> EntityDataAccessor<T> accessor(Class<?> owner, String name) throws ReflectiveOperationException {
        Field f = owner.getDeclaredField(name);
        f.setAccessible(true);
        return (EntityDataAccessor<T>) f.get(null);
    }

    /** Horizon's vitals into the view as they are, setting fields so nothing writes back. */
    static void fillVitals(ServerPlayer player, PlayerGroups g) throws ReflectiveOperationException {
        player.getEntityData().set(PlayerSync.<Float>accessor(LivingEntity.class, "DATA_HEALTH_ID"), g.health);
        FoodData food = player.getFoodData();
        FOOD.setInt(food, g.food);
        SATURATION.setFloat(food, g.saturation);
        EXHAUSTION.setFloat(food, g.exhaustion);
        player.getEntityData().set(PlayerSync.<Float>accessor(Player.class, "DATA_PLAYER_ABSORPTION_ID"), g.absorption);
    }

    /** Horizon's effects into the view, telling the client what changed. */
    static void fillEffects(ServerPlayer player, PlayerGroups g) {
        var effects = player.getActiveEffectsMap();
        var before = new HashMap<>(effects);
        effects.clear();
        for (var effect : g.effects) {
            var holder = BuiltInRegistries.MOB_EFFECT.get(Identifier.parse(effect.id())).orElse(null);
            if (holder != null) effects.put(holder, new MobEffectInstance(holder, effect.duration(), effect.amplifier(), effect.ambient(), effect.visible(), effect.visible()));
        }
        sendEffectChanges(player, before, effects);
    }

    static final int SLOTS = 41;
    /** Per player, copies of the stacks native holds: what its view was last filled with or last told native. */
    private static final Map<UUID, ItemStack[]> SYNCED = new HashMap<>();

    static ItemStack[] synced(UUID id) {
        return SYNCED.computeIfAbsent(id, k -> {
            var stacks = new ItemStack[SLOTS];
            Arrays.fill(stacks, ItemStack.EMPTY);
            return stacks;
        });
    }

    static void forget(UUID id) {
        SYNCED.remove(id);
    }

    static void forgetAll() {
        SYNCED.clear();
    }

    /** Native's own copy of the acting player's slots: they become what Java changed. */
    static void stacksWritten(byte[] slots) {}

    /** Before a routed method's native body: Java's changes to the acting player's slots reach native first, as the body reads them. */
    public static void beforeNativeBody() {
        var player = acting;
        if (player == null) return;
        var synced = synced(player.getUUID());
        var inventory = player.getInventory();
        List<Integer> changed = null;
        for (int slot = 0; slot < SLOTS; slot++) {
            if (!ItemStack.matches(synced[slot], inventory.getItem(slot))) {
                if (changed == null) changed = new ArrayList<>();
                changed.add(slot);
            }
        }
        if (changed == null) return;
        var bytes = new ByteArrayOutputStream();
        try {
            writeSlots(player, new DataOutputStream(bytes), changed);
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
        stacksWritten(bytes.toByteArray());
    }

    /** The reply's block for the player a call filled: experience, then the slots Java changed against what native holds. */
    static void writeChanges(ServerPlayer player, DataOutputStream out) throws IOException {
        out.writeInt(player.experienceLevel);
        out.writeFloat(player.experienceProgress);
        out.writeInt(player.totalExperience);
        var synced = synced(player.getUUID());
        var inventory = player.getInventory();
        var changed = new ArrayList<Integer>();
        for (int slot = 0; slot < SLOTS; slot++) {
            if (!ItemStack.matches(synced[slot], inventory.getItem(slot))) changed.add(slot);
        }
        writeSlots(player, out, changed);
    }

    private static void writeSlots(ServerPlayer player, DataOutputStream out, List<Integer> slots) throws IOException {
        var synced = synced(player.getUUID());
        var inventory = player.getInventory();
        var ops = GameBridge.registries().createSerializationContext(NbtOps.INSTANCE);
        out.writeInt(slots.size());
        for (int slot : slots) {
            out.writeInt(slot);
            ItemStack stack = inventory.getItem(slot);
            if (stack.isEmpty()) out.writeInt(0);
            else GameBridge.writeTag(out, (CompoundTag) ItemStack.CODEC.encodeStart(ops, stack).getOrThrow());
            synced[slot] = stack.copy();
        }
    }

    /** `ServerPlayer.onEffectAdded`, `onEffectUpdated` and `onEffectsRemoved` for what Horizon changed since the last fill: the client hears of an effect only from these. */
    private static void sendEffectChanges(ServerPlayer player, Map<Holder<MobEffect>, MobEffectInstance> before, Map<Holder<MobEffect>, MobEffectInstance> now) {
        for (var effect : now.values()) {
            var old = before.get(effect.getEffect());
            boolean changed = old == null || old.getAmplifier() != effect.getAmplifier() || old.isAmbient() != effect.isAmbient()
                    || old.isVisible() != effect.isVisible() || effect.getDuration() > old.getDuration();
            if (changed) player.connection.send(new ClientboundUpdateMobEffectPacket(player.getId(), effect, false));
        }
        for (var holder : before.keySet()) {
            if (!now.containsKey(holder)) player.connection.send(new ClientboundRemoveMobEffectPacket(player.getId(), holder));
        }
    }

    private static final Field LAST_EQUIPMENT = field(LivingEntity.class, "lastEquipmentItems");
    private static final Field FLUIDS = field(Entity.class, "fluidInteraction");
    private static final Method COLLECT = method(LivingEntity.class, "collectEquipmentChanges", Map.class);

    private static Method method(Class<?> owner, String name, Class<?>... types) {
        try {
            Method m = owner.getDeclaredMethod(name, types);
            m.setAccessible(true);
            return m;
        } catch (NoSuchMethodException e) {
            throw new IllegalStateException(e);
        }
    }

    /** What `LivingEntity.tick` settles before a view's reads: its equipment's attribute modifiers (`detectEquipmentUpdates` without the packet to others) and the fluids at its eyes (`updateFluidInteraction` without its splash and current). */
    @SuppressWarnings("unchecked")
    static void settle(ServerPlayer player) {
        try {
            var last = (Map<EquipmentSlot, ItemStack>) LAST_EQUIPMENT.get(player);
            var changed = (Map<EquipmentSlot, ItemStack>) COLLECT.invoke(player, last);
            if (changed != null) changed.forEach((slot, stack) -> last.put(slot, stack.copy()));
            ((EntityFluidInteraction) FLUIDS.get(player)).update(player, true);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    /** `ServerEntity.sendChanges` for the attributes of the acting player: its own client hears of the ones that changed. */
    static void sendAttributes(ServerPlayer player) {
        settle(player);
        var dirty = player.getAttributes().getAttributesToSync();
        if (!dirty.isEmpty()) player.connection.send(new ClientboundUpdateAttributesPacket(player.getId(), dirty));
        dirty.clear();
    }

    /** `ServerEntity.sendPairingData`'s attributes, for a client that just logged in. */
    static void sendAllAttributes(ServerPlayer player) {
        settle(player);
        player.getAttributes().getAttributesToSync().clear();
        player.connection.send(new ClientboundUpdateAttributesPacket(player.getId(), player.getAttributes().getSyncableAttributes()));
    }
}
