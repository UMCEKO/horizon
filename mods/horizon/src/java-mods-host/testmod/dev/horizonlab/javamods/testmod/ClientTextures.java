package dev.horizonlab.javamods.testmod;

import com.mojang.blaze3d.platform.NativeImage;
import net.minecraft.client.Minecraft;
import net.minecraft.client.renderer.texture.ReloadableTexture;
import net.minecraft.client.renderer.texture.TextureContents;
import net.minecraft.resources.Identifier;
import net.minecraft.server.packs.resources.ResourceManager;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.event.lifecycle.FMLClientSetupEvent;

final class ClientTextures {
    private ClientTextures() {}

    static void register(IEventBus bus) {
        bus.addListener((FMLClientSetupEvent event) -> event.enqueueWork(() -> {
            var id = Identifier.fromNamespaceAndPath("horizon_java_test", "textures/gui/reload_probe.png");
            var texture = new ReloadableTexture(id) {
                private int loads;

                @Override
                public TextureContents loadContents(ResourceManager resources) {
                    var image = new NativeImage(++loads, 1, false);
                    for (int x = 0; x < loads; x++) image.setPixel(x, 0, 0x80402010);
                    return new TextureContents(image, null);
                }
            };
            var manager = Minecraft.getInstance().getTextureManager();
            manager.registerAndLoad(id, texture);
            var old = texture.getTexture();
            if (old.getWidth(0) != 1) throw new AssertionError("initial texture width");
            manager.registerAndLoad(id, texture);
            if (!old.isClosed() || texture.getTexture().isClosed() || texture.getTexture().getWidth(0) != 2) throw new AssertionError("texture reload lifetime");
            System.out.println("CHECK client_texture_reload true");
        }));
    }
}
