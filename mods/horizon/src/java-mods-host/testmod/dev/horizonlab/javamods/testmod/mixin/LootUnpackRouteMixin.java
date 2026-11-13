package dev.horizonlab.javamods.testmod.mixin;

import dev.horizonlab.javamods.testmod.Checks;
import dev.horizonlab.javamods.testmod.RouteState;
import net.minecraft.world.RandomizableContainer;
import net.minecraft.world.entity.player.Player;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** A body route on `unpackLootTable`, as Lootr's: it sees the opening player (null for a hopper) and the table the view loaded from Horizon. */
@Mixin(RandomizableContainer.class)
public interface LootUnpackRouteMixin {
    @Inject(method = "unpackLootTable", at = @At(value = "INVOKE", target = "Lnet/minecraft/world/RandomizableContainer;getLootTable()Lnet/minecraft/resources/ResourceKey;"))
    private void horizon$unpacking(Player player, CallbackInfo ci) {
        var container = (RandomizableContainer) (Object) this;
        if (container.getLootTable() == null) return;
        RouteState.unpackPlayers.add(player == null ? null : player.getPlainTextName());
        if (player != null) Checks.check("unpack_routed_with_player", () -> container.getLootTableSeed() == 12345L);
        else Checks.check("unpack_routed_without_player", () -> true);
    }
}
