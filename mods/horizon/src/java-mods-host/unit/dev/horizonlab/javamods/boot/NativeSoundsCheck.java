package dev.horizonlab.javamods.boot;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.nio.charset.StandardCharsets;
import net.minecraft.client.resources.sounds.SimpleSoundInstance;
import net.minecraft.client.resources.sounds.SoundInstance;
import net.minecraft.sounds.SoundSource;
import net.minecraft.util.RandomSource;
import net.neoforged.bus.api.BusBuilder;
import net.neoforged.neoforge.client.event.sound.PlaySoundEvent;

/** `PlaySoundEvent` for a native sound: the census sees a mod's listener and not Horizon's own, and the listener's cancel or change comes back whole. */
public final class NativeSoundsCheck {
    private static byte[] request(String id, float volume) throws Exception {
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        byte[] name = id.getBytes(StandardCharsets.UTF_8);
        out.writeInt(name.length);
        out.write(name);
        out.writeInt(SoundSource.BLOCKS.ordinal());
        out.writeFloat(volume);
        out.writeFloat(1.0F);
        out.writeBoolean(false);
        out.writeDouble(1.5);
        out.writeDouble(64.5);
        out.writeDouble(2.5);
        out.writeBoolean(false);
        out.writeInt(0);
        out.writeBoolean(true);
        return bytes.toByteArray();
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    public static void main(String[] args) throws Exception {
        var bus = BusBuilder.builder().build();
        bus.start();
        check(!EventPosts.listened(bus, PlaySoundEvent.class), "an empty bus has no listener");
        bus.addListener(PlaySoundEvent.class, event -> {});
        EventPosts.own(PlaySoundEvent.class);
        check(!EventPosts.listened(bus, PlaySoundEvent.class), "Horizon's own listener does not count");
        bus.addListener(PlaySoundEvent.class, event -> {
            String path = event.getSound().getIdentifier().getPath();
            if (path.equals("cancel")) event.setSound(null);
            else if (path.equals("louder")) event.setSound(new SimpleSoundInstance(event.getSound().getIdentifier(), SoundSource.BLOCKS, 0.75F, 2.0F, RandomSource.create(), false, 0, SoundInstance.Attenuation.LINEAR, 9, 8, 7, false));
        });
        check(EventPosts.listened(bus, PlaySoundEvent.class), "a mod's PlaySoundEvent listener is found");
        java.util.function.UnaryOperator<SoundInstance> post = sound -> {
            var event = new PlaySoundEvent(null, sound);
            bus.post(event);
            return event.getSound();
        };
        check(NativeSounds.ask(new DataInputStream(new ByteArrayInputStream(request("a:cancel", 1.0F))), post)[0] == NativeSounds.CANCEL, "a cancel comes back");
        check(NativeSounds.ask(new DataInputStream(new ByteArrayInputStream(request("a:quiet", 1.0F))), post)[0] == NativeSounds.AS_ASKED, "no change plays as asked");
        var changed = new DataInputStream(new ByteArrayInputStream(NativeSounds.ask(new DataInputStream(new ByteArrayInputStream(request("a:louder", 1.0F))), post)));
        check(changed.readByte() == NativeSounds.CHANGED, "a replaced sound is a change");
        String id = new String(changed.readNBytes(changed.readInt()), StandardCharsets.UTF_8);
        check(id.equals("a:louder") && changed.readInt() == SoundSource.BLOCKS.ordinal() && changed.readFloat() == 0.75F && changed.readFloat() == 2.0F, "its name, source, volume and pitch");
        check(!changed.readBoolean() && changed.readDouble() == 9 && changed.readDouble() == 8 && changed.readDouble() == 7, "its place");
        check(!changed.readBoolean() && changed.readInt() == 0 && changed.readBoolean() && changed.available() == 0, "its looping, delay and attenuation, and nothing after");
        System.out.println("NativeSoundsCheck ok");
    }
}
