package dev.horizonlab.javamods.testmod.mixin;

import net.minecraft.client.player.ClientInput;
import net.minecraft.world.phys.Vec2;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

@Mixin(ClientInput.class)
public interface ClientInputSet {
    @Accessor("moveVector")
    void horizon$testMoveVector(Vec2 value);
}
