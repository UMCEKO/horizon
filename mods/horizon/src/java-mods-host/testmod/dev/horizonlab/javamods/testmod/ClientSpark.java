package dev.horizonlab.javamods.testmod;

import net.minecraft.client.Minecraft;
import net.minecraft.client.multiplayer.ClientLevel;
import com.mojang.blaze3d.vertex.PoseStack;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.Camera;
import net.minecraft.client.particle.Particle;
import net.minecraft.client.particle.ParticleEngine;
import net.minecraft.client.particle.ParticleGroup;
import net.minecraft.client.particle.ParticleRenderType;
import net.minecraft.client.particle.SingleQuadParticle;
import net.minecraft.client.renderer.culling.Frustum;
import net.minecraft.client.renderer.rendertype.RenderTypes;
import net.minecraft.client.renderer.state.level.ParticleGroupRenderState;
import net.minecraft.client.renderer.texture.TextureAtlasSprite;
import net.minecraft.core.particles.ParticleTypes;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.client.event.RegisterParticleGroupsEvent;
import net.neoforged.neoforge.client.event.RegisterParticleProvidersEvent;
import net.neoforged.neoforge.common.NeoForge;

/** A mod's particle with its own provider, and the wand spawning it beside a vanilla flame. */
final class ClientSpark {
    static final ParticleRenderType BEAM_GROUP = new ParticleRenderType("HORIZON_TEST_BEAM", "HB");

    private ClientSpark() {}

    static void register(IEventBus bus) {
        bus.addListener((RegisterParticleProvidersEvent event) -> event.registerSpriteSet(TestMod.SPARK.get(),
                sprites -> (options, level, x, y, z, xa, ya, za, random) -> new Spark(level, x, y, z, sprites.first())));
        bus.addListener((RegisterParticleGroupsEvent event) -> event.register(BEAM_GROUP, BeamGroup::new));
        bus.addListener((RegisterParticleProvidersEvent event) -> event.registerSpecial(TestMod.BEAM.get(), (options, level, x, y, z, xa, ya, za, random) -> new Beam(level, x, y, z)));
        bus.addListener((net.neoforged.neoforge.client.extensions.common.RegisterClientExtensionsEvent event) -> event.registerBlock(new Burst(), Battery.BLOCK.get()));
        NeoForge.EVENT_BUS.addListener((ClientTickEvent.Post event) -> {
            var minecraft = Minecraft.getInstance();
            var level = minecraft.level;
            if (level == null || minecraft.player == null || !(minecraft.player.getMainHandItem().getItem() instanceof TestMod.Wand) || level.getGameTime() % 20 != 0) return;
            level.addParticle(TestMod.SPARK.get(), 8.5, 102, 8.5, 0, 0, 0);
            level.addParticle(ParticleTypes.FLAME, 9.5, 102, 8.5, 0, 0, 0);
            if (level.getGameTime() % 40 == 0) level.addParticle(TestMod.BEAM.get(), 10.5, 101, 8.5, 0, 0, 0);
        });
    }

    static final class Beam extends Particle {
        Beam(ClientLevel level, double x, double y, double z) {
            super(level, x, y, z);
            lifetime = 40;
            hasPhysics = false;
        }

        double[] relative(float partial, Camera camera) {
            var at = camera.position();
            return new double[] {xo + (x - xo) * partial - at.x, yo + (y - yo) * partial - at.y, zo + (z - zo) * partial - at.z};
        }

        @Override
        public ParticleRenderType getGroup() {
            return BEAM_GROUP;
        }
    }

    static final class BeamGroup extends ParticleGroup<Beam> {
        BeamGroup(ParticleEngine engine) {
            super(engine);
        }

        @Override
        public ParticleGroupRenderState extractRenderState(Frustum frustum, Camera camera, float partial) {
            List<double[]> beams = new ArrayList<>();
            for (var beam : particles) beams.add(beam.relative(partial, camera));
            return (collector, cameraState) -> collector.submitCustomGeometry(new PoseStack(), RenderTypes.debugQuads(), (pose, consumer) -> {
                for (var at : beams) {
                    float x = (float) at[0], y = (float) at[1], z = (float) at[2];
                    consumer.addVertex(pose, x - 0.05F, y, z).setColor(0x8000FF00);
                    consumer.addVertex(pose, x + 0.05F, y, z).setColor(0x8000FF00);
                    consumer.addVertex(pose, x + 0.05F, y + 2, z).setColor(0x8000FF00);
                    consumer.addVertex(pose, x - 0.05F, y + 2, z).setColor(0x8000FF00);
                }
            });
        }
    }

    /** The battery's own destroy effect: four sparks at its centre, in place of vanilla's crumbs. */
    static final class Burst implements net.neoforged.neoforge.client.extensions.common.IClientBlockExtensions {
        @Override
        public boolean addDestroyEffects(net.minecraft.world.level.block.state.BlockState state, net.minecraft.world.level.Level level, net.minecraft.core.BlockPos pos, net.minecraft.client.particle.ParticleEngine engine) {
            for (int i = 0; i < 4; i++) level.addParticle(TestMod.SPARK.get(), pos.getX() + 0.5, pos.getY() + 0.5, pos.getZ() + 0.5, 0, 0, 0);
            return true;
        }
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
