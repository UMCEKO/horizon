package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ClientSounds;
import net.minecraft.client.resources.sounds.SoundInstance;
import net.minecraft.client.resources.sounds.TickableSoundInstance;
import net.minecraft.client.sounds.SoundEngine;
import net.minecraft.resources.Identifier;
import net.minecraft.sounds.SoundSource;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** Audio is native: the channels this engine would open are Horizon's, which `ClientSounds` tells. */
@Mixin(SoundEngine.class)
abstract class SoundEngineMixin {
    @Inject(method = "play", at = @At("HEAD"), cancellable = true)
    private void horizon$play(SoundInstance instance, CallbackInfoReturnable<SoundEngine.PlayResult> result) {
        result.setReturnValue(ClientSounds.play(instance, 0));
    }

    @Inject(method = "playDelayed", at = @At("HEAD"), cancellable = true)
    private void horizon$playDelayed(SoundInstance instance, int delay, CallbackInfo info) {
        info.cancel();
        ClientSounds.play(instance, delay);
    }

    @Inject(method = "stop(Lnet/minecraft/client/resources/sounds/SoundInstance;)V", at = @At("HEAD"), cancellable = true)
    private void horizon$stop(SoundInstance instance, CallbackInfo info) {
        info.cancel();
        ClientSounds.stop(instance);
    }

    @Inject(method = "stop(Lnet/minecraft/resources/Identifier;Lnet/minecraft/sounds/SoundSource;)V", at = @At("HEAD"), cancellable = true)
    private void horizon$stopMatching(Identifier sound, SoundSource source, CallbackInfo info) {
        info.cancel();
        ClientSounds.stop(sound, source);
    }

    @Inject(method = "stopAll", at = @At("HEAD"), cancellable = true)
    private void horizon$stopAll(CallbackInfo info) {
        info.cancel();
        ClientSounds.stopAll();
    }

    @Inject(method = "isActive", at = @At("HEAD"), cancellable = true)
    private void horizon$active(SoundInstance instance, CallbackInfoReturnable<Boolean> result) {
        result.setReturnValue(ClientSounds.isActive(instance));
    }

    @Inject(method = "queueTickingSound", at = @At("HEAD"), cancellable = true)
    private void horizon$queue(TickableSoundInstance instance, CallbackInfo info) {
        info.cancel();
        ClientSounds.queueTicking(instance);
    }

    @Inject(method = "tick", at = @At("HEAD"), cancellable = true)
    private void horizon$tick(boolean paused, CallbackInfo info) {
        info.cancel();
        ClientSounds.tick(paused);
    }

    /** The library would open an audio device in the game's process at the resource reload; the sound registry the reload fills stays. */
    @Inject(method = "loadLibrary", at = @At("HEAD"), cancellable = true)
    private void horizon$library(CallbackInfo info) {
        info.cancel();
        // Mods keep the engine this event hands them, to stop their sounds with.
        net.neoforged.fml.ModLoader.postEvent(new net.neoforged.neoforge.client.event.sound.SoundEngineLoadEvent((SoundEngine) (Object) this));
    }

    @Inject(method = "<init>", at = @At("RETURN"))
    private void horizon$made(CallbackInfo info) {
        ClientSounds.engine = (SoundEngine) (Object) this;
    }
}
