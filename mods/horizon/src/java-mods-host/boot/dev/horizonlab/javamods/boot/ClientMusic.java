package dev.horizonlab.javamods.boot;

import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.client.sounds.MusicManager;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.sounds.Music;
import net.minecraft.sounds.SoundEvent;
import net.neoforged.neoforge.client.event.SelectMusicEvent;

/** The music tick runs in Java while a mod patches it or listens to `SelectMusicEvent`: Horizon answers the two questions it asks the game. */
public final class ClientMusic {
    private ClientMusic() {}

    /** Whether Java's `MusicManager.tick` runs the session's music. */
    public static boolean owned;
    /** `Minecraft.getSituationalMusic`, as the window answered this tick. */
    public static Music music;
    /** `Minecraft.getMusicVolume`. */
    public static float volume = 1.0F;
    /** The window shows its level loading screen. */
    public static boolean loading;
    private static int frequency;
    private static final List<Boolean> TOASTS = new ArrayList<>();

    /** Once per connect: a route on the tick or a mod's listener of the event hands the tick to Java. */
    static void decide() {
        String why = null;
        if (Routing.route("net/minecraft/client/sounds/MusicManager#tick()V") >= 0) why = "route";
        else if (EventPosts.listened(net.neoforged.neoforge.common.NeoForge.EVENT_BUS, SelectMusicEvent.class)) why = "SelectMusicEvent listener";
        owned = why != null;
        System.err.println(owned ? "Horizon: music runs in Java (" + why + ")" : "Horizon: music stays native");
    }

    /** The window's answer as a `Music`: the event's holder when registered, else a direct one as data resolves an unknown event. */
    static Music music(String id, int minDelay, int maxDelay, boolean replace) {
        var location = Identifier.parse(id);
        Holder<SoundEvent> holder = BuiltInRegistries.SOUND_EVENT.get(location).<Holder<SoundEvent>>map(h -> h).orElseGet(() -> Holder.direct(SoundEvent.createVariableRangeEvent(location)));
        return new Music(holder, minDelay, maxDelay, replace);
    }

    private static String text(DataInputStream in) throws IOException {
        return new String(in.readNBytes(in.readInt()), java.nio.charset.StandardCharsets.UTF_8);
    }

    /** `Input` kind `Music`. */
    static void read(DataInputStream in) throws IOException {
        music = in.readBoolean() ? music(text(in), in.readInt(), in.readInt(), in.readBoolean()) : null;
        volume = in.readFloat();
        loading = in.readBoolean();
        int next = in.readByte();
        if (next != frequency) {
            frequency = next;
            var manager = Minecraft.getInstance().getMusicManager();
            if (manager != null) manager.setMinutesBetweenSongs(MusicManager.MusicFrequency.values()[Math.max(0, Math.min(2, next))]);
        }
    }

    /** A Now Playing toast request the tick made: native draws its own toast. */
    public static void toast(boolean show) {
        TOASTS.add(show);
    }

    /** The status's `music` block. */
    static void writeStatus(DataOutputStream out) throws IOException {
        out.writeBoolean(owned);
        if (!owned) return;
        var current = ((dev.horizonlab.javamods.boot.mixin.MusicManagerAccess) Minecraft.getInstance().getMusicManager()).horizon$currentMusic();
        out.writeInt(current == null ? -1 : ClientSounds.handle(current));
        out.writeInt(TOASTS.size());
        for (boolean show : TOASTS) out.writeBoolean(show);
        TOASTS.clear();
    }

    static void forget() {
        music = null;
        TOASTS.clear();
    }
}
