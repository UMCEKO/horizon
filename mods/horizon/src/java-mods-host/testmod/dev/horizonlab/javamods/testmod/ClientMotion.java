package dev.horizonlab.javamods.testmod;

import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import java.util.function.IntSupplier;
import java.util.function.Supplier;
import net.minecraft.client.Minecraft;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.entity.player.Player;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.client.event.ComputeFovModifierEvent;
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
    private static final Identifier ROCKET = Identifier.fromNamespaceAndPath("horizon_java_test", "rocket");
    private static int rocketTicks, heldTicks;
    private static boolean rocketThisTick;
    private static double startY, preY, preVy;

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

    private static boolean holdsRocket(Player player) {
        return BuiltInRegistries.ITEM.getKey(player.getMainHandItem().getItem()).equals(ROCKET);
    }

    private static final List<Float> fovs = new ArrayList<>();
    private static int ticks, lastEventTick = -1, sprintTick = -1;
    private static boolean everyTick = true, fovChecked, sprintChecked;

    /** Vanilla's modifier for a sprinting player: speed 0.1 with the +30 % multiplied total, over the walking speed 0.1. */
    private static float sprintModifier() {
        float speed = (float) (0.1 * 1.3);
        return (speed / 0.1f + 1.0f) / 2.0f;
    }

    private static boolean holdsLens(Player player) {
        return BuiltInRegistries.ITEM.getKey(player.getMainHandItem().getItem()).toString().equals("horizon_java_test:lens");
    }

    private static void onFov(ComputeFovModifierEvent event) {
        Player player = Minecraft.getInstance().player;
        if (player == null) return;
        float seen = event.getFovModifier();
        if (lastEventTick >= 0 && ticks != lastEventTick + 1) everyTick = false;
        lastEventTick = ticks;
        if (fovs.size() < 20) fovs.add(seen);
        if (fovs.size() == 20 && !fovChecked) {
            fovChecked = true;
            Checks.check("fov_event_every_tick", () -> everyTick);
            Checks.check("fov_base_standing", () -> fovs.stream().allMatch(v -> v == 1.0f));
        }
        // The first changed modifier after sprinting starts comes one tick after the Post that first saw it.
        if (sprintTick >= 0 && !sprintChecked && seen != 1.0f) {
            sprintChecked = true;
            int at = ticks;
            Checks.check("fov_base_sprinting", () -> at == sprintTick + 1 && seen == sprintModifier());
        }
        if (holdsLens(player)) event.setNewFovModifier(0.5f);
    }

    static void register() {
        NeoForge.EVENT_BUS.addListener(ClientMotion::onFov);
        NeoForge.EVENT_BUS.addListener((ClientTickEvent.Post event) -> ticks++);
        NeoForge.EVENT_BUS.addListener((ClientTickEvent.Pre event) -> {
            markers.clear();
            mark("ClientTick.Pre");
            rocket(Minecraft.getInstance().player);
        });
        NeoForge.EVENT_BUS.addListener((PlayerTickEvent.Pre event) -> {
            if (!local(event.getEntity())) return;
            mark("PlayerTick.Pre");
            Player player = event.getEntity();
            if (heldTicks == 12 && holdsRocket(player)) {
                preY = player.getY();
                preVy = player.getDeltaMovement().y;
                player.setPos(player.getX(), preY + 1, player.getZ());
            }
        });
        NeoForge.EVENT_BUS.addListener((PlayerTickEvent.Post event) -> {
            if (!local(event.getEntity())) return;
            if (sprintTick < 0 && event.getEntity().isSprinting()) sprintTick = ticks;
            boolean stepped = parks("M") > m;
            mark("PlayerTick.Post");
            if (stepped && !postDone) postSees(event.getEntity());
            rocketSees(event.getEntity());
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

    /** A mod's `setDeltaMovement` from `ClientTickEvent.Pre`, as Mekanism's jetpack writes: native's step this tick moves by it. */
    private static void rocket(Player player) {
        if (player == null || !holdsRocket(player)) return;
        heldTicks++;
        if (rocketTicks >= 10) return;
        if (rocketTicks == 0) startY = player.getY();
        player.setDeltaMovement(0, 0.5, 0);
        rocketTicks++;
        rocketThisTick = true;
    }

    /** Vanilla's `travelInAir` moves by the delta as set, then applies gravity to the rest: tick k ends 0.5·k above the start. */
    private static void rocketSees(Player player) {
        if (rocketThisTick) {
            rocketThisTick = false;
            if (rocketTicks == 1) Checks.check("rocket_moves_in_its_own_tick", () -> Math.abs(player.getY() - startY - 0.5) < 1e-9);
            if (rocketTicks == 10) Checks.check("rocket_lifts_five_blocks", () -> Math.abs(player.getY() - startY - 5.0) < 1e-9);
        }
        if (heldTicks == 12 && holdsRocket(player)) {
            Checks.check("pre_write_moves_without_lerp_start", () -> {
                if (!(System.getProperties().get("horizon.lockstep.lastFill") instanceof Supplier<?> fill)) return false;
                try {
                    var in = new DataInputStream(new ByteArrayInputStream((byte[]) fill.get()));
                    if (!in.readBoolean()) return false;
                    in.readDouble();
                    return Math.abs(in.readDouble() - (preY + 1 + preVy)) < 1e-9 && player.yo == preY;
                } catch (IOException e) {
                    return false;
                }
            });
        }
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
