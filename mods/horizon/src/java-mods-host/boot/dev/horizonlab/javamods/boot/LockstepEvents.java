package dev.horizonlab.javamods.boot;

import dev.horizonlab.javamods.boot.mixin.ClientInputAccess;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.io.UncheckedIOException;
import net.minecraft.client.Minecraft;
import net.minecraft.client.player.LocalPlayer;
import net.minecraft.world.entity.player.Input;
import net.minecraft.world.phys.Vec2;
import net.neoforged.neoforge.client.ClientHooks;
import net.neoforged.neoforge.common.CommonHooks;
import net.neoforged.neoforge.event.EventHooks;
import net.neoforged.neoforge.event.entity.living.LivingFallEvent;

/** The local player's movement input, jump and fall events, posted on the parked thread at the moments native's step reaches them (`G/java_lockstep.rs` sends them). */
final class LockstepEvents {
    private LockstepEvents() {}

    static final byte INPUT = 1, JUMP = 2, FALL = 3;

    static void install() {
        Lockstep.handler(INPUT, in -> run(in, LockstepEvents::input));
        Lockstep.handler(JUMP, in -> run(in, LockstepEvents::jump));
        Lockstep.handler(FALL, in -> run(in, LockstepEvents::fall));
    }

    private interface Body {
        void run(DataInputStream in, DataOutputStream out) throws IOException;
    }

    private static byte[] run(byte[] request, Body body) {
        var bytes = new ByteArrayOutputStream();
        try {
            body.run(new DataInputStream(new ByteArrayInputStream(request)), new DataOutputStream(bytes));
            // The write log is the last thing in every reply.
            bytes.write(MotionLog.take());
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
        return bytes.toByteArray();
    }

    static Input keyPresses(int bits) {
        return new Input((bits & 1) != 0, (bits & 2) != 0, (bits & 4) != 0, (bits & 8) != 0, (bits & 16) != 0, (bits & 32) != 0, (bits & 64) != 0);
    }

    static int bits(Input input) {
        return (input.forward() ? 1 : 0) | (input.backward() ? 2 : 0) | (input.left() ? 4 : 0) | (input.right() ? 8 : 0)
                | (input.jump() ? 16 : 0) | (input.shift() ? 32 : 0) | (input.sprint() ? 64 : 0);
    }

    private static LocalPlayer player() {
        LocalPlayer player = Minecraft.getInstance().player;
        if (player == null) throw new IllegalStateException("no local player at a lockstep event");
        return player;
    }

    /** `byte keyPresses`, `float left`, `float forward` in; the same out, as the event's listeners left them. */
    private static void input(DataInputStream in, DataOutputStream out) throws IOException {
        LocalPlayer player = player();
        int keys = in.readUnsignedByte();
        float x = in.readFloat(), y = in.readFloat();
        MotionLog.filling = true;
        try {
            player.input.keyPresses = keyPresses(keys);
            ((ClientInputAccess) player.input).horizon$moveVector(new Vec2(x, y));
        } finally {
            MotionLog.filling = false;
        }
        ClientHooks.onMovementInputUpdate(player, player.input);
        out.writeByte(bits(player.input.keyPresses));
        out.writeFloat(player.input.getMoveVector().x);
        out.writeFloat(player.input.getMoveVector().y);
    }

    /** Three `double`s: the velocity vanilla's jump just set. */
    private static void jump(DataInputStream in, DataOutputStream out) throws IOException {
        LocalPlayer player = player();
        double x = in.readDouble(), y = in.readDouble(), z = in.readDouble();
        MotionLog.filling = true;
        try {
            player.setDeltaMovement(x, y, z);
        } finally {
            MotionLog.filling = false;
        }
        CommonHooks.onLivingJump(player);
    }

    /** `bool flyable`, `double distance`, `float multiplier` in; `bool canceled`, `double distance`, `float multiplier` out. */
    private static void fall(DataInputStream in, DataOutputStream out) throws IOException {
        LocalPlayer player = player();
        boolean flyable = in.readBoolean();
        double distance = in.readDouble();
        float multiplier = in.readFloat();
        if (flyable) {
            EventHooks.onPlayerFall(player, (float) distance, multiplier);
            out.writeBoolean(false);
            out.writeDouble(distance);
            out.writeFloat(multiplier);
            return;
        }
        LivingFallEvent event = CommonHooks.onLivingFall(player, distance, multiplier);
        out.writeBoolean(event.isCanceled());
        out.writeDouble(event.getDistance());
        out.writeFloat(event.getDamageMultiplier());
    }
}
