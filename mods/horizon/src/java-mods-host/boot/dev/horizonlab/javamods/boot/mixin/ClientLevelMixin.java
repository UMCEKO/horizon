package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ClientBridge;
import dev.horizonlab.javamods.boot.ClientWorld;
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

/** The window's level holds the chunks in Java: its blocks, heights, light and biomes are read from them, and its block entities are client copies of the server's. */
@Mixin(ClientLevel.class)
abstract class ClientLevelMixin extends Level {
    protected ClientLevelMixin(WritableLevelData data, ResourceKey<Level> dimension, RegistryAccess registries, Holder<DimensionType> type, boolean client, boolean debug, long seed, int updates) {
        super(data, dimension, registries, type, client, debug, seed, updates);
    }

    /** Light is read from the window's chunks (`getBrightness`); there is no engine to run here. */
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

    @Override
    public Holder<net.minecraft.world.level.biome.Biome> getNoiseBiome(int x, int y, int z) {
        return ClientWorld.biome(x, y, z);
    }

    @Override
    public int getHeight(net.minecraft.world.level.levelgen.Heightmap.Types type, int x, int z) {
        return ClientWorld.height(type, x, z);
    }

    @Override
    public int getBrightness(net.minecraft.world.level.LightLayer layer, BlockPos pos) {
        return ClientWorld.brightness(layer, pos);
    }

    @Override
    public int getRawBrightness(BlockPos pos, int darken) {
        return ClientWorld.rawBrightness(pos, darken);
    }

    @Override
    public net.minecraft.world.level.material.FluidState getFluidState(BlockPos pos) {
        return ClientBridge.blockState((ClientLevel) (Object) this, pos).getFluidState();
    }
}
