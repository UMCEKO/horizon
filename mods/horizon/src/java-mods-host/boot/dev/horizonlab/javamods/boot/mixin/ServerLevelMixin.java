package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.BlockEntities;
import dev.horizonlab.javamods.boot.EntityViews;
import dev.horizonlab.javamods.boot.GameBridge;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.sounds.SoundSource;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.entity.LevelEntityGetter;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.material.FluidState;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Unique;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

@Mixin(ServerLevel.class)
abstract class ServerLevelMixin extends Level {
    private ServerLevelMixin() {
        super(null, null, null, null, false, false, 0L, 0);
    }

    /** Fluids live in block states, so this is what vanilla's chunk lookup answers. */
    @Override
    public FluidState getFluidState(BlockPos pos) {
        return ((ServerLevel) (Object) this).getBlockState(pos).getFluidState();
    }

    @Override
    public BlockEntity getBlockEntity(BlockPos pos) {
        return BlockEntities.get((ServerLevel) (Object) this, pos);
    }

    @Override
    public void setBlockEntity(BlockEntity entity) {
        BlockEntities.set((ServerLevel) (Object) this, entity);
    }

    @Override
    public void removeBlockEntity(BlockPos pos) {
        BlockEntities.remove((ServerLevel) (Object) this, pos);
    }

    @Override
    public void blockEntityChanged(BlockPos pos) {
        BlockEntities.changed((ServerLevel) (Object) this, pos);
    }

    /** Block collisions read states through the level, which Rust serves; no chunk where none is loaded. */
    @Override
    public BlockGetter getChunkForCollisions(int chunkX, int chunkZ) {
        return hasChunk(chunkX, chunkZ) ? this : null;
    }

    @Unique
    private EntityViews horizon$entities;

    @Inject(method = "getEntities()Lnet/minecraft/world/level/entity/LevelEntityGetter;", at = @At("HEAD"), cancellable = true)
    private void horizon$entities(CallbackInfoReturnable<LevelEntityGetter<Entity>> result) {
        if (horizon$entities == null) horizon$entities = new EntityViews((ServerLevel) (Object) this);
        result.setReturnValue(horizon$entities);
    }

    @Inject(method = "broadcastEntityEvent", at = @At("HEAD"), cancellable = true)
    private void horizon$entityEvent(Entity entity, byte event, CallbackInfo info) {
        info.cancel();
    }

    @Inject(method = "broadcastDamageEvent", at = @At("HEAD"), cancellable = true)
    private void horizon$damageEvent(Entity entity, DamageSource source, CallbackInfo info) {
        info.cancel();
    }

    @Inject(method = "getNextEntityId", at = @At("HEAD"), cancellable = true)
    private void horizon$nextEntityId(CallbackInfoReturnable<Integer> result) {
        result.setReturnValue(GameBridge.nextEntityId());
    }

    @Inject(method = "addFreshEntity", at = @At("HEAD"), cancellable = true)
    private void horizon$addFreshEntity(Entity entity, CallbackInfoReturnable<Boolean> result) {
        result.setReturnValue(GameBridge.spawn(entity));
    }

    @Inject(method = "playSeededSound(Lnet/minecraft/world/entity/Entity;DDDLnet/minecraft/core/Holder;Lnet/minecraft/sounds/SoundSource;FFJ)V", at = @At("HEAD"), cancellable = true)
    private void horizon$soundAt(Entity except, double x, double y, double z, Holder<SoundEvent> sound, SoundSource source, float volume, float pitch, long seed, CallbackInfo info) {
        GameBridge.sound(sound.value(), x, y, z, volume, pitch);
        info.cancel();
    }

    @Inject(method = "playSeededSound(Lnet/minecraft/world/entity/Entity;Lnet/minecraft/world/entity/Entity;Lnet/minecraft/core/Holder;Lnet/minecraft/sounds/SoundSource;FFJ)V", at = @At("HEAD"), cancellable = true)
    private void horizon$soundFrom(Entity except, Entity from, Holder<SoundEvent> sound, SoundSource source, float volume, float pitch, long seed, CallbackInfo info) {
        GameBridge.sound(sound.value(), from.getX(), from.getY(), from.getZ(), volume, pitch);
        info.cancel();
    }
}
