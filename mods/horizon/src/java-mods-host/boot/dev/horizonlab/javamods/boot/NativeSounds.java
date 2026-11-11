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

/** `PlaySoundEvent` for the sounds native plays: Rust's listener posts it right before it plays one, at `SoundEngine.play`'s point, and applies the answer. */
final class NativeSounds {
    private NativeSounds() {}

    static final byte CANCEL = 0;
    static final byte AS_ASKED = 1;
    static final byte CHANGED = 2;

    private static String text(DataInputStream in) throws IOException {
        return new String(in.readNBytes(in.readInt()), StandardCharsets.UTF_8);
    }

    /** The event for one sound native is about to play (name, source, volume, pitch, relative, position, looping, delay, linear), posted through `post`; answers cancel, as asked, or the sound the listeners left in the fields it was asked with. */
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
            out.writeInt(result.getSource().ordinal());
            out.writeFloat(ClientSounds.volume(result));
            out.writeFloat(ClientSounds.pitch(result));
            out.writeBoolean(result.isRelative());
            out.writeDouble(result.getX());
            out.writeDouble(result.getY());
            out.writeDouble(result.getZ());
            out.writeBoolean(result.isLooping());
            out.writeInt(result.getDelay());
            out.writeBoolean(result.getAttenuation() == SoundInstance.Attenuation.LINEAR);
        }
        return bytes.toByteArray();
    }

    static byte[] ask(DataInputStream in) throws IOException {
        return ask(in, sound -> net.neoforged.neoforge.client.ClientHooks.playSound(ClientSounds.engine, sound));
    }
}
