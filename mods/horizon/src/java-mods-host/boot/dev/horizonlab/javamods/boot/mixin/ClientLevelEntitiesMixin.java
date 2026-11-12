package dev.horizonlab.javamods.boot.mixin;

import com.google.common.collect.Iterables;
import dev.horizonlab.javamods.boot.ClientEntities;
import java.util.List;
import java.util.UUID;
import java.util.function.Predicate;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.core.Holder;
import net.minecraft.core.RegistryAccess;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.dimension.DimensionType;
import net.minecraft.world.level.entity.EntityTypeTest;
import net.minecraft.world.level.storage.WritableLevelData;
import net.minecraft.world.phys.AABB;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** The window's entities answer the level's entity queries beside what its own storage holds (the players); its section manager stays empty of them. */
@Mixin(ClientLevel.class)
abstract class ClientLevelEntitiesMixin extends Level {
    protected ClientLevelEntitiesMixin(WritableLevelData data, ResourceKey<Level> dimension, RegistryAccess registries, Holder<DimensionType> type, boolean client, boolean debug, long seed, int updates) {
        super(data, dimension, registries, type, client, debug, seed, updates);
    }

    @Inject(method = "getEntity(I)Lnet/minecraft/world/entity/Entity;", at = @At("HEAD"), cancellable = true)
    private void horizon$entity(int id, CallbackInfoReturnable<Entity> info) {
        Entity view = ClientEntities.get(id);
        if (view != null) info.setReturnValue(view);
    }

    @Inject(method = "entitiesForRendering", at = @At("RETURN"), cancellable = true)
    private void horizon$forRendering(CallbackInfoReturnable<Iterable<Entity>> info) {
        info.setReturnValue(Iterables.concat(info.getReturnValue(), ClientEntities.all()));
    }

    @Inject(method = "getEntityCount", at = @At("RETURN"), cancellable = true)
    private void horizon$count(CallbackInfoReturnable<Integer> info) {
        info.setReturnValue(info.getReturnValue() + ClientEntities.count());
    }

    @Inject(method = "tickEntities", at = @At("HEAD"))
    private void horizon$tickEntities(CallbackInfo info) {
        assert ClientEntities.noneTicking((ClientLevel) (Object) this) : "the Java client ticks an entity the window ticks";
    }

    @Override
    public Entity getEntity(UUID uuid) {
        Entity view = ClientEntities.get(uuid);
        return view != null ? view : super.getEntity(uuid);
    }

    @Override
    public List<Entity> getEntities(Entity except, AABB box, Predicate<? super Entity> selector) {
        List<Entity> out = super.getEntities(except, box, selector);
        ClientEntities.collect(except, box, selector, out);
        return out;
    }

    @Override
    public <T extends Entity> void getEntities(EntityTypeTest<Entity, T> type, AABB box, Predicate<? super T> selector, List<? super T> output, int maxResults) {
        super.getEntities(type, box, selector, output, maxResults);
        if (output.size() < maxResults) ClientEntities.collect(type, box, selector, output, maxResults);
    }
}
