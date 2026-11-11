package dev.horizonlab.javamods.boot;

import io.netty.buffer.ByteBuf;
import io.netty.buffer.Unpooled;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import net.minecraft.client.Minecraft;
import net.minecraft.client.particle.Particle;
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
    private static final StackWalker STACK = StackWalker.getInstance(StackWalker.Option.RETAIN_CLASS_REFERENCE);
    private static int scopes;
    /** The particle `createParticle` is adding for a mod's type, which `add` keeps. */
    public static Particle admitted;
    public static int forwarded;
    public static int dropped;

    /** Opens a scope whose particles are Java's own: pair with `exit` in a finally. */
    public static void enter() {
        scopes++;
    }

    public static void exit() {
        scopes--;
    }

    /** Whether a particle made now is Java's own, with no native copy: a Java-run `animateTick`, a scope, or a mod's class on the stack. */
    public static boolean sourced() {
        return Animate.forwarding() || scopes > 0 || modOnStack();
    }

    /** Whether `ParticleEngine.add` keeps a particle: a mod's class, or made while `sourced`. */
    public static boolean keeps(Class<?> particle) {
        return Shells.isMods(particle) || sourced();
    }

    private static int debugTicks;

    /** The debug line, every 100 window ticks when anything is non-zero. */
    static void debug(Minecraft minecraft) {
        if (++debugTicks < 100) return;
        debugTicks = 0;
        var groups = ((dev.horizonlab.javamods.boot.mixin.ParticleEngineAccess) minecraft.particleEngine).horizon$groups();
        int live = 0;
        for (var group : groups.values()) live += group.size();
        if (live + forwarded + dropped > 0) System.err.println("Horizon: Java particles: " + live + " live in " + groups.size() + " groups, " + forwarded + " forwarded, " + dropped + " dropped");
        forwarded = 0;
        dropped = 0;
    }

    static boolean modOnStack() {
        return STACK.walk(s -> s.anyMatch(f -> Shells.isMods(f.getDeclaringClass())));
    }

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
            forwarded++;
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
