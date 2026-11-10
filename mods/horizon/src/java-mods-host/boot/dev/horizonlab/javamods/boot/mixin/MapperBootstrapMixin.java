package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.BootFacts;
import java.lang.invoke.MethodHandles;
import net.minecraft.client.color.item.ItemTintSources;
import net.minecraft.client.renderer.item.ItemModels;
import net.minecraft.client.renderer.item.properties.conditional.ConditionalItemModelProperties;
import net.minecraft.client.renderer.item.properties.numeric.RangeSelectItemModelProperties;
import net.minecraft.client.renderer.item.properties.select.SelectItemModelProperties;
import net.minecraft.client.renderer.special.SpecialModelRenderers;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfo;

/** Each item model mapper's bootstrap has put vanilla's entries when it hands the mods their registration event. */
@Mixin({ItemModels.class, SpecialModelRenderers.class, ItemTintSources.class, SelectItemModelProperties.class, ConditionalItemModelProperties.class, RangeSelectItemModelProperties.class})
abstract class MapperBootstrapMixin {
    @Inject(method = "bootstrap", require = 0, at = {
        @At(value = "INVOKE", target = "Lnet/neoforged/fml/ModLoader;postEvent(Lnet/neoforged/bus/api/Event;)V"),
        @At(value = "INVOKE", target = "Lnet/neoforged/fml/ModLoader;postEventWithReturn(Lnet/neoforged/bus/api/Event;)Lnet/neoforged/bus/api/Event;")})
    private static void horizon$vanillaPut(CallbackInfo info) {
        BootFacts.snapshotMapper(MethodHandles.lookup().lookupClass());
    }
}
