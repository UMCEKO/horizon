package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.TreeMap;
import java.util.concurrent.CompletableFuture;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.texture.TextureAtlas;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.server.packs.resources.PreparableReloadListener;
import net.minecraft.util.Unit;
import net.minecraft.world.item.Item;
import net.minecraft.world.level.block.Block;

/** What the client's first resource reload made that Horizon's native side reads, taken once as the reload ends; Java isn't asked for it again. */
public final class ClientResources {
    private ClientResources() {}

    private static volatile byte[] exported = new byte[0];
    private static volatile java.util.Set<net.minecraft.world.level.block.state.BlockState> bakeStates = java.util.Set.of();

    /** A listener whose whole work is for a GPU this client hasn't: it reaches the reload's barrier with nothing prepared and applies nothing. */
    public static CompletableFuture<Void> skipped(PreparableReloadListener.PreparationBarrier barrier) {
        return barrier.wait(Unit.INSTANCE).thenAccept(unit -> {});
    }

    /** The export, as `client_resources` hands it to Rust: empty until the reload has run. */
    static byte[] export() {
        return exported;
    }

    /** Streams the code-built block states' baked quads to `out` as `BakedWriter` writes them, for the game's generated content; the set is dropped after. */
    static byte[] baked(Path out) throws IOException {
        ResourceTextures.export(out.resolveSibling(out.getFileName().toString().replaceFirst("\\.[^.]+$", ".textures")));
        var states = bakeStates;
        bakeStates = java.util.Set.of();
        if (states.isEmpty()) return new byte[] {1};
        var models = Minecraft.getInstance().getModelManager().getBlockStateModelSet();
        var n = dev.horizonlab.facts.BakedWriter.write(out, models::get, states::contains);
        System.err.println("Horizon: baked " + n[0] + " blocks, " + n[1] + " states, " + n[2] + " quads for the content");
        return new byte[] {1};
    }

    /** Writes the export, then frees the stitched pixels nothing reads again. */
    @SuppressWarnings("unchecked")
    static void take() throws IOException, ReflectiveOperationException {
        var minecraft = Minecraft.getInstance();
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        var states = ClientModels.codeBuiltStates();
        out.writeInt(states.size());
        for (var state : states) out.writeInt(Block.BLOCK_STATE_REGISTRY.getId(state));
        bakeStates = states;
        var drawn = ClientModels.foreignItems();
        out.writeInt(drawn.size());
        for (var id : drawn) text(out, id.toString());
        // The items mods registered decorators for (`RegisterItemDecorationsEvent`), as `ItemDecoratorHandler.of` looks them up.
        var lookup = net.neoforged.neoforge.client.ItemDecoratorHandler.class.getDeclaredField("DECORATOR_LOOKUP");
        lookup.setAccessible(true);
        var decorated = new ArrayList<Identifier>();
        for (var item : ((java.util.Map<Item, ?>) lookup.get(null)).keySet()) decorated.add(BuiltInRegistries.ITEM.getKey(item));
        out.writeInt(decorated.size());
        for (var id : decorated) text(out, id.toString());
        var atlases = new TreeMap<Identifier, TextureAtlas>();
        minecraft.getAtlasManager().forEach(atlases::put);
        out.writeInt(atlases.size());
        int sprites = 0;
        long freed = 0;
        var report = new StringBuilder();
        for (var entry : atlases.entrySet()) {
            var atlas = entry.getValue();
            Sprites.sources(entry.getKey(), atlas);
            var view = (Sprites.View) atlas.getTextureView();
            var names = new ArrayList<>(atlas.getTextures().keySet());
            // The missing sprite is generated, not a file: Rust has its own.
            names.remove(net.minecraft.client.renderer.texture.MissingTextureAtlasSprite.getLocation());
            names.sort(null);
            text(out, entry.getKey().toString());
            text(out, atlas.location().toString());
            out.writeInt(view.width());
            out.writeInt(view.height());
            out.writeInt(names.size());
            for (var name : names) text(out, Sprites.file(atlas.location(), name).toString());
            sprites += names.size();
            report.append("\n  ").append(atlas.location()).append(' ').append(view.width()).append('x').append(view.height()).append(": ").append(names.size()).append(" sprites");
            freed += Sprites.free(atlas);
        }
        exported = bytes.toByteArray();
        System.err.println("Horizon: the client's resource reload stitched " + atlases.size() + " atlases (" + sprites + " sprites), " + states.size()
                + " block states with code-built models, " + drawn.size() + " items only their mods' code draws; freed " + freed / 1048576 + " MB of sprite mip levels" + report);
    }

    private static void text(DataOutputStream out, String value) throws IOException {
        byte[] text = value.getBytes(java.nio.charset.StandardCharsets.UTF_8);
        out.writeInt(text.length);
        out.write(text);
    }
}
