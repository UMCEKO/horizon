package dev.horizonlab.javamods.testmod.mixin;

import dev.horizonlab.javamods.testmod.Checks;
import dev.horizonlab.javamods.testmod.RouteState;
import java.util.Arrays;
import net.minecraft.client.Minecraft;
import net.minecraft.sounds.SoundSource;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyArg;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Handlers on the loop methods Horizon never runs: the constructor's end, `run`'s start and `runTick`'s pause. */
@Mixin(Minecraft.class)
public abstract class MinecraftLoopMixin {
    @Inject(method = "<init>", at = @At("RETURN"))
    private void horizon$constructed(CallbackInfo ci) {
        RouteState.initRan = true;
    }

    @Inject(method = "run", at = @At("HEAD"))
    private void horizon$run(CallbackInfo ci) {
        RouteState.runRan = true;
        RouteState.initBeforeRun = RouteState.initRan;
        Checks.check("loop_init_then_run", () -> RouteState.initBeforeRun && RouteState.runRan);
    }

    @ModifyArg(method = "runTick", at = @At(value = "INVOKE", target = "Lnet/minecraft/client/sounds/SoundManager;pauseAllExcept([Lnet/minecraft/sounds/SoundSource;)V"))
    private SoundSource[] horizon$keep(SoundSource[] sources) {
        return Arrays.stream(sources).filter(source -> source != SoundSource.UI).toArray(SoundSource[]::new);
    }
}
