package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.GameBridge;
import java.nio.file.Path;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.level.storage.LevelResource;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Horizon's world folder, so mods' server configs and data live with the save, as on any server. */
@Mixin(MinecraftServer.class)
abstract class MinecraftServerMixin {
    @Inject(method = "getWorldPath", at = @At("HEAD"), cancellable = true)
    private void horizon$worldPath(LevelResource resource, CallbackInfoReturnable<Path> result) {
        result.setReturnValue(GameBridge.worldFolder().resolve(resource.id()).normalize());
    }

    /** A server here is a view of Horizon's, which stops it itself: the client leaving finds it already shut down. */
    @Inject(method = "halt", at = @At("HEAD"), cancellable = true)
    private void horizon$halt(boolean wait, CallbackInfo info) {
        info.cancel();
    }

    @Inject(method = "isShutdown", at = @At("HEAD"), cancellable = true)
    private void horizon$shutdown(CallbackInfoReturnable<Boolean> result) {
        result.setReturnValue(true);
    }
}
