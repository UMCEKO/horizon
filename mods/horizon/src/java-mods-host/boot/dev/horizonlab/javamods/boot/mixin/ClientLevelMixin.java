package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ClientBridge;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.RegistryAccess;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.dimension.DimensionType;
import net.minecraft.world.level.storage.WritableLevelData;
import org.spongepowered.asm.mixin.Mixin;

/** The window's level has no chunks in Java: its block entities are client copies of the server's. */
@Mixin(ClientLevel.class)
abstract class ClientLevelMixin extends Level {
    protected ClientLevelMixin(WritableLevelData data, ResourceKey<Level> dimension, RegistryAccess registries, Holder<DimensionType> type, boolean client, boolean debug, long seed, int updates) {
        super(data, dimension, registries, type, client, debug, seed, updates);
    }

    /** Horizon lights its own chunks; a mod's light check has no engine to reach. */
    @Override
    public net.minecraft.world.level.lighting.LevelLightEngine getLightEngine() {
        return net.minecraft.world.level.lighting.LevelLightEngine.EMPTY;
    }

    /** A block entity's client data changed: the window bakes the block's geometry again. */
    @Override
    public void sendBlockUpdated(BlockPos pos, BlockState old, BlockState current, int flags) {
        ClientBridge.redraw(current, pos);
    }

    /** The client's own record of blocks takes what the client itself sets. */
    @Override
    public boolean setBlock(BlockPos pos, BlockState state, int flags, int limit) {
        ClientBridge.setBlock(pos, state);
        return true;
    }

    /** A server's block update: vanilla's `super.setBlock` would reach the server's chunk views, so the client's record takes it here. */
    @org.spongepowered.asm.mixin.injection.Inject(method = "setServerVerifiedBlockState", at = @org.spongepowered.asm.mixin.injection.At("HEAD"), cancellable = true)
    private void horizon$serverState(BlockPos pos, BlockState state, int flags, org.spongepowered.asm.mixin.injection.callback.CallbackInfo info) {
        ClientBridge.setBlock(pos, state);
        info.cancel();
    }

    @Override
    public BlockEntity getBlockEntity(BlockPos pos) {
        return ClientBridge.blockEntity((ClientLevel) (Object) this, pos);
    }

    @Override
    public BlockState getBlockState(BlockPos pos) {
        return ClientBridge.blockState((ClientLevel) (Object) this, pos);
    }
}
