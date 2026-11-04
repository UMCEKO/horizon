package dev.horizonlab.javamods.boot;

import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import net.minecraft.client.Minecraft;
import net.minecraft.core.particles.ParticleOptions;
import net.minecraft.core.particles.ParticleTypes;
import net.minecraft.network.RegistryFriendlyByteBuf;

/** The particles Java-run `animateTick` code makes, for Horizon's particle engine to draw: where, how fast, and the type as `ClientboundLevelParticlesPacket` writes it. */
public final class ClientParticles {
    private ClientParticles() {}

    /** The particle type's wire form; a registry-backed codec in the game, a fake in the unit check. */
    interface Encoder {
        byte[] encode(ParticleOptions options);
    }

    private static final ByteArrayOutputStream BYTES = new ByteArrayOutputStream();
    private static final DataOutputStream OUT = new DataOutputStream(BYTES);
    private static int count;

    /** `ParticleEngine.createParticle`'s arguments, after the level's limiter let them through. */
    public static void add(ParticleOptions options, double x, double y, double z, double xa, double ya, double za) {
        add(ClientParticles::encode, options, x, y, z, xa, ya, za);
    }

    static void add(Encoder encoder, ParticleOptions options, double x, double y, double z, double xa, double ya, double za) {
        try {
            byte[] codec = encoder.encode(options);
            OUT.writeDouble(x);
            OUT.writeDouble(y);
            OUT.writeDouble(z);
            OUT.writeDouble(xa);
            OUT.writeDouble(ya);
            OUT.writeDouble(za);
            OUT.writeInt(codec.length);
            OUT.write(codec);
            count++;
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    private static byte[] encode(ParticleOptions options) {
        ByteBuf buffer = Unpooled.buffer();
        ParticleTypes.STREAM_CODEC.encode(new RegistryFriendlyByteBuf(buffer, Minecraft.getInstance().level.registryAccess()), options);
        byte[] bytes = new byte[buffer.readableBytes()];
        buffer.readBytes(bytes);
        return bytes;
    }

    /** The particles since the last call. */
    static void take(DataOutputStream out) throws IOException {
        out.writeInt(count);
        BYTES.writeTo(out);
        forget();
    }

    static void forget() {
        BYTES.reset();
        count = 0;
    }
}
