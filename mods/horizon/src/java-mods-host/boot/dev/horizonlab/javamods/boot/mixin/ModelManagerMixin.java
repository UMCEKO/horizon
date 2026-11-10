package dev.horizonlab.javamods.boot.mixin;

import dev.horizonlab.javamods.boot.ClientModels;
import java.util.Map;
import net.minecraft.client.resources.model.BlockStateModelLoader;
import net.minecraft.client.resources.model.ClientItemInfoLoader;
import net.minecraft.client.resources.model.ModelManager;
import net.minecraft.client.resources.model.UnbakedModel;
import net.minecraft.resources.Identifier;
import net.neoforged.neoforge.client.model.standalone.StandaloneModelLoader;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.callback.CallbackInfoReturnable;

/** What the reload's model discovery starts from: which block states and items name a model type only their mod's code knows. */
@Mixin(ModelManager.class)
abstract class ModelManagerMixin {
    @Inject(method = "discoverModelDependencies(Ljava/util/Map;Lnet/minecraft/client/resources/model/BlockStateModelLoader$LoadedModels;Lnet/minecraft/client/resources/model/ClientItemInfoLoader$LoadedClientInfos;Lnet/neoforged/neoforge/client/model/standalone/StandaloneModelLoader$LoadedModels;)Lnet/minecraft/client/resources/model/ModelManager$ResolvedModels;",
            at = @At("HEAD"))
    @SuppressWarnings("rawtypes")
    private static void horizon$discover(Map<Identifier, UnbakedModel> models, BlockStateModelLoader.LoadedModels states, ClientItemInfoLoader.LoadedClientInfos items,
            StandaloneModelLoader.LoadedModels standalone, CallbackInfoReturnable result) {
        ClientModels.discovered(states, items);
    }
}
