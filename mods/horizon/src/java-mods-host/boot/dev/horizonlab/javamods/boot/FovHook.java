package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.io.UncheckedIOException;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.AbstractClientPlayer;
import net.neoforged.neoforge.client.ClientHooks;

/** `Camera.tickFov` in a lockstep tick: the event runs on native's base modifier, and the result goes back in the park record for native to ease toward. */
public final class FovHook {
    private FovHook() {}

    private static volatile float base = 1f;
    private static volatile boolean scoping;
    private static volatile float scale = 1f;
    private static volatile Float target;

    /** Native's `FovBase` input: the modifier it computed for the player, whether a spyglass scopes, the effect scale. */
    static void base(DataInputStream in) throws IOException {
        base = in.readFloat();
        scoping = in.readBoolean();
        scale = in.readFloat();
    }

    /** Vanilla's `tickFov` up to the easing; the spyglass branch returns before the event, as the patched `getFieldOfViewModifier` does. */
    public static void tick() {
        if (scoping) {
            target = 0.1f;
        } else if (Minecraft.getInstance().getCameraEntity() instanceof AbstractClientPlayer player) {
            target = ClientHooks.getFieldOfViewModifier(player, base, scale);
        } else {
            target = 1.0f;
        }
    }

    /** `bool present`, then `float target`; the target is taken. */
    static byte[] take() {
        Float taken = target;
        target = null;
        var bytes = new ByteArrayOutputStream();
        try {
            var out = new DataOutputStream(bytes);
            out.writeBoolean(taken != null);
            out.writeFloat(taken != null ? taken : 0f);
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
        return bytes.toByteArray();
    }
}
