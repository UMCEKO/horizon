package dev.horizonlab.javamods.testmod.mixin;

import dev.horizonlab.javamods.testmod.Checks;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.BlockHitResult;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** An edge route: a player tagged `horizon_block` gets nothing from a crafting table, because the mod answers before the block does. */
@Mixin(BlockBehaviour.BlockStateBase.class)
public abstract class UseWithoutItemMixin {
    @Inject(method = "useWithoutItem", at = @At("HEAD"), cancellable = true)
    private void horizon$table(Level level, Player player, BlockHitResult hit, CallbackInfoReturnable<InteractionResult> result) {
        var state = (BlockState) (Object) this;
        if (!(player instanceof ServerPlayer) || !state.is(Blocks.CRAFTING_TABLE) || !player.getTags().contains("horizon_block")) return;
        Checks.check("use_table_head_cancelled", () -> true);
        result.setReturnValue(InteractionResult.SUCCESS);
    }
}
