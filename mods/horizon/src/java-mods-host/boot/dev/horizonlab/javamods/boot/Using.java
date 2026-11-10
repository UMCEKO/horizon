package dev.horizonlab.javamods.boot;

import java.lang.reflect.Field;
import java.lang.reflect.Method;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.item.ItemStack;

/** A held use on a player view: Horizon's player holds it between calls, and each call that goes on with it puts it back as `LivingEntity` keeps it. */
final class Using {
    private Using() {}

    private static final Field ITEM = field("useItem");
    private static final Field REMAINING = field("useItemRemaining");
    private static final Method FLAG = method("setLivingEntityFlag", int.class, boolean.class);
    private static final Method TICK = method("updatingUsingItem");

    private static Field field(String name) {
        try {
            Field f = LivingEntity.class.getDeclaredField(name);
            f.setAccessible(true);
            return f;
        } catch (NoSuchFieldException e) {
            throw new IllegalStateException(e);
        }
    }

    private static Method method(String name, Class<?>... parameters) {
        try {
            Method m = LivingEntity.class.getDeclaredMethod(name, parameters);
            m.setAccessible(true);
            return m;
        } catch (NoSuchMethodException e) {
            throw new IllegalStateException(e);
        }
    }

    /** No use, without `stopUsingItem`'s game event: the view only forgets one an earlier call left. */
    static void clear(LivingEntity entity) throws ReflectiveOperationException {
        ITEM.set(entity, ItemStack.EMPTY);
        REMAINING.setInt(entity, 0);
        FLAG.invoke(entity, 1, false);
    }

    /** The use `startUsingItem` began `ticks` ticks ago with the stack now in `hand`. */
    static void resume(LivingEntity entity, InteractionHand hand, int ticks) throws ReflectiveOperationException {
        ItemStack stack = entity.getItemInHand(hand);
        if (stack.isEmpty()) return;
        ITEM.set(entity, stack);
        REMAINING.setInt(entity, stack.getUseDuration(entity) - ticks);
        FLAG.invoke(entity, 1, true);
        FLAG.invoke(entity, 2, hand == InteractionHand.OFF_HAND);
    }

    /** `LivingEntity.updatingUsingItem`, as the entity's tick runs it. */
    static void tick(LivingEntity entity) throws ReflectiveOperationException {
        TICK.invoke(entity);
    }
}
