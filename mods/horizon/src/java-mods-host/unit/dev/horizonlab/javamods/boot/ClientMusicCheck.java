package dev.horizonlab.javamods.boot;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.nio.charset.StandardCharsets;

/** The window's music answer reads as a `Music`, and an event no registry knows resolves as data does. */
public final class ClientMusicCheck {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    private static void text(DataOutputStream out, String value) throws Exception {
        byte[] bytes = value.getBytes(StandardCharsets.UTF_8);
        out.writeInt(bytes.length);
        out.write(bytes);
    }

    private static void read(String id, int min, int max, boolean replace) throws Exception {
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeBoolean(true);
        text(out, id);
        out.writeInt(min);
        out.writeInt(max);
        out.writeBoolean(replace);
        out.writeFloat(0.5F);
        out.writeBoolean(true);
        out.writeByte(0);
        ClientMusic.read(new DataInputStream(new ByteArrayInputStream(bytes.toByteArray())));
    }

    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.tryDetectVersion();
        net.minecraft.server.Bootstrap.bootStrap();
        read("minecraft:music.credits", 0, 5, true);
        var music = ClientMusic.music;
        check(music != null && music.minDelay() == 0 && music.maxDelay() == 5 && music.replaceCurrentMusic(), "delays and replace flag");
        check(music.sound().value().location().toString().equals("minecraft:music.credits"), "a registered id");
        read("horizon:no_such_music", 7, 9, false);
        music = ClientMusic.music;
        check(music.sound().value().location().toString().equals("horizon:no_such_music"), "an unregistered id keeps its location");
        check(!music.sound().kind().equals(net.minecraft.core.Holder.Kind.REFERENCE), "and is a direct holder");
        check(music.minDelay() == 7 && music.maxDelay() == 9 && !music.replaceCurrentMusic(), "its delays");
        check(ClientMusic.volume == 0.5F && ClientMusic.loading, "volume and loading");
        System.out.println("ClientMusicCheck ok");
    }
}
