package dev.horizonlab.javamods.boot;

import java.lang.reflect.Method;
import java.util.List;
import java.util.Set;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.Level;
import net.neoforged.neoforge.common.extensions.IItemExtension;

/** Which parts of a click with an item are its mod's own code, so they run in Java while Horizon runs the rest natively. */
public final class ItemCode {
    private ItemCode() {}

    /** `Item` and `IItemExtension` methods a right click in the air runs, from `use` to the end of a held use. */
    public static final List<Method> USE = methods(
            "use", List.of(Level.class, Player.class, InteractionHand.class),
            "finishUsingItem", List.of(ItemStack.class, Level.class, LivingEntity.class),
            "onUseTick", List.of(Level.class, LivingEntity.class, ItemStack.class, int.class),
            "releaseUsing", List.of(ItemStack.class, Level.class, LivingEntity.class, int.class),
            "getUseDuration", List.of(ItemStack.class, LivingEntity.class),
            "getUseAnimation", List.of(ItemStack.class),
            "useOnRelease", List.of(ItemStack.class),
            "onStopUsing", List.of(ItemStack.class, LivingEntity.class, int.class),
            "canContinueUsing", List.of(ItemStack.class, ItemStack.class),
            "canEquip", List.of(ItemStack.class, EquipmentSlot.class, LivingEntity.class),
            "getEquipmentSlot", List.of(ItemStack.class));
    /** The methods a right click on a block runs before the click falls through to `use`. */
    public static final List<Method> USE_ON = methods(
            "useOn", List.of(UseOnContext.class),
            "onItemUseFirst", List.of(ItemStack.class, UseOnContext.class),
            "doesSneakBypassUse", List.of(ItemStack.class, net.minecraft.world.level.LevelReader.class, net.minecraft.core.BlockPos.class, Player.class));
    /** The method a right click on a living entity runs once the entity passed. */
    public static final List<Method> ENTITY = methods("interactLivingEntity", List.of(ItemStack.class, Player.class, LivingEntity.class, InteractionHand.class));

    /** The method that runs after a player's break removed a block, where a tool spends itself and may break more. */
    public static final List<Method> MINE = methods("mineBlock", List.of(ItemStack.class, Level.class, net.minecraft.world.level.block.state.BlockState.class, net.minecraft.core.BlockPos.class, LivingEntity.class));

    private static List<Method> methods(Object... pairs) {
        var out = new java.util.ArrayList<Method>();
        for (int i = 0; i < pairs.length; i += 2) {
            @SuppressWarnings("unchecked")
            var parameters = ((List<Class<?>>) pairs[i + 1]).toArray(Class<?>[]::new);
            try {
                out.add(Item.class.getMethod((String) pairs[i], parameters));
            } catch (NoSuchMethodException e) {
                throw new IllegalStateException("Item has no " + pairs[i] + " to check mods' items against", e);
            }
        }
        return List.copyOf(out);
    }

    /** The classes whose use Horizon runs natively: `Item` from the stack's components, `BlockItem` for the blocks it declares. */
    private static final Set<Class<?>> NATIVE = Set.of(Item.class, IItemExtension.class, net.minecraft.world.item.BlockItem.class);

    /** Whether the class answers one of these methods with code Horizon doesn't run natively. */
    public static boolean ownCode(Class<?> type, List<Method> methods) {
        for (Method m : methods) {
            try {
                if (!NATIVE.contains(type.getMethod(m.getName(), m.getParameterTypes()).getDeclaringClass())) return true;
            } catch (NoSuchMethodException e) {
                throw new IllegalStateException(e);
            }
        }
        return false;
    }

    public static boolean use(Class<?> type) {
        return ownCode(type, USE);
    }

    public static boolean useOn(Class<?> type) {
        return ownCode(type, USE_ON);
    }

    /** `BlockItem.place` runs its block's code (`getStateForPlacement`, `setPlacedBy`, its block entity's `applyComponentsFromItemStack`), a mod's for a block a mod owns. */
    static boolean useOn(Item item) {
        return useOn(item.getClass()) || item instanceof net.minecraft.world.item.BlockItem placing && BootFacts.ownsBlock(placing.getBlock());
    }

    public static boolean mine(Class<?> type) {
        return ownCode(type, MINE);
    }

    public static boolean entity(Class<?> type) {
        return ownCode(type, ENTITY);
    }

}
