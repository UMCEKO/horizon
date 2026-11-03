package dev.horizonlab.javamods.boot;

import java.io.DataInputStream;
import java.io.IOException;
import java.lang.reflect.Field;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.syncher.EntityDataAccessor;
import net.minecraft.resources.Identifier;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.effect.MobEffect;
import net.minecraft.world.effect.MobEffectInstance;
import net.minecraft.world.entity.LivingEntity;
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
    static void fill(ServerPlayer player, DataInputStream in) throws IOException, ReflectiveOperationException {
        player.getEntityData().set(PlayerSync.<Float>accessor(LivingEntity.class, "DATA_HEALTH_ID"), in.readFloat());
        FoodData food = player.getFoodData();
        FOOD.setInt(food, in.readInt());
        SATURATION.setFloat(food, in.readFloat());
        EXHAUSTION.setFloat(food, in.readFloat());
        player.getEntityData().set(PlayerSync.<Float>accessor(Player.class, "DATA_PLAYER_ABSORPTION_ID"), in.readFloat());
        var effects = player.getActiveEffectsMap();
        effects.clear();
        for (int n = in.readInt(); n > 0; n--) {
            var id = Identifier.parse(GameBridge.text(in));
            int duration = in.readInt(), amplifier = in.readInt();
            boolean ambient = in.readBoolean(), visible = in.readBoolean();
            var holder = BuiltInRegistries.MOB_EFFECT.get(id).orElse(null);
            if (holder != null) effects.put(holder, new MobEffectInstance(holder, duration, amplifier, ambient, visible, visible));
        }
    }
}
