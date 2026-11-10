package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.function.UnaryOperator;
import net.minecraft.client.resources.sounds.SimpleSoundInstance;
import net.minecraft.client.resources.sounds.SoundInstance;
import net.minecraft.resources.Identifier;
import net.minecraft.sounds.SoundSource;
import net.minecraft.util.RandomSource;
import net.neoforged.neoforge.client.event.sound.PlaySoundEvent;

/** `PlaySoundEvent` for the sounds native plays: native asks right before it plays one, at `SoundEngine.play`'s point, only when a loaded mod listens, and applies the answer. */
final class NativeSounds {
    private NativeSounds() {}

    static final byte CANCEL = 0;
    static final byte AS_ASKED = 1;
    static final byte CHANGED = 2;

    /** Whether a loaded mod listens; decided once when the mods have loaded. */
    static volatile boolean listened;

    /** Whether the bus has a listener of the event. */
    static boolean listened(net.neoforged.bus.api.IEventBus bus) {
        try {
            var list = net.neoforged.bus.EventBus.class.getDeclaredMethod("getListenerList", Class.class);
            list.setAccessible(true);
            return ((net.neoforged.bus.ListenerList) list.invoke(bus, PlaySoundEvent.class)).getListeners().length > 0;
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    static void decide() {
        listened = listened(net.neoforged.neoforge.common.NeoForge.EVENT_BUS);
        System.err.println("Horizon: " + (listened ? "a mod listens to PlaySoundEvent; native asks it before it plays a sound" : "no mod listens to PlaySoundEvent; native plays its sounds unasked"));
    }

    static byte[] listenedReply() {
        return new byte[] {(byte) (listened ? 1 : 0)};
    }

    private static String text(DataInputStream in) throws IOException {
        return new String(in.readNBytes(in.readInt()), StandardCharsets.UTF_8);
    }

    /** The event for one sound native is about to play (name, source, volume, pitch, relative, position, looping, delay, linear), posted through `post`; answers cancel, as asked, or the sound the listeners left. */
    static byte[] ask(DataInputStream in, UnaryOperator<SoundInstance> post) throws IOException {
        Identifier id = Identifier.parse(text(in));
        SoundSource source = SoundSource.values()[in.readInt()];
        float volume = in.readFloat();
        float pitch = in.readFloat();
        boolean relative = in.readBoolean();
        double x = in.readDouble(), y = in.readDouble(), z = in.readDouble();
        boolean looping = in.readBoolean();
        int delay = in.readInt();
        var attenuation = in.readBoolean() ? SoundInstance.Attenuation.LINEAR : SoundInstance.Attenuation.NONE;
        var asked = new SimpleSoundInstance(id, source, volume, pitch, RandomSource.create(), looping, delay, attenuation, x, y, z, relative);
        SoundInstance result = post.apply(asked);
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        if (result == null) {
            out.writeByte(CANCEL);
        } else if (result == asked) {
            out.writeByte(AS_ASKED);
        } else {
            out.writeByte(CHANGED);
            byte[] name = result.getIdentifier().toString().getBytes(StandardCharsets.UTF_8);
            out.writeInt(name.length);
            out.write(name);
            out.writeFloat(ClientSounds.volume(result));
            out.writeFloat(ClientSounds.pitch(result));
            out.writeDouble(result.getX());
            out.writeDouble(result.getY());
            out.writeDouble(result.getZ());
        }
        return bytes.toByteArray();
    }

    static byte[] ask(DataInputStream in) throws IOException {
        return ask(in, sound -> net.neoforged.neoforge.client.ClientHooks.playSound(ClientSounds.engine, sound));
    }
}
