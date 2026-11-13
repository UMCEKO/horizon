package dev.horizonlab.javamods.boot;

import java.util.function.Supplier;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;

/** `Item.use` inside a merged `ItemStack.use` that native started: a vanilla item's use is native's port, not Java's copy of vanilla. */
public final class NativeUse {
    private NativeUse() {}

    /** Bridged: native's use of the held item on the acting player, as `Flat.toInteractionResult` reads it. */
    static int nativeUse() {
        throw new IllegalStateException("not bridged");
    }

    /** `Item.use` as `ItemStack.use` calls it: native's port for the item the route was started for, else `original`. */
    public static InteractionResult use(Item item, Player player, Supplier<InteractionResult> original) {
        var served = Routing.serving();
        if (served == null || served.getItem() != item || !(player instanceof ServerPlayer) || !Routing.vanillaItem(item)) return original.get();
        return Flat.toInteractionResult(nativeUse());
    }
}
