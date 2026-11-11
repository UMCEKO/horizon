package dev.horizonlab.javamods.testmod;

import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.entity.EntitySpawnReason;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.LightningBolt;
import net.minecraft.world.entity.item.PrimedTnt;
import net.minecraft.world.entity.vehicle.minecart.Minecart;
import net.minecraft.world.level.block.Blocks;
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
    private static PrimedTnt fused;
    private static Chicken bystander;
    private static ItemEntity diamond;

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
        diamond = drop;
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
        tntAndLightning(level, area);
        itemHurt(level, area);
        vehicleHurt(level, area);
    }

    private static Minecart dented;

    /** `VehicleEntity.hurtServer`: a hit adds ten times its damage and past 40 the vehicle breaks; the damage wears off one a tick. */
    private static void vehicleHurt(Level level, BlockPos area) {
        var sl = (ServerLevel) level;
        var cart = EntityTypes.MINECART.create(level, EntitySpawnReason.COMMAND);
        cart.setPos(area.getX() + 2.5, area.getY() + 1, area.getZ() + 6.5);
        level.addFreshEntity(cart);
        Checks.check("vehicle_hurt_breaks_past_40", () -> cart.hurtServer(sl, level.damageSources().generic(), 3F) && sl.getEntity(cart.getId()) == cart
                && cart.hurtServer(sl, level.damageSources().generic(), 2F) && sl.getEntity(cart.getId()) == null);
        dented = EntityTypes.MINECART.create(level, EntitySpawnReason.COMMAND);
        dented.setPos(area.getX() + 2.5, area.getY() + 1, area.getZ() + 8.5);
        level.addFreshEntity(dented);
        dented.hurtServer(sl, level.damageSources().generic(), 3F);
    }

    private static void tntAndLightning(Level level, BlockPos area) {
        var sl = (ServerLevel) level;
        var tnt = new PrimedTnt(level, area.getX() + 0.5, area.getY() + 1, area.getZ() + 5.5, null);
        tnt.setFuse(200);
        Checks.check("tnt_spawns_as_a_view", () -> level.addFreshEntity(tnt) && tnt.getId() > 0 && sl.getEntity(tnt.getId()) == tnt && tnt.getFuse() == 200);
        tnt.setFuse(600);
        Checks.check("tnt_fuse_writes_through", () -> sl.getEntity(tnt.getId()) == tnt && tnt.getFuse() == 600);
        tnt.setBlockState(Blocks.OBSIDIAN.defaultBlockState());
        Checks.check("tnt_block_state_writes_through", () -> sl.getEntity(tnt.getId()) == tnt && tnt.getBlockState().is(Blocks.OBSIDIAN));
        fused = tnt;
        bystander = EntityTypes.CHICKEN.create(level, EntitySpawnReason.COMMAND);
        bystander.setPos(area.getX() + 0.5, area.getY() + 1, area.getZ() - 4.5);
        bystander.setNoAi(true);
        level.addFreshEntity(bystander);
        var bolt = EntityTypes.LIGHTNING_BOLT.create(level, EntitySpawnReason.COMMAND);
        bolt.setDamage(0F);
        bolt.setVisualOnly(true);
        bolt.snapTo(area.getX() + 0.5, area.getY() + 1, area.getZ() - 3.5, 0, 0);
        Checks.check("bolt_spawn_keeps_damage_and_visual", () -> level.addFreshEntity(bolt) && sl.getEntity(bolt.getId()) == bolt && bolt.getDamage() == 0F);
        bolt.setDamage(2F);
        Checks.check("bolt_setters_write_through", () -> sl.getEntity(bolt.getId()) == bolt && bolt.getDamage() == 2F);
        var hitter = EntityTypes.LIGHTNING_BOLT.create(level, EntitySpawnReason.COMMAND);
        hitter.setDamage(3F);
        var struck = EntityTypes.CHICKEN.create(level, EntitySpawnReason.COMMAND);
        struck.setPos(area.getX() + 0.5, area.getY() + 1, area.getZ() + 8.5);
        struck.setNoAi(true);
        level.addFreshEntity(struck);
        struck.thunderHit(sl, hitter);
        Checks.check("thunder_hit_uses_bolt_damage", () -> sl.getEntity(struck.getId()) == struck && struck.getHealth() == struck.getMaxHealth() - 3F && struck.getRemainingFireTicks() > 0);
    }

    private static void itemHurt(Level level, BlockPos area) {
        var sl = (ServerLevel) level;
        var safe = new ItemEntity(level, area.getX() + 0.5, area.getY() + 1, area.getZ() + 3.5, new ItemStack(Items.COBBLESTONE, 1));
        safe.setPermanentlyInvulnerable(true);
        level.addFreshEntity(safe);
        Checks.check("item_hurt_respects_invulnerable", () -> !safe.hurtServer(sl, level.damageSources().generic(), 10F) && sl.getEntity(safe.getId()) == safe);
        var frail = new ItemEntity(level, area.getX() + 0.5, area.getY() + 1, area.getZ() + 4.5, new ItemStack(Items.DIRT, 1));
        level.addFreshEntity(frail);
        Checks.check("item_hurt_truncates_like_vanilla", () -> {
            for (int i = 0; i < 4; i++) if (!frail.hurtServer(sl, level.damageSources().generic(), 0.5F)) return false;
            if (sl.getEntity(frail.getId()) != frail) return false;
            return frail.hurtServer(sl, level.damageSources().generic(), 0.5F) && sl.getEntity(frail.getId()) == null;
        });
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
        if (spawned != null) {
            Checks.check("spawned_mob_is_the_live_view", () -> chicken == spawned);
            var join = EntityLife.JOINS.get(spawned.getId());
            Checks.check("join_fired_once_fresh", () -> join != null && join.count() == 1 && !join.fromDisk());
            Checks.check("join_before_level", () -> join != null && !join.visibleWhileJoining());
        }
        // The harness kills the diamond with `/kill` (`Entity.kill`); the emerald below is discarded.
        if (diamond != null) Checks.check("leave_with_reason_killed", () -> "KILLED".equals(EntityLife.LEAVES.get(diamond.getId())));
        Checks.check("refused_bat_absent", () -> !EntityLife.REFUSED.isEmpty() && EntityLife.BAT_LEAVES.isEmpty()
                && level.getEntitiesOfClass(net.minecraft.world.entity.ambient.Bat.class, box).isEmpty());
        chicken.setHealth(1.5F);
        Checks.check("set_health_writes_through", () -> ((net.minecraft.server.level.ServerLevel) level).getEntity(chicken.getId()) == chicken && chicken.getHealth() == 1.5F);
        chicken.setPathfindingMalus(net.minecraft.world.level.pathfinder.PathType.WATER, 0F);
        Checks.check("malus_writes_through", () -> ((ServerLevel) level).getEntity(chicken.getId()) == chicken && chicken.getPathfindingMalus(net.minecraft.world.level.pathfinder.PathType.WATER) == 0F);
        var arrow = EntityTypes.ARROW.create(level, EntitySpawnReason.COMMAND);
        arrow.setPos(area.getX() + 0.5, area.getY() + 3, area.getZ() + 0.5);
        level.addFreshEntity(arrow);
        arrow.setBaseDamage(7.0);
        Checks.check("arrow_base_damage_writes_through", () -> ((ServerLevel) level).getEntity(arrow.getId()) == arrow && baseDamage(arrow) == 7.0);
        var snowball = EntityTypes.SNOWBALL.create(level, EntitySpawnReason.COMMAND);
        snowball.setPos(area.getX() + 0.5, area.getY() + 3, area.getZ() + 0.5);
        level.addFreshEntity(snowball);
        snowball.shootFromRotation(chicken, 0, 90, 0, 1.5F, 0);
        Checks.check("shoot_from_rotation_writes_through", () -> ((ServerLevel) level).getEntity(snowball.getId()) == snowball && snowball.getDeltaMovement().x < -1.4);
        var stand = EntityTypes.ARMOR_STAND.create(level, EntitySpawnReason.COMMAND);
        stand.setPos(area.getX() + 2.5, area.getY() + 1, area.getZ() + 0.5);
        level.addFreshEntity(stand);
        snowball.setOwner(stand);
        Checks.check("projectile_owner_any_entity", () -> snowball.getOwner() == stand);
        arrow.setOwner(context.getPlayer());
        chicken.setCustomName(net.minecraft.network.chat.Component.literal("java"));
        chicken.getPersistentData().putInt("horizon_java_test", 7);
        chicken.setDeltaMovement(0, 0.5, 0);
        Checks.check("delta_movement_writes_through", () -> ((net.minecraft.server.level.ServerLevel) level).getEntity(chicken.getId()) == chicken && chicken.getDeltaMovement().y == 0.5);
        // 30 from the first call less a tick's wear, plus 11, stays at 40 or below only if the native cart took the first hit.
        if (dented != null) Checks.check("vehicle_damage_wears_off_natively", () -> dented.hurtServer((ServerLevel) level, level.damageSources().generic(), 1.1F) && ((ServerLevel) level).getEntity(dented.getId()) == dented);
        if (fused != null) Checks.check("tnt_fuse_ticks_natively", () -> ((ServerLevel) level).getEntity(fused.getId()) == fused && fused.getFuse() < 600);
        if (bystander != null) Checks.check("visual_bolt_hurts_nobody", () -> ((ServerLevel) level).getEntity(bystander.getId()) == bystander && bystander.getHealth() == bystander.getMaxHealth());
        if (doomed != null) {
            int id = doomed.getId();
            doomed.discard();
            Checks.check("discard_writes_through", () -> ((net.minecraft.server.level.ServerLevel) level).getEntity(id) == null && doomed.isRemoved());
            Checks.check("leave_with_reason_discarded", () -> "DISCARDED".equals(EntityLife.LEAVES.get(id)));
        }
    }

    private static double baseDamage(net.minecraft.world.entity.projectile.arrow.AbstractArrow arrow) {
        try {
            var field = net.minecraft.world.entity.projectile.arrow.AbstractArrow.class.getDeclaredField("baseDamage");
            field.setAccessible(true);
            return field.getDouble(arrow);
        } catch (ReflectiveOperationException e) {
            return Double.NaN;
        }
    }

    static void afterChange(UseOnContext context, BlockPos area) {
        Level level = context.getLevel();
        var chickens = level.getEntitiesOfClass(Chicken.class, new AABB(area).inflate(6));
        Checks.check("mob_change_written_back", () -> chickens.stream().anyMatch(c -> c.getHealth() == 1.5F));
        Checks.check("mob_name_written_back", () -> chickens.stream().anyMatch(c -> c.hasCustomName() && "java".equals(c.getCustomName().getString())));
        Checks.check("persistent_data_saved", () -> chickens.stream().anyMatch(c -> c.getPersistentData().getIntOr("horizon_java_test", 0) == 7));
        Checks.check("join_from_disk", () -> chickens.stream().anyMatch(c -> {
            var join = EntityLife.JOINS.get(c.getId());
            return c.hasCustomName() && join != null && join.count() == 1 && join.fromDisk();
        }));
        var arrows = level.getEntitiesOfClass(net.minecraft.world.entity.projectile.arrow.AbstractArrow.class, new AABB(area).inflate(6));
        Checks.check("projectile_owner_survives_reload", () -> arrows.stream().anyMatch(a -> a.getOwner() == context.getPlayer()));
    }
}
