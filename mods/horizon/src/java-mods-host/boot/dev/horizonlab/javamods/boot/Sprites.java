package dev.horizonlab.javamods.boot;

import com.mojang.renderpearl.api.GpuFormat;
import com.mojang.renderpearl.api.textures.GpuTexture;
import com.mojang.renderpearl.api.textures.GpuTextureView;
import java.io.InputStream;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.texture.AbstractTexture;
import net.minecraft.client.renderer.texture.SpriteContents;
import net.minecraft.client.renderer.texture.TextureAtlas;
import net.minecraft.client.renderer.texture.TextureAtlasSprite;
import net.minecraft.client.resources.metadata.animation.AnimationMetadataSection;
import net.minecraft.client.resources.metadata.gui.GuiMetadataSection;
import net.minecraft.data.AtlasIds;
import net.minecraft.resources.Identifier;
import net.minecraft.server.packs.metadata.MetadataSectionType;

/** Textures and sprites without a GPU: each sprite is its own texture, named by its file, which Rust finds in its own atlas. */
public final class Sprites {
    private Sprites() {}

    /** The id of a texture's file stands for its pixels. */
    public record View(Identifier id, int width, int height) implements GpuTextureView, GpuTexture {
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
    private static final Map<Identifier, TextureAtlas> ATLASES = new java.util.concurrent.ConcurrentHashMap<>();
    private static final Map<Identifier, Map<Identifier, TextureAtlasSprite>> SPRITES = new java.util.concurrent.ConcurrentHashMap<>();
    private static final Identifier MISSING = Identifier.withDefaultNamespace("missingno");

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

    public static AbstractTexture texture(Identifier id) {
        // An atlas's own texture (`textures/atlas/blocks.png`) is that atlas, as mods cast it to one.
        String path = id.getPath();
        if (path.startsWith("textures/atlas/") && path.endsWith(".png")) {
            return atlas(id.withPath(path.substring("textures/atlas/".length(), path.length() - ".png".length())));
        }
        return TEXTURES.computeIfAbsent(id, file -> {
            try {
                // A sprite's own file stands as its atlas, since mods cast a sprite's atlas texture to one.
                TextureAtlas texture = GameBridge.construct(TextureAtlas.class, Object.class.getDeclaredConstructor());
                var location = TextureAtlas.class.getDeclaredField("location");
                location.setAccessible(true);
                location.set(texture, file);
                int[] size = size(file);
                var view = new View(file, size[0], size[1]);
                var f = AbstractTexture.class.getDeclaredField("textureView");
                f.setAccessible(true);
                f.set(texture, view);
                var g = AbstractTexture.class.getDeclaredField("texture");
                g.setAccessible(true);
                g.set(texture, view);
                return texture;
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        });
    }

    public static TextureAtlas atlas(Identifier id) {
        return ATLASES.computeIfAbsent(id, key -> {
            try {
                TextureAtlas atlas = GameBridge.construct(TextureAtlas.class, Object.class.getDeclaredConstructor());
                var f = TextureAtlas.class.getDeclaredField("location");
                f.setAccessible(true);
                f.set(atlas, key);
                Identifier sheet = sheet(key);
                if (sheet != null) {
                    // The block and item atlases draw as their own texture, whose sprites `indexed` tells apart.
                    var view = new View(sheet, 1, 1);
                    set(AbstractTexture.class, atlas, "textureView", view);
                    set(AbstractTexture.class, atlas, "texture", view);
                }
                return atlas;
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
        });
    }

    /** Where an atlas's sprites come from, as its definition's directory source names them. */
    private static Identifier file(Identifier atlas, Identifier sprite) {
        if (atlas.getPath().endsWith(".png")) return atlas;
        String folder = atlas.equals(AtlasIds.GUI) ? "gui/sprites/"
                : atlas.equals(AtlasIds.PAINTINGS) ? "painting/"
                : atlas.equals(AtlasIds.PARTICLES) ? "particle/"
                : atlas.equals(AtlasIds.MAP_DECORATIONS) ? "map/decorations/"
                : "";
        return sprite.withPath(path -> "textures/" + folder + path + ".png");
    }

    public static TextureAtlasSprite sprite(Identifier atlas, Identifier id) {
        return sprite(atlas, id, null);
    }

    private static final List<TextureAtlasSprite> INDEXED = new java.util.concurrent.CopyOnWriteArrayList<>();

    /** The texture of the block or item atlas, for its id or its texture's; null for every other atlas. */
    private static Identifier sheet(Identifier atlas) {
        if (atlas.equals(AtlasIds.BLOCKS) || atlas.equals(TextureAtlas.LOCATION_BLOCKS)) return TextureAtlas.LOCATION_BLOCKS;
        if (atlas.equals(AtlasIds.ITEMS) || atlas.equals(TextureAtlas.LOCATION_ITEMS)) return TextureAtlas.LOCATION_ITEMS;
        return null;
    }

    /** The block or item atlas sprite whose U span starts at `u`'s whole part, as each sits one unit right of the last. */
    public static TextureAtlasSprite indexed(float u) {
        int index = (int) Math.floor(u);
        return index >= 0 && index < INDEXED.size() ? INDEXED.get(index) : null;
    }

    /** A sprite's texture file, as the GUI recorder names textures. */
    public static String textureFile(TextureAtlasSprite sprite) {
        return sprite.contents().name().withPath(path -> "textures/" + path + ".png").toString();
    }

    /** As `sprite`, but as baked models see it: in the atlas texture `sheet`, as the baker checks, each sprite one unit of U wide. */
    public static TextureAtlasSprite sprite(Identifier requested, Identifier id, Identifier requestedSheet) {
        Identifier sheet = sheet(requested) != null ? sheet(requested) : requestedSheet;
        Identifier atlas = sheet(requested) == null ? requested : sheet.equals(TextureAtlas.LOCATION_BLOCKS) ? AtlasIds.BLOCKS : AtlasIds.ITEMS;
        Identifier key = sheet == null ? atlas : atlas.withSuffix("/baked");
        return SPRITES.computeIfAbsent(key, a -> new java.util.concurrent.ConcurrentHashMap<>()).computeIfAbsent(id, sprite -> {
            try {
                Identifier file = file(atlas, sprite);
                int[] size = size(file);
                List<MetadataSectionType.WithValue<?>> metadata = new ArrayList<>();
                var resource = Minecraft.getInstance().getResourceManager().getResource(file);
                if (resource.isPresent()) {
                    var meta = resource.get().metadata();
                    meta.getSection(GuiMetadataSection.TYPE).ifPresent(gui -> metadata.add(new MetadataSectionType.WithValue<>(GuiMetadataSection.TYPE, gui)));
                    var animation = meta.getSection(AnimationMetadataSection.TYPE);
                    if (animation.isPresent()) {
                        var frame = animation.get().calculateFrameSize(size[0], size[1]);
                        size = new int[] {frame.width(), frame.height()};
                    }
                }
                SpriteContents contents = GameBridge.construct(SpriteContents.class, Object.class.getDeclaredConstructor());
                set(SpriteContents.class, contents, "name", sprite);
                set(SpriteContents.class, contents, "width", size[0]);
                set(SpriteContents.class, contents, "height", size[1]);
                set(SpriteContents.class, contents, "additionalMetadata", metadata);
                set(SpriteContents.class, contents, "transparency", transparency(file, size[0], size[1]));
                var make = TextureAtlasSprite.class.getDeclaredConstructor(Identifier.class, SpriteContents.class, int.class, int.class, int.class, int.class, int.class);
                make.setAccessible(true);
                // One sprite a texture: its coordinates span the file, which Rust maps onto where its atlas put it.
                if (sheet == null) return make.newInstance(file, contents, size[0], size[1], 0, 0, 0);
                synchronized (INDEXED) {
                    var placed = make.newInstance(sheet, contents, size[0], size[1], INDEXED.size() * size[0], 0, 0);
                    INDEXED.add(placed);
                    return placed;
                }
            } catch (Exception e) {
                throw new IllegalStateException(e);
            }
        });
    }

    private static final Map<Identifier, java.util.Set<Identifier>> LISTED = new java.util.concurrent.ConcurrentHashMap<>();

    /** The sprite ids an atlas's definition lists (`SpriteSourceList.list`'s, without loading their images). */
    private static java.util.Set<Identifier> listed(Identifier atlas) {
        return LISTED.computeIfAbsent(atlas, id -> {
            var manager = Minecraft.getInstance().getResourceManager();
            var ids = new java.util.HashSet<Identifier>();
            var output = new net.minecraft.client.renderer.texture.atlas.SpriteSource.Output() {
                @Override
                public void add(Identifier sprite, net.minecraft.client.renderer.texture.atlas.SpriteSource.DiscardableLoader loader) {
                    ids.add(sprite);
                }

                @Override
                public void removeAll(java.util.function.Predicate<Identifier> predicate) {
                    ids.removeIf(predicate);
                }
            };
            try {
                var sources = net.minecraft.client.renderer.texture.atlas.SpriteSourceList.class.getDeclaredField("sources");
                sources.setAccessible(true);
                @SuppressWarnings("unchecked")
                var list = (List<net.minecraft.client.renderer.texture.atlas.SpriteSource>) sources.get(net.minecraft.client.renderer.texture.atlas.SpriteSourceList.load(manager, id));
                list.forEach(source -> source.run(manager, output));
            } catch (ReflectiveOperationException e) {
                throw new IllegalStateException(e);
            }
            return java.util.Set.copyOf(ids);
        });
    }

    /** The sprite, or null when the atlas's definition doesn't list it, as a stitched atlas lacks it. */
    public static TextureAtlasSprite spriteIfPresent(Identifier atlas, Identifier id, Identifier sheet) {
        if (!listed(atlas).contains(id)) return null;
        return sprite(atlas, id, sheet);
    }

    /** Whether the sprite's first frame has clear or part-clear texels, which picks its block layer. */
    private static com.mojang.blaze3d.platform.Transparency transparency(Identifier file, int width, int height) {
        var resource = Minecraft.getInstance().getResourceManager().getResource(file);
        if (resource.isEmpty()) return com.mojang.blaze3d.platform.Transparency.NONE;
        try (InputStream in = resource.get().open()) {
            var image = javax.imageio.ImageIO.read(in);
            boolean clear = false, partly = false;
            for (int y = 0; y < Math.min(height, image.getHeight()); y++) {
                for (int x = 0; x < Math.min(width, image.getWidth()); x++) {
                    int alpha = image.getRGB(x, y) >>> 24;
                    if (alpha == 0) clear = true;
                    else if (alpha < 255) partly = true;
                }
            }
            return com.mojang.blaze3d.platform.Transparency.of(clear, partly);
        } catch (Exception e) {
            return com.mojang.blaze3d.platform.Transparency.NONE;
        }
    }

    public static TextureAtlasSprite missing(Identifier atlas) {
        return sprite(atlas, MISSING);
    }

    private static void set(Class<?> owner, Object target, String field, Object value) throws ReflectiveOperationException {
        var f = owner.getDeclaredField(field);
        f.setAccessible(true);
        f.set(target, value);
    }
}
