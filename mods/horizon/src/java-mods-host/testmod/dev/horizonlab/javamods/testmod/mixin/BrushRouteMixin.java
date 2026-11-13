package dev.horizonlab.javamods.testmod.mixin;

import dev.horizonlab.javamods.testmod.Checks;
import dev.horizonlab.javamods.testmod.RouteState;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.BrushItem;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.Level;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** A body route on `BrushItem.onUseTick`: one call per tick of brushing, on the player's own brush. */
@Mixin(BrushItem.class)
public abstract class BrushRouteMixin {
    @Inject(method = "onUseTick", at = @At("HEAD"))
    private void horizon$brushTick(Level level, LivingEntity entity, ItemStack stack, int remaining, CallbackInfo ci) {
        if (level.isClientSide() || !(entity instanceof Player player)) return;
        // A full stroke is ten ticks: by then each tick crossed once, with the player's own stack in hand.
        if (++RouteState.brushTicks == 10) Checks.check("brush_route_ticks", () -> player.getMainHandItem() == stack && remaining > 0);
    }
}
