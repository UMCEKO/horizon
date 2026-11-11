package dev.horizonlab.javamods.testmod;

import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Set;
import net.minecraft.world.entity.EntityTypes;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.EntityJoinLevelEvent;
import net.neoforged.neoforge.event.entity.EntityLeaveLevelEvent;

/** Records the server's entity joins and leaves by entity id, and refuses every bat's join. */
final class EntityLife {
    private EntityLife() {}

    record Join(int count, boolean fromDisk, boolean visibleWhileJoining) {}

    static final Map<Integer, Join> JOINS = new HashMap<>();
    static final Map<Integer, String> LEAVES = new HashMap<>();
    static final Set<Integer> REFUSED = new HashSet<>();
    static final Set<Integer> BAT_LEAVES = new HashSet<>();

    static void register() {
        NeoForge.EVENT_BUS.addListener((EntityJoinLevelEvent e) -> {
            if (e.getLevel().isClientSide()) return;
            var entity = e.getEntity();
            if (entity.getType() == EntityTypes.BAT) {
                e.setCanceled(true);
                REFUSED.add(entity.getId());
                return;
            }
            boolean visible = e.getLevel().getEntity(entity.getId()) != null;
            JOINS.merge(entity.getId(), new Join(1, e.loadedFromDisk(), visible), (a, b) -> new Join(a.count() + 1, a.fromDisk() || b.fromDisk(), a.visibleWhileJoining() || b.visibleWhileJoining()));
        });
        NeoForge.EVENT_BUS.addListener((EntityLeaveLevelEvent e) -> {
            if (e.getLevel().isClientSide()) return;
            var entity = e.getEntity();
            if (entity.getType() == EntityTypes.BAT) BAT_LEAVES.add(entity.getId());
            var reason = entity.getRemovalReason();
            LEAVES.put(entity.getId(), reason == null ? "none" : reason.name());
        });
    }
}
