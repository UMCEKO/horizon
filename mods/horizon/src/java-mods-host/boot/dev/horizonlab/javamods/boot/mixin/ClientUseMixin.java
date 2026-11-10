package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ClientBridge;
import net.minecraft.client.multiplayer.MultiPlayerGameMode;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** What `useItemOn` answered on this client, which ends the click as it does in vanilla. */
@Mixin(MultiPlayerGameMode.class)
abstract class ClientUseMixin {
    @Inject(method = "useItemOn", at = @At("RETURN"))
    private void horizon$used(net.minecraft.client.player.LocalPlayer player, InteractionHand hand, net.minecraft.world.phys.BlockHitResult hit, CallbackInfoReturnable<InteractionResult> result) {
        ClientBridge.blockUsed(hand, result.getReturnValue());
    }
}
