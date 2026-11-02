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

    @Override
    public BlockEntity getBlockEntity(BlockPos pos) {
        return ClientBridge.blockEntity((ClientLevel) (Object) this, pos);
    }

    @Override
    public BlockState getBlockState(BlockPos pos) {
        return ClientBridge.blockState((ClientLevel) (Object) this, pos);
    }
}
