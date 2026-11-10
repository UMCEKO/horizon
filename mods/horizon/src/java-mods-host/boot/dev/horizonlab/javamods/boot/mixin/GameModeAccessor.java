package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.server.level.ServerPlayerGameMode;
import net.minecraft.world.level.GameType;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

@Mixin(ServerPlayerGameMode.class)
public interface GameModeAccessor {
    @Accessor("gameModeForPlayer")
    void horizon$type(GameType type);
}
