package dev.horizonlab.javamods.boot;

import com.mojang.blaze3d.platform.NativeImage;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.HexFormat;
import java.util.IdentityHashMap;
import java.util.Map;
import net.minecraft.resources.Identifier;

public final class ResourceTextures {
    private ResourceTextures() {}

    private record Pixels(Path file, boolean blur) {}
    private static final Map<Sprites.View, Pixels> LIVE = new IdentityHashMap<>();
    private static Path scratch;

    public static synchronized Sprites.View load(NativeImage image, boolean repeat, boolean blur) {
        Path file = null;
        try {
            if (scratch == null) {
                Path cache = Path.of(".horizon-cache");
                Files.createDirectories(cache);
                scratch = Files.createTempDirectory(cache, "texture-upload-");
            }
            file = Files.createTempFile(scratch, "pixels-", ".png");
            image.writeToFile(file);
            String hash = HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(Files.readAllBytes(file)));
            var id = Identifier.fromNamespaceAndPath("horizon_java_textures", "textures/gui/" + hash + (repeat ? "_r" : "_c") + (blur ? "l" : "n") + ".png");
            var view = new Sprites.View(id, image.getWidth(), image.getHeight(), repeat) {
                private volatile boolean closed;

                @Override
                public boolean isClosed() { return closed; }

                @Override
                public void close() {
                    ResourceTextures.release(this);
                    closed = true;
                }
            };
            LIVE.put(view, new Pixels(file, blur));
            return view;
        } catch (IOException | NoSuchAlgorithmException e) {
            if (file != null) try { Files.deleteIfExists(file); } catch (IOException cleanup) { e.addSuppressed(cleanup); }
            throw new IllegalStateException("Cannot capture texture pixels for native rendering", e);
        }
    }

    private static synchronized void release(Sprites.View view) {
        Pixels pixels = LIVE.remove(view);
        Path file = pixels == null ? null : pixels.file();
        if (file != null) try { Files.deleteIfExists(file); } catch (IOException e) { throw new UncheckedIOException(e); }
        if (LIVE.isEmpty() && scratch != null) {
            try { Files.deleteIfExists(scratch); } catch (IOException e) { throw new UncheckedIOException(e); }
            scratch = null;
        }
    }

    public static synchronized void export(Path root) throws IOException {
        if (Files.exists(root)) {
            try (var paths = Files.walk(root)) {
                for (var path : paths.sorted(java.util.Comparator.reverseOrder()).toList()) Files.delete(path);
            }
        }
        if (LIVE.isEmpty()) return;
        Files.createDirectories(root);
        Files.writeString(root.resolve("horizon-mod.toml"), "id = \"horizon_java_textures\"\nname = \"Java texture uploads\"\nversion = \"1\"\napi = \"0.2\"\nside = \"client\"\n");
        for (var entry : LIVE.entrySet()) {
            var id = entry.getKey().id();
            Path target = root.resolve("assets").resolve(id.getNamespace()).resolve(id.getPath());
            Files.createDirectories(target.getParent());
            Files.copy(entry.getValue().file(), target, StandardCopyOption.REPLACE_EXISTING);
            Files.writeString(target.resolveSibling(target.getFileName() + ".mcmeta"), "{\"texture\":{\"blur\":" + entry.getValue().blur() + ",\"clamp\":" + !entry.getKey().repeat() + "}}");
        }
    }
}
