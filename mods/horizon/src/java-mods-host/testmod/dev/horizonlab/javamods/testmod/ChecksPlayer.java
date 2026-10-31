package dev.horizonlab.javamods.testmod;

import net.minecraft.core.BlockPos;
import net.minecraft.resources.Identifier;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.util.Prediction;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.effect.MobEffectInstance;
import net.minecraft.world.effect.MobEffects;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.food.FoodProperties;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.component.SwingAnimation;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.phys.Vec3;

final class ChecksPlayer {
    private ChecksPlayer() {}

    static void run(UseOnContext context, BlockPos area) {
        if (!(context.getPlayer() instanceof ServerPlayer player)) return;
        ServerLevel level = (ServerLevel) context.getLevel();
        double x = player.getX();
        double y = player.getY();
        double z = player.getZ();
        float yaw = player.getYRot();
        float pitch = player.getXRot();

        Checks.check("teleportTo", () -> {
            player.teleportTo(x + 1, y, z);
            boolean moved = player.getX() == x + 1;
            player.teleportTo(x, y, z);
            return moved && player.getX() == x;
        });
        Checks.check("teleportRelative", () -> {
            player.teleportRelative(0, 0, 2);
            boolean moved = player.getZ() == z + 2;
            player.teleportTo(x, y, z);
            return moved && player.getZ() == z;
        });
        Checks.check("snapTo3", () -> {
            player.snapTo(x, y, z + 1);
            boolean moved = player.getZ() == z + 1;
            player.snapTo(x, y, z);
            return moved && player.getZ() == z;
        });
        Checks.check("snapTo5", () -> {
            player.snapTo(x, y, z, 45.0F, 10.0F);
            boolean turned = player.getYRot() == 45.0F && player.getXRot() == 10.0F;
            player.snapTo(x, y, z, yaw, pitch);
            return turned && player.getYRot() == yaw;
        });
        Checks.check("setDeltaMovement", () -> {
            player.setDeltaMovement(new Vec3(0, 0, 0));
            return true;
        });
        Checks.check("hurtServer", () -> {
            player.hurtServer(level, player.damageSources().generic(), 0.0F);
            return true;
        });
        Checks.check("heal", () -> {
            player.heal(1.0F);
            return true;
        });
        Checks.check("addEffect", () -> player.addEffect(new MobEffectInstance(MobEffects.SPEED, 40, 0)));
        Checks.check("removeEffect", () -> player.removeEffect(MobEffects.SPEED) && !player.removeEffect(MobEffects.SPEED));
        Checks.check("removeAllEffects", () -> {
            player.addEffect(new MobEffectInstance(MobEffects.SPEED, 40, 0));
            return player.removeAllEffects() && !player.removeAllEffects();
        });
        Checks.check("swing", () -> {
            player.swing(InteractionHand.MAIN_HAND, SwingAnimation.DEFAULT, true);
            return true;
        });
        Checks.check("changeGameModeForPlayer", () -> {
            GameType was = player.gameMode();
            GameType other = was == GameType.SURVIVAL ? GameType.CREATIVE : GameType.SURVIVAL;
            boolean changed = player.gameMode.changeGameModeForPlayer(other);
            boolean now = player.gameMode() == other;
            boolean same = !player.gameMode.changeGameModeForPlayer(other);
            player.gameMode.changeGameModeForPlayer(was);
            return changed && now && same && player.gameMode() == was;
        });
        Checks.check("setGameMode", () -> {
            GameType was = player.gameMode();
            GameType other = was == GameType.ADVENTURE ? GameType.SURVIVAL : GameType.ADVENTURE;
            boolean changed = player.setGameMode(other);
            boolean now = player.gameMode() == other;
            player.setGameMode(was);
            return changed && now && player.gameMode() == was;
        });
        Checks.check("destroyBlock", () -> {
            BlockPos target = area.above(2);
            level.setBlock(target, Blocks.STONE.defaultBlockState(), 3);
            return player.gameMode.destroyBlock(target) && level.getBlockState(target).isAir();
        });
        Checks.check("addCooldown", () -> {
            player.getCooldowns().addCooldown(Identifier.parse("minecraft:structure_void"), 5);
            return true;
        });
        Checks.check("removeCooldown", () -> {
            player.getCooldowns().removeCooldown(Identifier.parse("minecraft:structure_void"));
            return true;
        });
        Checks.check("FoodData.eat", () -> {
            player.getFoodData().eat(0, 0.0F);
            player.getFoodData().eat(new FoodProperties(0, 0.0F, false));
            return true;
        });
        Checks.check("FoodData.set", () -> {
            player.getFoodData().setFoodLevel(20);
            player.getFoodData().setSaturation(5.0F);
            player.getFoodData().addExhaustion(0.0F);
            return true;
        });

        // The calls below keep Java's own body, so they run after the checks that refill the view.
        Checks.check("Inventory.add", () -> {
            Inventory inventory = player.getInventory();
            ItemStack dirt = new ItemStack(Items.DIRT, 3);
            return inventory.add(dirt) && inventory.contains(new ItemStack(Items.DIRT)) && dirt.isEmpty();
        });
        Checks.check("Inventory.getItem_setItem", () -> {
            Inventory inventory = player.getInventory();
            int slot = inventory.getFreeSlot();
            if (slot < 0) return false;
            inventory.setItem(slot, new ItemStack(Items.STICK, 2));
            boolean read = inventory.getItem(slot).is(Items.STICK) && inventory.getItem(slot).getCount() == 2;
            inventory.setItem(slot, ItemStack.EMPTY);
            return read && inventory.getFreeSlot() >= 0;
        });
        Checks.check("giveExperienceLevels", () -> {
            int level0 = player.experienceLevel;
            player.giveExperienceLevels(1);
            boolean up = player.experienceLevel == level0 + 1;
            player.giveExperienceLevels(-1);
            return up && player.experienceLevel == level0 && player.getXpNeededForNextLevel() > 0;
        });
        Checks.check("mayUseItemAt", () -> player.mayUseItemAt(area, net.minecraft.core.Direction.UP, new ItemStack(Items.DIRT)));
        Checks.check("playSound", () -> {
            player.playSound(net.minecraft.sounds.SoundEvents.UI_BUTTON_CLICK.value(), 1.0F, 1.0F);
            return true;
        });
        Checks.check("drop", () -> {
            player.drop(new ItemStack(Items.STICK), false, Prediction.SERVER_ONLY);
            return true;
        });
        Checks.check("onUpdateAbilities", () -> {
            player.onUpdateAbilities();
            return true;
        });
        Checks.check("getDestroySpeed", () -> {
            player.getDestroySpeed(Blocks.STONE.defaultBlockState());
            return !player.isSwimming();
        });
    }
}
