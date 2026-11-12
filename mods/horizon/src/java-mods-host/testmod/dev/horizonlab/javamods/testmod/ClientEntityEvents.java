package dev.horizonlab.javamods.testmod;

import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityTypes;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.EntityJoinLevelEvent;
import net.neoforged.neoforge.event.entity.EntityLeaveLevelEvent;
import net.neoforged.neoforge.event.tick.EntityTickEvent;

/** The client bus hears the tagged pig join once, tick about once per tick, and leave once. */
final class ClientEntityEvents {
    private ClientEntityEvents() {}

    private static volatile int joins, leaves, ticks;
    private static int ticksSinceJoin = -1, ticksSinceLeave = -1;
    private static boolean joinChecked, leaveChecked;

    private static boolean tagged(Entity entity) {
        return entity.getType() == EntityTypes.PIG && entity.getCustomName() != null && ClientChecks.NAME.equals(entity.getCustomName().getString());
    }

    static void register() {
        NeoForge.EVENT_BUS.addListener((EntityJoinLevelEvent e) -> {
            if (e.getLevel().isClientSide() && tagged(e.getEntity())) joins++;
        });
        NeoForge.EVENT_BUS.addListener((EntityLeaveLevelEvent e) -> {
            if (e.getLevel().isClientSide() && tagged(e.getEntity())) leaves++;
        });
        NeoForge.EVENT_BUS.addListener((EntityTickEvent.Post e) -> {
            if (e.getEntity().level().isClientSide() && tagged(e.getEntity())) ticks++;
        });
        NeoForge.EVENT_BUS.addListener((ClientTickEvent.Post event) -> tick());
    }

    private static void tick() {
        if (joins > 0 && !joinChecked && ++ticksSinceJoin >= 20) {
            joinChecked = true;
            int count = joins, ticked = ticks;
            Checks.check("client_join_once", () -> count == 1);
            Checks.check("client_tick_post_per_tick", () -> ticked >= 18 && ticked <= 22);
        }
        if (leaves > 0 && !leaveChecked && ++ticksSinceLeave >= 5) {
            leaveChecked = true;
            int count = leaves;
            Checks.check("client_leave_once", () -> count == 1);
        }
    }
}
