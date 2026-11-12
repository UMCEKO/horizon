package dev.horizonlab.javamods.testmod;

import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.function.IntSupplier;
import java.util.function.Supplier;
import net.minecraft.client.Minecraft;
import net.minecraft.world.entity.player.Player;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.tick.PlayerTickEvent;

/** A client listener of the local player's tick: the window's step must run between its `Pre` and `Post`, in vanilla's order. */
final class ClientMotion {
    private ClientMotion() {}

    private static final List<String> ORDER = List.of("ClientTick.Pre", "K", "PlayerTick.Pre", "M", "PlayerTick.Post", "ClientTick.Post");
    private static final int TICKS = 20;
    private static final List<String> markers = new ArrayList<>();
    private static int k, m, checked;
    private static boolean orderDone, orderOk = true, postDone;
    private static String firstWrong;

    private static int parks(String point) {
        return System.getProperties().get("horizon.lockstep.parks." + point) instanceof IntSupplier parks ? parks.getAsInt() : 0;
    }

    /** The parks since the last event, then the event itself. */
    private static void mark(String event) {
        int nowK = parks("K"), nowM = parks("M");
        if (nowK > k) markers.add("K");
        if (nowM > m) markers.add("M");
        k = nowK;
        m = nowM;
        markers.add(event);
    }

    private static boolean local(Player player) {
        return player.level().isClientSide() && player == Minecraft.getInstance().player;
    }

    static void register() {
        NeoForge.EVENT_BUS.addListener((ClientTickEvent.Pre event) -> {
            markers.clear();
            mark("ClientTick.Pre");
        });
        NeoForge.EVENT_BUS.addListener((PlayerTickEvent.Pre event) -> {
            if (local(event.getEntity())) mark("PlayerTick.Pre");
        });
        NeoForge.EVENT_BUS.addListener((PlayerTickEvent.Post event) -> {
            if (!local(event.getEntity())) return;
            boolean stepped = parks("M") > m;
            mark("PlayerTick.Post");
            if (stepped && !postDone) postSees(event.getEntity());
        });
        NeoForge.EVENT_BUS.addListener((ClientTickEvent.Post event) -> {
            mark("ClientTick.Post");
            // Counting starts at the first tick the player stepped in lockstep.
            if (orderDone || (checked == 0 && !markers.contains("M"))) return;
            if (!markers.equals(ORDER) && orderOk) {
                orderOk = false;
                firstWrong = String.join(", ", markers);
            }
            if (++checked == TICKS) {
                orderDone = true;
                if (!orderOk) System.out.println("Horizon test: lockstep order was " + firstWrong);
                Checks.check("lockstep_order", () -> orderOk);
            }
        });
    }

    /** While walking, `Post` sees the position the window's step reached this tick. */
    private static void postSees(Player player) {
        if (player.getX() == player.xo && player.getZ() == player.zo) return;
        postDone = true;
        Checks.check("lockstep_post_sees_this_tick", () -> {
            if (!(System.getProperties().get("horizon.lockstep.lastFill") instanceof Supplier<?> fill)) return false;
            try {
                var in = new DataInputStream(new ByteArrayInputStream((byte[]) fill.get()));
                return in.readBoolean() && player.getX() == in.readDouble() && player.getY() == in.readDouble() && player.getZ() == in.readDouble();
            } catch (IOException e) {
                return false;
            }
        });
    }
}
