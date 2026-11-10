package dev.horizonlab.javamods.boot.mixin;

import net.minecraft.network.syncher.EntityDataAccessor;
import net.minecraft.network.syncher.SynchedEntityData;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;

/** A synched value written without `onSyncedDataUpdated` or the dirty mark: the data item itself. */
@Mixin(SynchedEntityData.class)
public interface SynchedAccess {
    @Invoker("getItem")
    <T> SynchedEntityData.DataItem<T> horizon$item(EntityDataAccessor<T> accessor);
}
