package dev.horizonlab.javamods.testmod.mixin;

import dev.horizonlab.javamods.testmod.Checks;
import dev.horizonlab.javamods.testmod.RouteState;
import net.minecraft.world.entity.player.Inventory;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** A server-only cancel at HEAD, and handlers after the body, on a routed vanilla method. */
@Mixin(Inventory.class)
public abstract class InventoryRouteMixin {
    @Inject(method = "setSelectedSlot", at = @At("HEAD"), cancellable = true)
    private void horizon$head(int slot, CallbackInfo ci) {
        var inventory = (Inventory) (Object) this;
        if (!RouteState.server(inventory)) return;
        if (slot == 7) {
            RouteState.canceled = true;
            ci.cancel();
        } else if (RouteState.canceled) {
            RouteState.canceled = false;
            Checks.check("route_head_cancel", () -> inventory.getSelectedSlot() != 7);
        }
    }

    @Inject(method = "setSelectedSlot", at = @At("TAIL"))
    private void horizon$tail(int slot, CallbackInfo ci) {
        var inventory = (Inventory) (Object) this;
        if (!RouteState.server(inventory)) return;
        RouteState.tails++;
        Checks.check("route_tail_sees_body", () -> inventory.getSelectedSlot() == slot);
    }

    @Inject(method = "setSelectedSlot", at = @At("RETURN"))
    private void horizon$return(int slot, CallbackInfo ci) {
        var inventory = (Inventory) (Object) this;
        if (!RouteState.server(inventory)) return;
        RouteState.returns++;
        Checks.check("route_return_equals_tail", () -> inventory.getSelectedSlot() == slot && RouteState.returns >= RouteState.tails && RouteState.returns - RouteState.tails <= 1);
    }
}
