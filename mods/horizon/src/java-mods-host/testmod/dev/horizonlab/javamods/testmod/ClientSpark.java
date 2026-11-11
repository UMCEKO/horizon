package dev.horizonlab.javamods.testmod;

import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.particle.SingleQuadParticle;
import net.minecraft.client.renderer.texture.TextureAtlasSprite;
import net.minecraft.core.particles.ParticleTypes;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.client.event.RegisterParticleProvidersEvent;
import net.neoforged.neoforge.common.NeoForge;

/** A mod's particle with its own provider, and the wand spawning it beside a vanilla flame. */
final class ClientSpark {
    private ClientSpark() {}

    static void register(IEventBus bus) {
        bus.addListener((RegisterParticleProvidersEvent event) -> event.registerSpriteSet(TestMod.SPARK.get(),
                sprites -> (options, level, x, y, z, xa, ya, za, random) -> new Spark(level, x, y, z, sprites.first())));
        NeoForge.EVENT_BUS.addListener((ClientTickEvent.Post event) -> {
            var minecraft = Minecraft.getInstance();
            var level = minecraft.level;
            if (level == null || minecraft.player == null || !(minecraft.player.getMainHandItem().getItem() instanceof TestMod.Wand) || level.getGameTime() % 20 != 0) return;
            level.addParticle(TestMod.SPARK.get(), 8.5, 102, 8.5, 0, 0, 0);
            level.addParticle(ParticleTypes.FLAME, 9.5, 102, 8.5, 0, 0, 0);
        });
    }

    static final class Spark extends SingleQuadParticle {
        Spark(ClientLevel level, double x, double y, double z, TextureAtlasSprite sprite) {
            super(level, x, y, z, sprite);
            lifetime = 20;
            gravity = 0;
            friction = 1;
            hasPhysics = false;
            xd = 0.1;
            yd = 0;
            zd = 0;
            quadSize = 0.25F;
        }

        @Override
        protected Layer getLayer() {
            return Layer.OPAQUE;
        }
    }
}
