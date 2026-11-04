package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.lang.reflect.Field;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import net.minecraft.client.resources.sounds.AbstractSoundInstance;
import net.minecraft.client.resources.sounds.SoundInstance;
import net.minecraft.client.resources.sounds.TickableSoundInstance;
import net.minecraft.client.sounds.SoundEngine;
import net.minecraft.resources.Identifier;
import net.minecraft.sounds.SoundSource;

/** `SoundEngine` whose channels are Horizon's: each play, change and stop goes to its sound engine as an event, in the order this client made them. */
public final class ClientSounds {
    private ClientSounds() {}

    static final byte PLAY = 0;
    static final byte UPDATE = 1;
    static final byte STOP = 2;

    /** `instanceToChannel`: each playing instance by the handle Horizon knows it as. */
    private static final Map<SoundInstance, Integer> PLAYING = new IdentityHashMap<>();
    private static final List<TickableSoundInstance> TICKING = new ArrayList<>();
    private static final List<TickableSoundInstance> QUEUED_TICKING = new ArrayList<>();
    private static final ByteArrayOutputStream BYTES = new ByteArrayOutputStream();
    private static final DataOutputStream OUT = new DataOutputStream(BYTES);
    /** The engine mods get in `SoundEngineLoadEvent` and `PlaySoundEvent`. */
    public static SoundEngine engine;
    private static int events;
    private static int next;
    private static Field volume;
    private static Field pitch;

    static float volume(SoundInstance instance) {
        return field(instance, true);
    }

    static float pitch(SoundInstance instance) {
        return field(instance, false);
    }

    /** The instance's own volume and pitch: Horizon samples its sound's, as resolving it there picks the sound. */
    private static float field(SoundInstance instance, boolean isVolume) {
        if (!(instance instanceof AbstractSoundInstance)) return isVolume ? instance.getVolume() : instance.getPitch();
        try {
            if (volume == null) {
                volume = AbstractSoundInstance.class.getDeclaredField("volume");
                pitch = AbstractSoundInstance.class.getDeclaredField("pitch");
                volume.setAccessible(true);
                pitch.setAccessible(true);
            }
            return (isVolume ? volume : pitch).getFloat(instance);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    private static void text(String value) throws IOException {
        byte[] bytes = value.getBytes(StandardCharsets.UTF_8);
        OUT.writeInt(bytes.length);
        OUT.write(bytes);
    }

    /** `SoundEngine.play` and `playDelayed`: Horizon resolves the event, picks its sound and plays it. */
    public static SoundEngine.PlayResult play(SoundInstance instance, int delay) {
        // Ticks run only to record a renderer's motion make no sound.
        if (BlockRenders.sampling() || !instance.canPlaySound()) return SoundEngine.PlayResult.NOT_STARTED;
        if (nativePlays()) return SoundEngine.PlayResult.NOT_STARTED;
        return start(instance, delay);
    }

    /** A vanilla source native ports plays its own sound: this one is its duplicate, except where Java-run `animateTick` code made it. */
    private static boolean nativePlays() {
        return !Animate.forwarding() && SoundOwners.nativeOwnsCaller();
    }

    private static SoundEngine.PlayResult start(SoundInstance instance, int delay) {
        // `PlaySoundEvent` is where mods swap, wrap or drop a sound and learn which ones to stop later.
        instance = net.neoforged.neoforge.client.ClientHooks.playSound(engine, instance);
        if (instance == null) return SoundEngine.PlayResult.NOT_STARTED;
        // A vanilla HUD layer's sounds are native's: the window plays them itself.
        if (HudLayers.discarded()) return SoundEngine.PlayResult.NOT_STARTED;
        // A click's prediction repeats the window's own: the window already plays that placement's block sound.
        if (ClientBridge.clicking() && instance.getSource() == SoundSource.BLOCKS) return SoundEngine.PlayResult.NOT_STARTED;
        stop(instance);
        int handle = ++next;
        PLAYING.put(instance, handle);
        if (instance instanceof TickableSoundInstance tickable) TICKING.add(tickable);
        try {
            OUT.writeByte(PLAY);
            OUT.writeInt(handle);
            text(instance.getIdentifier().toString());
            OUT.writeInt(instance.getSource().ordinal());
            OUT.writeFloat(field(instance, true));
            OUT.writeFloat(field(instance, false));
            OUT.writeDouble(instance.getX());
            OUT.writeDouble(instance.getY());
            OUT.writeDouble(instance.getZ());
            OUT.writeBoolean(instance.isLooping());
            OUT.writeInt(delay > 0 ? delay : instance.getDelay());
            OUT.writeBoolean(instance.getAttenuation() == SoundInstance.Attenuation.LINEAR);
            OUT.writeBoolean(instance.isRelative());
            OUT.writeBoolean(instance.canStartSilent());
            OUT.writeBoolean(instance instanceof TickableSoundInstance);
            events++;
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
        return SoundEngine.PlayResult.STARTED;
    }

    public static void stop(SoundInstance instance) {
        Integer handle = PLAYING.remove(instance);
        if (instance instanceof TickableSoundInstance tickable) TICKING.remove(tickable);
        if (handle == null) return;
        try {
            OUT.writeByte(STOP);
            OUT.writeInt(handle);
            events++;
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    public static void stopAll() {
        for (var instance : new ArrayList<>(PLAYING.keySet())) stop(instance);
        TICKING.clear();
        QUEUED_TICKING.clear();
    }

    /** `SoundEngine.stop(sound, source)`. */
    public static void stop(Identifier sound, SoundSource source) {
        if (source == null && sound == null) {
            stopAll();
            return;
        }
        for (var instance : new ArrayList<>(PLAYING.keySet())) {
            if ((source == null || instance.getSource() == source) && (sound == null || instance.getIdentifier().equals(sound))) stop(instance);
        }
    }

    public static boolean isActive(SoundInstance instance) {
        return PLAYING.containsKey(instance);
    }

    public static void queueTicking(TickableSoundInstance instance) {
        if (nativePlays()) return;
        QUEUED_TICKING.add(instance);
    }

    /** `SoundEngine.tickInGameSound`: queued tickable sounds start, then each ticking one ticks and Horizon gets its new volume, pitch and place. */
    public static void tick(boolean paused) {
        if (paused) return;
        var queued = new ArrayList<>(QUEUED_TICKING);
        QUEUED_TICKING.clear();
        for (var instance : queued) if (!BlockRenders.sampling() && instance.canPlaySound()) start(instance, 0);
        for (var instance : new ArrayList<>(TICKING)) {
            if (!instance.canPlaySound()) stop(instance);
            instance.tick();
            if (instance.isStopped()) {
                stop(instance);
                continue;
            }
            Integer handle = PLAYING.get(instance);
            if (handle == null) continue;
            try {
                OUT.writeByte(UPDATE);
                OUT.writeInt(handle);
                OUT.writeFloat(field(instance, true));
                OUT.writeFloat(field(instance, false));
                OUT.writeDouble(instance.getX());
                OUT.writeDouble(instance.getY());
                OUT.writeDouble(instance.getZ());
                events++;
            } catch (IOException e) {
                throw new IllegalStateException(e);
            }
        }
    }

    /** Horizon's channels that finished since the last tick, as `tickInGameSound` drops stopped channels. */
    static void finished(DataInputStream in) throws IOException {
        for (int n = in.readInt(); n > 0; n--) {
            int handle = in.readInt();
            PLAYING.entrySet().removeIf(entry -> {
                if (entry.getValue() != handle) return false;
                if (entry.getKey() instanceof TickableSoundInstance tickable) TICKING.remove(tickable);
                return true;
            });
        }
    }

    /** The events since the last call, for Horizon's sound engine. */
    static void take(DataOutputStream out) throws IOException {
        out.writeInt(events);
        BYTES.writeTo(out);
        BYTES.reset();
        events = 0;
    }

    static void forget() {
        PLAYING.clear();
        TICKING.clear();
        QUEUED_TICKING.clear();
        BYTES.reset();
        events = 0;
    }
}
