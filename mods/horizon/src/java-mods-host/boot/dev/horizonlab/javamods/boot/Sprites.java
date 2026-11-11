package dev.horizonlab.javamods.boot;

import com.mojang.renderpearl.api.GpuFormat;
import com.mojang.renderpearl.api.textures.GpuTexture;
import com.mojang.renderpearl.api.textures.GpuTextureView;
import java.io.InputStream;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.texture.AbstractTexture;
import net.minecraft.client.renderer.texture.TextureAtlas;
import net.minecraft.client.renderer.texture.TextureAtlasSprite;
import net.minecraft.client.renderer.texture.atlas.SpriteSource;
import net.minecraft.client.renderer.texture.atlas.SpriteSourceList;
import net.minecraft.client.renderer.texture.atlas.sources.DirectoryLister;
import net.minecraft.client.renderer.texture.atlas.sources.SingleFile;
import net.minecraft.client.resources.metadata.texture.TextureMetadataSection;
import net.minecraft.resources.FileToIdConverter;
import net.minecraft.resources.Identifier;

/** Textures without a GPU: the atlases are stitched by the client's resource reload and stand for their texture by id; any other texture stands for its file. */
public final class Sprites {
    private Sprites() {}

    /** The id of a texture stands for its pixels, which Rust has; `repeat` is its sampler's address mode (atlases clamp). */
    public static class View implements GpuTextureView, GpuTexture {
        private final Identifier id;
        private final int width;
        private final int height;
        private final boolean repeat;

        public View(Identifier id, int width, int height, boolean repeat) {
            this.id = id;
            this.width = width;
            this.height = height;
            this.repeat = repeat;
        }

        public Identifier id() { return id; }
        public int width() { return width; }
        public int height() { return height; }
        public boolean repeat() { return repeat; }
        @Override
        public boolean isClosed() {
            return false;
        }

        @Override
        public GpuTexture texture() {
            return this;
        }

        @Override
        public int baseMipLevel() {
            return 0;
        }

        @Override
        public int mipLevels() {
            return 1;
        }

        @Override
        public int getMipLevels() {
            return 1;
        }

        @Override
        public int getWidth(int mipLevel) {
            return width;
        }

        @Override
        public int getHeight(int mipLevel) {
            return height;
        }

        @Override
        public int getDepthOrLayers() {
            return 1;
        }

        @Override
        public GpuFormat getFormat() {
            return GpuFormat.RGBA8_UNORM;
        }

        @Override
        public int usage() {
            return GpuTexture.USAGE_TEXTURE_BINDING;
        }

        @Override
        public String getLabel() {
            return id.toString();
        }

        @Override
        public void close() {}
    }

    private static final Map<Identifier, TextureAtlas> TEXTURES = new java.util.concurrent.ConcurrentHashMap<>();

    /** Width and height from a PNG's header; 16 a side when the file isn't there. */
    private static int[] size(Identifier file) {
        var resource = Minecraft.getInstance().getResourceManager().getResource(file);
        if (resource.isEmpty()) return new int[] {16, 16};
        try (InputStream in = resource.get().open()) {
            byte[] head = in.readNBytes(24);
            if (head.length < 24) return new int[] {16, 16};
            int w = (head[16] & 255) << 24 | (head[17] & 255) << 16 | (head[18] & 255) << 8 | head[19] & 255;
            int h = (head[20] & 255) << 24 | (head[21] & 255) << 16 | (head[22] & 255) << 8 | head[23] & 255;
            return new int[] {w, h};
        } catch (Exception e) {
            return new int[] {16, 16};
        }
    }

    /** `TextureContents.clamp` of a texture file's metadata. */
    private static boolean clamps(Identifier file) {
        var resource = Minecraft.getInstance().getResourceManager().getResource(file);
        try {
            return resource.isPresent() && resource.get().metadata().getSection(TextureMetadataSection.TYPE).map(TextureMetadataSection::clamp).orElse(false);
        } catch (java.io.IOException e) {
            throw new java.io.UncheckedIOException(e);
        }
    }

    /** A texture no atlas registered: its file, standing as an atlas since mods cast a texture to one. */
    public static AbstractTexture texture(Identifier id) {
        return TEXTURES.computeIfAbsent(id, file -> {
            try {
                TextureAtlas texture = GameBridge.construct(TextureAtlas.class, Object.class.getDeclaredConstructor());
                set(TextureAtlas.class, texture, "location", file);
                int[] size = size(file);
                var view = new View(file, size[0], size[1], !clamps(file));
                set(AbstractTexture.class, texture, "textureView", view);
                set(AbstractTexture.class, texture, "texture", view);
                return texture;
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        });
    }

    /** A texture object whose constructor, which allocates it on a GPU, never ran. */
    public static <T> T unallocated(Class<T> type) {
        try {
            return GameBridge.construct(type, Object.class.getDeclaredConstructor());
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    /** What `TextureAtlas.createTexture` would allocate on a GPU: the atlas's texture, as its id and size. */
    public static void stitched(TextureAtlas atlas, Identifier location, int width, int height) {
        var view = new View(location, width, height, false);
        try {
            set(AbstractTexture.class, atlas, "textureView", view);
            set(AbstractTexture.class, atlas, "texture", view);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
        INDEX.remove(location);
    }

    /** A spot in an atlas: the sprite there, its texture file, and the spot within the sprite. */
    public record Located(TextureAtlasSprite sprite, String file, float u, float v) {}

    private record Index(TextureAtlas atlas, int width, int height, Map<Integer, List<TextureAtlasSprite>> cells) {}

    private static final Map<Identifier, Index> INDEX = new java.util.concurrent.ConcurrentHashMap<>();
    private static final Map<Identifier, Map<Identifier, Identifier>> FILES = new java.util.concurrent.ConcurrentHashMap<>();
    private static final int CELL = 16;

    /** The sprite's first texel; `getX` is its padded slot, which starts `padding` texels earlier. */
    private static int left(TextureAtlasSprite sprite, int width) {
        return Math.round(sprite.getU0() * width);
    }

    private static int top(TextureAtlasSprite sprite, int height) {
        return Math.round(sprite.getV0() * height);
    }

    private static Index index(Identifier texture) {
        return INDEX.computeIfAbsent(texture, id -> {
            if (!(Minecraft.getInstance().getTextureManager().getTexture(id) instanceof TextureAtlas atlas) || !(atlas.getTextureView() instanceof View view)) return null;
            // A plain texture stands as an atlas with nothing stitched in it.
            if (TEXTURES.get(id) == atlas) return null;
            var cells = new java.util.HashMap<Integer, List<TextureAtlasSprite>>();
            int columns = (view.width() + CELL - 1) / CELL;
            for (var sprite : atlas.getTextures().values()) {
                int sx = left(sprite, view.width()), sy = top(sprite, view.height());
                int x0 = sx / CELL, y0 = sy / CELL;
                int x1 = (sx + sprite.contents().width() - 1) / CELL, y1 = (sy + sprite.contents().height() - 1) / CELL;
                for (int y = y0; y <= y1; y++) for (int x = x0; x <= x1; x++) cells.computeIfAbsent(y * columns + x, c -> new ArrayList<>(1)).add(sprite);
            }
            return new Index(atlas, view.width(), view.height(), cells);
        });
    }

    /** The sprite of a stitched atlas at `(u, v)` and the spot within it; null when `texture` isn't an atlas or nothing is there. */
    public static Located locate(Identifier texture, float u, float v) {
        var index = index(texture);
        if (index == null) return null;
        int x = (int) Math.floor(u * index.width()), y = (int) Math.floor(v * index.height());
        var here = index.cells().get(y / CELL * ((index.width() + CELL - 1) / CELL) + x / CELL);
        if (here == null) return null;
        for (var sprite : here) {
            int sx = left(sprite, index.width()), sy = top(sprite, index.height());
            if (x < sx || y < sy || x >= sx + sprite.contents().width() || y >= sy + sprite.contents().height()) continue;
            float lu = (u - sprite.getU0()) / (sprite.getU1() - sprite.getU0());
            float lv = (v - sprite.getV0()) / (sprite.getV1() - sprite.getV0());
            return new Located(sprite, file(texture, sprite.contents().name()).withPath(path -> "textures/" + path + ".png").toString(), lu, lv);
        }
        return null;
    }

    /** Each sprite's texture as the atlas definition's sources name it (`DirectoryLister`, `SingleFile`), run in their order; a generated sprite has none. */
    static void sources(Identifier definition, TextureAtlas atlas) throws ReflectiveOperationException {
        var manager = Minecraft.getInstance().getResourceManager();
        var field = SpriteSourceList.class.getDeclaredField("sources");
        field.setAccessible(true);
        @SuppressWarnings("unchecked")
        var sources = (List<SpriteSource>) field.get(SpriteSourceList.load(manager, definition));
        var files = new java.util.HashMap<Identifier, Identifier>();
        var output = new SpriteSource.Output() {
            @Override
            public void add(Identifier id, SpriteSource.DiscardableLoader loader) {
                files.remove(id);
            }

            @Override
            public void removeAll(java.util.function.Predicate<Identifier> predicate) {
                files.keySet().removeIf(predicate);
            }
        };
        for (var source : sources) {
            if (source instanceof DirectoryLister directory) {
                var converter = new FileToIdConverter("textures/" + directory.sourcePath(), ".png");
                for (var file : converter.listMatchingResources(manager).keySet()) files.put(converter.fileToId(file).withPrefix(directory.idPrefix()), underTextures(file));
            } else if (source instanceof SingleFile single) {
                var file = single.resourceId().withPath(path -> "textures/" + path + ".png");
                if (manager.getResource(file).isPresent()) files.put(single.spriteId().orElse(single.resourceId()), single.resourceId());
            } else {
                source.run(manager, output);
            }
        }
        FILES.put(atlas.location(), files);
    }

    private static Identifier underTextures(Identifier file) {
        return file.withPath(path -> path.substring("textures/".length(), path.length() - ".png".length()));
    }

    /** A sprite as Rust names what it draws: its texture's path under `textures/`, or its own id when the atlas generated it. */
    static Identifier file(Identifier atlasTexture, Identifier sprite) {
        return FILES.getOrDefault(atlasTexture, Map.of()).getOrDefault(sprite, sprite);
    }

    /** Frees each sprite's mip levels once the atlas is stitched: only a GPU upload reads them, and Java keeps the original image NeoForge hands mods (`getOriginalImage`). */
    static long free(TextureAtlas atlas) {
        long bytes = 0;
        for (var sprite : atlas.getTextures().values()) {
            var contents = sprite.contents();
            var levels = contents.byMipLevel;
            for (int level = 1; level < levels.length; level++) {
                bytes += (long) levels[level].getWidth() * levels[level].getHeight() * 4;
                levels[level].close();
            }
            // Level 0 is the original image; vanilla reads it only for transparency, which the mask answers.
            if (levels.length > 0 && levels[0] == contents.getOriginalImage() && SpriteMasks.keep(contents, levels[0])) {
                bytes += (long) levels[0].getWidth() * levels[0].getHeight() * 4;
                levels[0].close();
                contents.byMipLevel = new com.mojang.blaze3d.platform.NativeImage[0];
            } else if (levels.length > 1) {
                contents.byMipLevel = new com.mojang.blaze3d.platform.NativeImage[] {levels[0]};
            }
        }
        return bytes;
    }

    private static void set(Class<?> owner, Object target, String field, Object value) throws ReflectiveOperationException {
        var f = owner.getDeclaredField(field);
        f.setAccessible(true);
        f.set(target, value);
    }
}
