package dev.horizonlab.javamods.boot;

import java.io.DataInputStream;
import java.io.IOException;
import java.lang.reflect.Field;
import java.lang.reflect.Method;
import java.util.HashMap;
import java.util.Map;
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

    /** Horizon's vitals and effects into the view as they are, setting fields so nothing writes back. */
    static void fill(ServerPlayer player, Crossing.In in) throws IOException, ReflectiveOperationException {
        player.getEntityData().set(PlayerSync.<Float>accessor(LivingEntity.class, "DATA_HEALTH_ID"), in.readFloat());
        FoodData food = player.getFoodData();
        FOOD.setInt(food, in.readInt());
        SATURATION.setFloat(food, in.readFloat());
        EXHAUSTION.setFloat(food, in.readFloat());
        player.getEntityData().set(PlayerSync.<Float>accessor(Player.class, "DATA_PLAYER_ABSORPTION_ID"), in.readFloat());
        var effects = player.getActiveEffectsMap();
        var before = new HashMap<>(effects);
        effects.clear();
        for (int n = in.readInt(); n > 0; n--) {
            var id = Identifier.parse(GameBridge.text(in));
            int duration = in.readInt(), amplifier = in.readInt();
            boolean ambient = in.readBoolean(), visible = in.readBoolean();
            var holder = BuiltInRegistries.MOB_EFFECT.get(id).orElse(null);
            if (holder != null) effects.put(holder, new MobEffectInstance(holder, duration, amplifier, ambient, visible, visible));
        }
        sendEffectChanges(player, before, effects);
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
