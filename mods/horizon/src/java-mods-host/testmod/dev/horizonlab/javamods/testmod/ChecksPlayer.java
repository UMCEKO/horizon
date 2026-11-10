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

        Checks.check("a call's view looks where Horizon's player looks", () -> {
            var early = player.pick(5.0, 0.0F, false);
            var late = player.pick(5.0, 1.0F, false);
            return player.xo == x && player.yo == y && player.zo == z
                && player.getViewYRot(0.5F) == yaw && player.getViewXRot(0.5F) == pitch
                && player.yHeadRotO == player.getYHeadRot() && player.getYHeadRot() == yaw
                && Math.abs(player.getBoundingBox().getCenter().x - x) < 1e-9
                && Math.abs(player.getBoundingBox().getCenter().z - z) < 1e-9
                && early.getType() == late.getType() && early.getLocation().equals(late.getLocation());
        });

        Checks.check("getDestroyProgress runs the held item's speed and BreakSpeed", () -> {
            var inventory = player.getInventory();
            int slot = inventory.getSelectedSlot();
            ItemStack kept = inventory.getItem(slot);
            try {
                var dirt = Blocks.DIRT.defaultBlockState();
                inventory.setItem(slot, ItemStack.EMPTY);
                float bare = dirt.getDestroyProgress(player, level, area);
                inventory.setItem(slot, new ItemStack(net.minecraft.core.registries.BuiltInRegistries.ITEM.getValue(Identifier.parse("horizon_java_test:digger"))));
                float fast = dirt.getDestroyProgress(player, level, area);
                float halved = Blocks.GRAVEL.defaultBlockState().getDestroyProgress(player, level, area);
                float air = player.onGround() ? 1.0f : 0.2f;
                System.out.println("CHECK destroy progress bare=" + bare + " fast=" + fast + " halved=" + halved + " air=" + air);
                return Math.abs(bare - air / 0.5f / 30) < 1e-4 && Math.abs(fast - 100 * air / 0.5f / 30) < 1e-3 && Math.abs(halved - 50 * air / 0.6f / 30) < 1e-3;
            } finally {
                inventory.setItem(slot, kept);
            }
        });
        Checks.check("destroyBlock runs BreakEvent and its extra break; a canceled event keeps the block", () -> {
            level.setBlock(area, Blocks.IRON_BLOCK.defaultBlockState(), 3);
            level.setBlock(area.above(), Blocks.DIRT.defaultBlockState(), 3);
            boolean broke = player.gameMode.destroyBlock(area);
            boolean both = level.getBlockState(area).isAir() && level.getBlockState(area.above()).isAir();
            level.setBlock(area, Blocks.GILDED_BLACKSTONE.defaultBlockState(), 3);
            boolean canceled = !player.gameMode.destroyBlock(area) && level.getBlockState(area).is(Blocks.GILDED_BLACKSTONE);
            level.setBlock(area, Blocks.AIR.defaultBlockState(), 3);
            return broke && both && canceled;
        });
        Checks.check("ItemStack.mineBlock runs the item's own code", () -> {
            var stack = new ItemStack(net.minecraft.core.registries.BuiltInRegistries.ITEM.getValue(Identifier.parse("horizon_java_test:miner")));
            int before = TestMod.Miner.mined;
            stack.mineBlock(level, Blocks.STONE.defaultBlockState(), area, player);
            return TestMod.Miner.mined == before + 1;
        });
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
