package dev.horizonlab.neoforgeclient;

import dev.horizonlab.facts.BakedWriter;
import java.nio.file.Path;
import java.util.Set;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.block.dispatch.BlockStateModel;

/** Every block state whose model a mod's code builds, baked once into the quads `carry` writes as `horizon:baked` models. */
final class Baker {
    private Baker() {}

    private static final Set<String> VANILLA = Set.of("SingleVariant", "WeightedVariants", "MultiPartModel");

    static boolean codeBuilt(BlockStateModel model) {
        return !VANILLA.contains(model.getClass().getSimpleName());
    }

    static void run(Path out) throws Exception {
        var models = Minecraft.getInstance().getModelManager().getBlockStateModelSet();
        int[] n = BakedWriter.write(out, models::get, state -> codeBuilt(models.get(state)));
        System.err.println("horizon oracle: baked " + n[0] + " blocks, " + n[1] + " states, " + n[2] + " quads");
    }
}
