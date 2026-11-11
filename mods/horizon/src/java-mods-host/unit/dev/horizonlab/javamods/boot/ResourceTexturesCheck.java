package dev.horizonlab.javamods.boot;

import com.mojang.blaze3d.platform.NativeImage;
import java.nio.file.Files;
import java.nio.file.Path;

public final class ResourceTexturesCheck {
    public static void main(String[] args) throws Exception {
        Path root = Files.createTempDirectory(Path.of("."), "texture-check-");
        Sprites.View first;
        try (var image = new NativeImage(2, 1, false)) {
            image.setPixel(0, 0, 0xffff0000);
            image.setPixel(1, 0, 0x8000ff00);
            first = ResourceTextures.load(image, true, false);
        }
        try {
            if (first.isClosed() || first.width() != 2 || first.height() != 1 || !first.repeat()) throw new AssertionError("texture view");
            ResourceTextures.export(root.resolve("first"));
            Path png = root.resolve("first/assets").resolve(first.id().getNamespace()).resolve(first.id().getPath());
            try (var in = Files.newInputStream(png); var decoded = NativeImage.read(in)) {
                if (decoded.getPixel(0, 0) != 0xffff0000 || decoded.getPixel(1, 0) != 0x8000ff00) throw new AssertionError("pixels or alpha lost");
            }
            Sprites.View replacement;
            try (var image = new NativeImage(1, 1, false)) {
                image.setPixel(0, 0, 0xff0000ff);
                replacement = ResourceTextures.load(image, false, true);
            }
            try {
                first.close();
                first.close();
                if (!first.isClosed() || replacement.isClosed() || replacement.repeat()) throw new AssertionError("replacement lifetime");
                if (first.id().equals(replacement.id())) throw new AssertionError("changed pixels must invalidate native cache");
                ResourceTextures.export(root.resolve("first"));
                if (Files.exists(root.resolve("first/assets").resolve(first.id().getNamespace()).resolve(first.id().getPath()))) throw new AssertionError("closed texture exported");
                Path next = root.resolve("first/assets").resolve(replacement.id().getNamespace()).resolve(replacement.id().getPath());
                try (var in = Files.newInputStream(next); var decoded = NativeImage.read(in)) {
                    if (decoded.getWidth() != 1 || decoded.getPixel(0, 0) != 0xff0000ff) throw new AssertionError("replacement pixels");
                }
                var metadata = com.google.gson.JsonParser.parseString(Files.readString(next.resolveSibling(next.getFileName() + ".mcmeta"))).getAsJsonObject().getAsJsonObject("texture");
                if (!metadata.get("blur").getAsBoolean() || !metadata.get("clamp").getAsBoolean()) throw new AssertionError("sampler metadata");
            } finally {
                replacement.close();
            }
            ResourceTextures.export(root.resolve("empty"));
            if (Files.exists(root.resolve("empty"))) throw new AssertionError("released texture retained");
        } finally {
            first.close();
            try (var paths = Files.walk(root)) {
                for (var path : paths.sorted(java.util.Comparator.reverseOrder()).toList()) Files.delete(path);
            }
        }
        System.out.println("ResourceTexturesCheck ok");
    }
}
