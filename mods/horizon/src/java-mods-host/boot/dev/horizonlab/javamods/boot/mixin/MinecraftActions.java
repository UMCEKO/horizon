package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.client.Minecraft;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;

/** `handleKeybinds`'s click actions, which the window hands over one at a time as it acts. */
@Mixin(Minecraft.class)
public interface MinecraftActions {
    @Invoker("startAttack")
    boolean horizon$startAttack();

    @Invoker("startUseItem")
    void horizon$startUseItem();

    @Invoker("pick")
    void horizon$pick(float partialTicks);

    @Invoker("isLevelRunningNormally")
    boolean horizon$runningNormally();

    @Invoker("pickBlockOrEntity")
    void horizon$pickBlockOrEntity();
}
