package dev.horizonlab.javamods.testmod;

import java.util.List;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.dispenser.BlockSource;
import net.minecraft.core.dispenser.DefaultDispenseItemBehavior;
import net.minecraft.core.dispenser.ProjectileDispenseBehavior;
import net.minecraft.core.particles.BlockParticleOption;
import net.minecraft.core.particles.DustParticleOptions;
import net.minecraft.core.particles.ItemParticleOption;
import net.minecraft.core.particles.ParticleTypes;
import net.minecraft.core.registries.Registries;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.Difficulty;
import net.minecraft.world.DifficultyInstance;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.BlockItem;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.context.BlockPlaceContext;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.item.enchantment.EnchantmentHelper;
import net.minecraft.world.item.enchantment.Enchantments;
import net.minecraft.world.level.Explosion;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LevelAccessor;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.DispenserBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.storage.loot.providers.number.ints.ResolvableInt;
import net.minecraft.world.damagesource.DamageTypes;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;

/** Drops, particles and explosions in a column of their own; items dropped are counted by the item entities around the spot. */
final class ChecksEffects {
    private ChecksEffects() {}

    private static int items(Level level, BlockPos at) {
        return level.getEntitiesOfClass(ItemEntity.class, new AABB(at).inflate(0.6)).size();
    }

    private static boolean dropsItem(Level level, BlockPos at, Runnable drop) {
        int before = items(level, at);
        drop.run();
        return items(level, at) > before;
    }

    private static boolean only(List<ItemStack> drops, net.minecraft.world.item.Item item) {
        return drops.size() == 1 && drops.get(0).is(item);
    }

    static void run(UseOnContext context, BlockPos area) {
        Level level = context.getLevel();
        ServerLevel server = (ServerLevel) level;
        Player player = context.getPlayer();
        BlockState dirt = Blocks.DIRT.defaultBlockState();
        BlockState stone = Blocks.STONE.defaultBlockState();
        Vec3 particleAt = Vec3.atCenterOf(area.above(10));

        Checks.check("getDrops", () -> only(Block.getDrops(dirt, server, area, null), Items.DIRT));
        Checks.check("getDrops of a tool", () -> {
            var silk = server.registryAccess().lookupOrThrow(Registries.ENCHANTMENT).getOrThrow(Enchantments.SILK_TOUCH);
            ItemStack pick = new ItemStack(Items.IRON_PICKAXE);
            EnchantmentHelper.updateEnchantments(pick, enchantments -> enchantments.set(silk, 1));
            return only(Block.getDrops(stone, server, area, null, null, pick), Items.STONE) && only(Block.getDrops(stone, server, area, null, null, new ItemStack(Items.IRON_PICKAXE)), Items.COBBLESTONE);
        });
        Checks.check("popResource", () -> dropsItem(level, area.above(), () -> Block.popResource(level, area.above(), new ItemStack(Items.DIAMOND))));
        Checks.check("popResource of nothing", () -> !dropsItem(level, area.above(), () -> Block.popResource(level, area.above(), ItemStack.EMPTY)));
        Checks.check("popResourceFromFace", () -> dropsItem(level, area.above(3), () -> Block.popResourceFromFace(level, area.above(3), Direction.UP, new ItemStack(Items.EMERALD))));
        Checks.check("dropResources", () -> dropsItem(level, area.above(5), () -> Block.dropResources(dirt, level, area.above(5))));
        Checks.check("dropResources of a block entity", () -> dropsItem(level, area.above(7), () -> Block.dropResources(dirt, (LevelAccessor) level, area.above(7), null)));
        Checks.check("spawnItem", () -> dropsItem(level, area.above(9), () -> DefaultDispenseItemBehavior.spawnItem(level, new ItemStack(Items.COBBLESTONE), 6, Direction.UP, Vec3.atCenterOf(area.above(9)))));

        BlockSource source = new BlockSource(server, area.above(2), Blocks.DISPENSER.defaultBlockState().setValue(DispenserBlock.FACING, Direction.UP), null);
        Checks.check("dispense", () -> {
            ItemStack left = new DefaultDispenseItemBehavior().dispense(source, new ItemStack(Items.COBBLESTONE, 2));
            return left.getCount() == 1;
        });
        Checks.check("dispense a projectile", () -> new ProjectileDispenseBehavior(Items.SNOWBALL).dispense(source, new ItemStack(Items.SNOWBALL, 2)).getCount() == 1);

        Checks.check("hurtAndBreak", () -> {
            ItemStack pick = new ItemStack(Items.DIAMOND_PICKAXE);
            pick.hurtAndBreak(5, server, null, broken -> {});
            return pick.getDamageValue() == 5;
        });
        Checks.check("hurtAndBreak by a player", () -> {
            ItemStack pick = new ItemStack(Items.DIAMOND_PICKAXE);
            ServerPlayer owner = (ServerPlayer) player;
            pick.hurtAndBreak(5, server, owner, broken -> {});
            return pick.getDamageValue() == (owner.hasInfiniteMaterials() ? 0 : 5);
        });
        Checks.check("hurtAndBreak breaks", () -> {
            ItemStack worn = new ItemStack(Items.WOODEN_PICKAXE);
            worn.setDamageValue(worn.getMaxDamage() - 1);
            boolean[] broke = {false};
            worn.hurtAndBreak(1, server, null, broken -> broke[0] = true);
            return broke[0] && worn.isEmpty();
        });

        Checks.check("sendParticles", () -> {
            int plain = server.sendParticles(ParticleTypes.CLOUD, particleAt.x, particleAt.y, particleAt.z, 3, 0.1, 0.1, 0.1, 0.02);
            int dust = server.sendParticles(new DustParticleOptions(0xFF0000, 1.0F), particleAt.x, particleAt.y, particleAt.z, 3, 0.1, 0.1, 0.1, 0.02);
            int block = server.sendParticles(new BlockParticleOption(ParticleTypes.BLOCK, stone), particleAt.x, particleAt.y, particleAt.z, 3, 0.1, 0.1, 0.1, 0.02);
            int axes = server.sendParticles(ParticleTypes.CLOUD, true, false, particleAt.x, particleAt.y, particleAt.z, 1, 0, 0, 0, 0.1, 0.2, 0.3, net.minecraft.network.protocol.game.ClientboundLevelParticlesPacket.RandomizationType.DEFAULT);
            return plain >= 0 && dust == plain && block == plain && axes >= plain;
        });
        Checks.check("sendParticles of an item sends none", () -> server.sendParticles(new ItemParticleOption(ParticleTypes.ITEM, Items.DIAMOND), particleAt.x, particleAt.y, particleAt.z, 3, 0.1, 0.1, 0.1, 0.02) == 0);

        BlockPos blast = area.offset(1, 5, 0);
        Checks.check("explode destroys", () -> {
            level.setBlock(blast, dirt, 3);
            level.explode(null, blast.getX() + 0.5, blast.getY() + 0.5, blast.getZ() + 0.5, 1.0F, Level.ExplosionInteraction.TNT);
            return level.getBlockState(blast).isAir();
        });
        BlockPos kept = area.offset(1, 7, 0);
        Checks.check("explode keeps", () -> {
            level.setBlock(kept, dirt, 3);
            level.explode(null, kept.getX() + 0.5, kept.getY() + 0.5, kept.getZ() + 0.5, 1.0F, false, Level.ExplosionInteraction.NONE);
            return level.getBlockState(kept).is(Blocks.DIRT);
        });
        BlockPos burnt = area.offset(1, 9, 0);
        Checks.check("explode of a level", () -> {
            level.setBlock(burnt, dirt, 3);
            server.explode(null, null, null, burnt.getX() + 0.5, burnt.getY() + 0.5, burnt.getZ() + 0.5, 1.0F, false, Level.ExplosionInteraction.BLOCK, ParticleTypes.EXPLOSION, ParticleTypes.EXPLOSION_EMITTER, net.minecraft.util.random.WeightedList.<net.minecraft.core.particles.ExplosionParticleInfo>of(), net.minecraft.sounds.SoundEvents.GENERIC_EXPLODE);
            return level.getBlockState(burnt).isAir();
        });

        Checks.check("damage sources", () -> level.damageSources().explosion(null, null).is(DamageTypes.EXPLOSION) && Explosion.getIndirectSourceEntity(null) == null);
        Checks.check("difficulty", () -> {
            float hard = new DifficultyInstance(Difficulty.HARD, 1_000_000L, 1_000_000L, 1.0F).getSpecialMultiplier();
            return hard >= 0.0F && hard <= 1.0F && new DifficultyInstance(Difficulty.PEACEFUL, 1_000_000L, 1_000_000L, 1.0F).getSpecialMultiplier() == 0.0F;
        });
        Checks.check("a number from an item", () -> ResolvableInt.getFromItem(new ItemStack(Items.STICK), DataComponents.FOOD, food -> null, null, 7) == 7);

        BlockPos ground = area.offset(0, 3, 0);
        Checks.check("place a block", () -> {
            level.setBlock(ground, stone, 3);
            var hit = new BlockHitResult(Vec3.atCenterOf(ground).add(0, 0.5, 0), Direction.UP, ground, false);
            var result = ((BlockItem) Items.COBBLESTONE).place(new BlockPlaceContext(player, InteractionHand.MAIN_HAND, new ItemStack(Items.COBBLESTONE), hit));
            return result.consumesAction() && level.getBlockState(ground.above()).is(Blocks.COBBLESTONE);
        });
    }
}
