package dev.horizonlab.javamods.boot;

import java.io.DataInputStream;
import net.minecraft.client.KeyboardHandler;
import net.minecraft.client.Minecraft;
import net.minecraft.client.MouseHandler;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.client.player.LocalPlayer;

/** W12 SPEC-22: a vanilla method of the Java client's objects that a mod patches, run as Java's merged method for the window, whose own body is the route's native. */
public final class ClientRoutes {
    private ClientRoutes() {}

    /** Whether a client route runs now, on this thread: route stubs call their native, and the client's level and player changes reach the window's. */
    public static boolean active() {
        return ClientBridge.standingIn();
    }

    /** The route stubs' guard: the window runs this call, so the window's body serves it; the Java client's own calls run vanilla's. */
    public static boolean window(Object receiver) {
        return active();
    }

    /** The `client_route` op: `int id`, the receiver byte, the arguments; answers the status and the value. */
    static byte[] call(DataInputStream in) throws Exception {
        var input = Crossing.In.of(in.readAllBytes());
        var out = new Crossing.Out();
        Routing.callClient(input, out, ClientRoutes::receiver);
        return out.toByteArray();
    }

    /** The Java client's object for a route's owner class, null while it holds none (no level yet); an owner of no client object is a seam bug. */
    static Object receiver(Class<?> owner, String key) {
        var minecraft = Minecraft.getInstance();
        if (owner == Minecraft.class) return minecraft;
        if (owner == KeyboardHandler.class) return minecraft.keyboardHandler;
        if (owner == MouseHandler.class) return minecraft.mouseHandler;
        if (owner == ClientLevel.class) return minecraft.level;
        if (owner == LocalPlayer.class) return minecraft.player;
        throw new IllegalStateException("route " + key + ": " + owner.getName() + " is no object of the Java client's");
    }

    /** A routed `KeyEvent`, as the window's input builds one: the key's name, the modifiers, SDL's keycode. */
    static KeyEvent keyEvent(Crossing.In in) {
        String name = in.text();
        int modifiers = in.readInt();
        return ClientBridge.keyEvent(name, modifiers, in.readInt());
    }
}
