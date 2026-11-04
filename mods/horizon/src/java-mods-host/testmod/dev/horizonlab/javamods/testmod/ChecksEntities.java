package dev.horizonlab.javamods.testmod;

import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.animal.chicken.Chicken;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.Level;
import net.minecraft.world.phys.AABB;

final class ChecksEntities {
    private ChecksEntities() {}

    private static Chicken spawned;
    private static ItemEntity doomed;

    static void run(UseOnContext context, BlockPos area) {
        Level level = context.getLevel();
        Player player = context.getPlayer();
        AABB box = new AABB(area).inflate(3);
        Checks.check("entities_empty_area", () -> {
            var found = level.getEntitiesOfClass(ItemEntity.class, box);
            if (!found.isEmpty()) System.out.println("entities_empty_area found " + found);
            return found.isEmpty();
        });
        Checks.check("player_in_own_box", () -> level.getEntitiesOfClass(Player.class, player.getBoundingBox().inflate(1)).contains(player));
        Checks.check("nearest_player", () -> level.getNearestPlayer(player.getX(), player.getY(), player.getZ(), 5, false) == player);
        Checks.check("server_entity_lookup", () -> ((net.minecraft.server.level.ServerLevel) level).getEntity(player.getUUID()) == player);
        var drop = new ItemEntity(level, area.getX() + 0.5, area.getY() + 1, area.getZ() + 0.5, new ItemStack(Items.DIAMOND, 2));
        Checks.check("spawn_item", () -> level.addFreshEntity(drop));
        Checks.check("spawn_is_native_at_once", () -> drop.getId() > 0 && ((net.minecraft.server.level.ServerLevel) level).getEntity(drop.getId()) == drop);
        doomed = new ItemEntity(level, area.getX() + 0.5, area.getY() + 1, area.getZ() - 1.5, new ItemStack(Items.EMERALD, 1));
        Checks.check("spawn_item_to_discard", () -> level.addFreshEntity(doomed));
        Checks.check("spawned_item_visible", () -> level.getEntitiesOfClass(ItemEntity.class, box).stream().anyMatch(e -> e.getItem().is(Items.DIAMOND) && e.getItem().getCount() == 2));
        var chicken = EntityTypes.CHICKEN.create(level, net.minecraft.world.entity.EntitySpawnReason.COMMAND);
        chicken.setPos(area.getX() + 0.5, area.getY() + 1, area.getZ() + 2.5);
        // It stays put to be found later, as a live one wanders out of the box.
        chicken.setNoAi(true);
        Checks.check("spawn_mob", () -> level.addFreshEntity(chicken));
        spawned = chicken;
    }

    /** Nothing here runs mid-call: `later` sees what the world did with them. */
    static void later(UseOnContext context, BlockPos area) {
        Level level = context.getLevel();
        AABB box = new AABB(area).inflate(6);
        var chickens = level.getEntitiesOfClass(Chicken.class, box);
        Checks.check("mob_visible_after", () -> !chickens.isEmpty());
        if (chickens.isEmpty()) return;
        Chicken chicken = chickens.get(0);
        Checks.check("mob_saved_form_read", () -> chicken.getHealth() > 0 && chicken.isAlive());
        if (spawned != null) Checks.check("spawned_mob_is_the_live_view", () -> chicken == spawned);
        chicken.setHealth(1.5F);
        Checks.check("set_health_writes_through", () -> ((net.minecraft.server.level.ServerLevel) level).getEntity(chicken.getId()) == chicken && chicken.getHealth() == 1.5F);
        chicken.setCustomName(net.minecraft.network.chat.Component.literal("java"));
        chicken.getPersistentData().putInt("horizon_java_test", 7);
        chicken.setDeltaMovement(0, 0.5, 0);
        Checks.check("delta_movement_writes_through", () -> ((net.minecraft.server.level.ServerLevel) level).getEntity(chicken.getId()) == chicken && chicken.getDeltaMovement().y == 0.5);
        if (doomed != null) {
            int id = doomed.getId();
            doomed.discard();
            Checks.check("discard_writes_through", () -> ((net.minecraft.server.level.ServerLevel) level).getEntity(id) == null && doomed.isRemoved());
        }
    }

    static void afterChange(UseOnContext context, BlockPos area) {
        Level level = context.getLevel();
        var chickens = level.getEntitiesOfClass(Chicken.class, new AABB(area).inflate(6));
        Checks.check("mob_change_written_back", () -> chickens.stream().anyMatch(c -> c.getHealth() == 1.5F));
        Checks.check("mob_name_written_back", () -> chickens.stream().anyMatch(c -> c.hasCustomName() && "java".equals(c.getCustomName().getString())));
        Checks.check("persistent_data_saved", () -> chickens.stream().anyMatch(c -> c.getPersistentData().getIntOr("horizon_java_test", 0) == 7));
    }
}
