package dev.horizonlab.javamods.testmod;

import net.minecraft.client.Minecraft;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.EntityJoinLevelEvent;
import net.neoforged.neoforge.event.entity.EntityLeaveLevelEvent;

/** The client's level holds the server's entities: a named pig the server summons is found by id within 5 ticks, and is gone within 5 once removed. */
final class ClientChecks {
    private ClientChecks() {}

    static final String NAME = "horizon_pig";
    private static volatile int pig = -1;
    private static volatile Vec3 at;
    private static volatile boolean removed;
    private static int waited;
    private static boolean seen, lost;

    static void register() {
        NeoForge.EVENT_BUS.addListener((EntityJoinLevelEvent e) -> {
            var entity = e.getEntity();
            if (e.getLevel().isClientSide() || entity.getType() != EntityTypes.PIG || !named(entity)) return;
            at = entity.position();
            removed = false;
            pig = entity.getId();
        });
        NeoForge.EVENT_BUS.addListener((EntityLeaveLevelEvent e) -> {
            if (!e.getLevel().isClientSide() && e.getEntity().getId() == pig) removed = true;
        });
        NeoForge.EVENT_BUS.addListener((ClientTickEvent.Post event) -> tick());
    }

    private static boolean named(Entity entity) {
        return entity.getCustomName() != null && NAME.equals(entity.getCustomName().getString());
    }

    private static void tick() {
        var level = Minecraft.getInstance().level;
        int id = pig;
        if (level == null || id < 0 || lost) return;
        if (!seen) {
            Entity view = level.getEntity(id);
            boolean found = view != null && named(view) && view.position().distanceTo(at) <= 0.1;
            if (found || ++waited > 5) {
                seen = true;
                waited = 0;
                Checks.check("client_level_sees_the_pig", () -> found);
                Checks.check("client_level_finds_the_pig_in_its_box", () -> found && level.getEntities((Entity) null, view.getBoundingBox(), e -> e == view).size() == 1);
            }
            return;
        }
        if (!removed) return;
        boolean gone = level.getEntity(id) == null;
        if (gone || ++waited > 5) {
            lost = true;
            Checks.check("client_level_loses_the_removed_pig", () -> gone);
        }
    }
}
