package dev.horizonlab.javamods.testmod;

import dev.horizonlab.javamods.testmod.mixin.ClientInputSet;
import net.minecraft.client.Minecraft;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.entity.player.Input;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.phys.Vec2;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.client.event.MovementInputUpdateEvent;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.living.LivingEvent;
import net.neoforged.neoforge.event.entity.living.LivingFallEvent;
import net.neoforged.neoforge.event.tick.PlayerTickEvent;

/** The local player's movement input, jump and fall events, listened to on the client. */
final class ClientMovement {
    private ClientMovement() {}

    private static final int WALK_TICKS = 20;
    private static int compassTicks, events;
    private static boolean everyTick = true, heldKeysSeen = true, walkChecked;
    private static double walkStartZ;
    private static boolean springJumped, apexChecked;
    private static double springStartY, apex;
    private static int fallEvents;
    private static double fallDistance;
    private static boolean fallChecked, fallWasAirborne;

    private static String held(Player player) {
        var key = BuiltInRegistries.ITEM.getKey(player.getMainHandItem().getItem());
        return key.getNamespace().equals("horizon_java_test") ? key.getPath() : "";
    }

    private static boolean local(Player player) {
        return player.level().isClientSide() && player == Minecraft.getInstance().player;
    }

    /** Vanilla's air loop after a jump of `v0`: travel moves first, then gravity 0.08 and drag 0.98. */
    private static double apexAbove(double v0) {
        double v = v0, h = 0;
        while (v > 0) {
            h += v;
            v = (v - 0.08) * 0.98;
        }
        return h;
    }

    static void register() {
        NeoForge.EVENT_BUS.addListener((MovementInputUpdateEvent event) -> {
            Player player = event.getEntity();
            if (!held(player).equals("compass")) return;
            if (compassTicks == 0) walkStartZ = player.getZ();
            events++;
            Input seen = event.getInput().keyPresses;
            if (seen.forward() || seen.backward() || seen.left() || seen.right() || seen.jump() || seen.shift() || seen.sprint()) heldKeysSeen = false;
            event.getInput().keyPresses = new Input(true, false, false, false, false, false, false);
            ((ClientInputSet) event.getInput()).horizon$testMoveVector(new Vec2(0, 1));
        });
        NeoForge.EVENT_BUS.addListener((PlayerTickEvent.Post event) -> {
            Player player = event.getEntity();
            if (!local(player)) return;
            if (held(player).equals("compass")) {
                if (++compassTicks == WALK_TICKS && !walkChecked) {
                    walkChecked = true;
                    double moved = player.getZ() - walkStartZ;
                    int seenEvents = events;
                    Checks.check("input_event_walks", () -> moved > 3.5 && moved < 4.5);
                    Checks.check("input_event_seen_every_tick", () -> seenEvents == WALK_TICKS && heldKeysSeen);
                }
            }
            if (springJumped && !apexChecked) {
                apex = Math.max(apex, player.getY());
                if (player.onGround()) {
                    apexChecked = true;
                    double rise = apex - springStartY;
                    Checks.check("spring_jump_apex", () -> Math.abs(rise - apexAbove(0.42 + 0.5)) < 1e-6);
                }
            }
        });
        NeoForge.EVENT_BUS.addListener((LivingEvent.LivingJumpEvent event) -> {
            if (!(event.getEntity() instanceof Player player) || !local(player) || !held(player).equals("spring") || springJumped) return;
            var dm = player.getDeltaMovement();
            springStartY = player.getY();
            apex = springStartY;
            springJumped = true;
            player.setDeltaMovement(dm.x, dm.y + 0.5, dm.z);
        });
        NeoForge.EVENT_BUS.addListener((LivingFallEvent event) -> {
            if (!(event.getEntity() instanceof Player player) || !local(player) || !held(player).equals("feather")) return;
            fallEvents++;
            fallDistance = event.getDistance();
            event.setCanceled(true);
        });
        NeoForge.EVENT_BUS.addListener((ClientTickEvent.Post event) -> {
            Player player = Minecraft.getInstance().player;
            if (player == null || fallChecked || !held(player).equals("feather")) return;
            if (!player.onGround()) fallWasAirborne = true;
            else if (fallWasAirborne) {
                fallChecked = true;
                int count = fallEvents;
                double distance = fallDistance;
                Checks.check("fall_canceled_client", () -> count == 1 && distance >= 5.9 && distance <= 6.1);
            }
        });
    }
}
