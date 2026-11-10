package dev.horizonlab.javamods.testmod;

import net.minecraft.core.BlockPos;
import net.minecraft.core.SectionPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.LightLayer;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.material.Fluids;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.ticks.TickPriority;

final class ChecksLevelApi {
    private ChecksLevelApi() {}

    static void run(UseOnContext context, BlockPos area) {
        ServerLevel level = (ServerLevel) context.getLevel();
        BlockPos top = area.atY(level.getMaxY() - 1);
        Checks.check("hasChunk", () -> level.hasChunk(area.getX() >> 4, area.getZ() >> 4) && !level.hasChunk(1_000_000, 1_000_000)
            && level.hasChunkAt(area) && !level.hasChunkAt(area.offset(16_000_000, 0, 0)) && level.hasChunksAt(area, area.above(5)));
        Checks.check("getBrightness", () -> level.getBrightness(LightLayer.SKY, top) == 15 && level.getBrightness(LightLayer.BLOCK, top) == 0
            && level.canSeeSky(top) && level.getEffectiveSkyBrightness(top) == 15 - level.getSkyDarken());
        Checks.check("getBiome", () -> {
            var biome = level.getBiome(area);
            return biome.isBound() && biome.unwrapKey().isPresent() && level.getNoiseBiome(area.getX() >> 2, area.getY() >> 2, area.getZ() >> 2).isBound()
                && level.getUncachedNoiseBiome(area.getX() >> 2, area.getY() >> 2, area.getZ() >> 2).isBound();
        });
        Checks.check("isRainingAt", () -> {
            level.isRainingAt(top);
            return level.precipitationAt(top) != null;
        });
        Checks.check("getSeaLevel", () -> level.getSeaLevel() == 63);
        Checks.check("getSeed", () -> {
            level.getSeed();
            return true;
        });
        Checks.check("rain", () -> {
            float rain = level.getRainLevel(1.0F);
            float thunder = level.getThunderLevel(1.0F);
            return rain >= 0.0F && rain <= 1.0F && thunder <= rain && level.isRaining() == (rain > 0.2F) && level.isThundering() == (thunder > 0.9F);
        });
        Checks.check("isBrightOutside", () -> level.isBrightOutside() != level.isDarkOutside());
        Checks.check("clockTime", () -> level.getOverworldClockTime() >= 0 && level.getDefaultClockTime() == level.getOverworldClockTime());
        Checks.check("getDifficulty", () -> {
            var here = level.getCurrentDifficultyAt(area);
            float moon = level.getMoonBrightness(area);
            return level.getDifficulty() != null && here.getDifficulty() == level.getDifficulty() && here.getEffectiveDifficulty() >= 0.0F && moon >= 0.0F && moon <= 1.0F;
        });
        Checks.check("getRespawnData", () -> level.getRespawnData().pos() != null && level.getRespawnData().dimension() == ServerLevel.OVERWORLD);
        Checks.check("noBlockCollision", () -> {
            BlockPos stone = area.above(2);
            BlockPos water = area.above(4);
            level.setBlock(stone, Blocks.STONE.defaultBlockState(), 3);
            level.setBlock(water, Blocks.WATER.defaultBlockState(), 3);
            return !level.noBlockCollision(null, new AABB(stone).deflate(0.1)) && level.noBlockCollision(null, new AABB(area.above(6)).deflate(0.1))
                && level.noBlockCollision(null, new AABB(water).deflate(0.1)) && !level.noBlockCollision(null, new AABB(water).deflate(0.1), true);
        });
        Checks.check("hasNearbyAlivePlayer", () -> {
            var at = context.getPlayer().position();
            return level.hasNearbyAlivePlayer(at.x, at.y, at.z, 4.0) && level.hasNearbyAlivePlayer(at.x + 1_000.0, at.y, at.z, -1.0)
                && !level.hasNearbyAlivePlayer(at.x + 1_000.0, at.y, at.z, 4.0);
        });
        Checks.check("scheduleTick", () -> {
            BlockPos at = area.above(8);
            level.scheduleTick(at, Blocks.STONE, 5, TickPriority.HIGH);
            level.scheduleTick(at, Fluids.WATER, 5);
            level.scheduleTick(at, Fluids.LAVA, 5, TickPriority.LOW);
            return true;
        });
        Checks.check("sectionsToVillage", () -> {
            int sections = level.sectionsToVillage(SectionPos.of(area));
            return sections >= 0 && sections <= 7 && level.isVillage(area) == (sections <= 1);
        });
        Checks.check("getBlockCollisions", () -> {
            BlockPos stone = area.above(10);
            level.setBlock(stone, Blocks.STONE.defaultBlockState(), 3);
            int boxes = 0;
            for (var shape : level.getBlockCollisions(null, new AABB(stone).deflate(0.1))) boxes += shape.isEmpty() ? 0 : 1;
            return boxes == 1 && level.getChunkForCollisions(1_000_000, 1_000_000) == null;
        });
        Checks.check("isRaided", () -> !level.isRaided(area));
        Checks.check("blockEvent", () -> {
            level.blockEvent(area.above(11), Blocks.STONE, 0, 0);
            return true;
        });
        Checks.check("getForceLoadedChunks", () -> {
            int cx = area.getX() >> 4;
            int cz = area.getZ() >> 4;
            boolean forced = level.setChunkForced(cx, cz, true) && level.getForceLoadedChunks().contains(ChunkPos.pack(cx, cz));
            level.setChunkForced(cx, cz, false);
            return forced && !level.getForceLoadedChunks().contains(ChunkPos.pack(cx, cz));
        });
        Checks.check("globalLevelEvent", () -> {
            level.globalLevelEvent(9999, area, 0);
            return true;
        });
    }
}
