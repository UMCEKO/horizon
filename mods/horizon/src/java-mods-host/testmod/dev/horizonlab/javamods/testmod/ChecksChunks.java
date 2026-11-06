package dev.horizonlab.javamods.testmod;

import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.server.level.ChunkTrackingView;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.entity.decoration.ArmorStand;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.ChunkPos;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.chunk.status.ChunkStatus;
import net.minecraft.world.phys.AABB;

/** `LevelChunk.setBlockState`'s flags, and `ChunkMap` and `ServerChunkCache` as Horizon's hub and tickets answer them. */
final class ChecksChunks {
    private ChecksChunks() {}

    static void run(UseOnContext context, BlockPos area) {
        ServerLevel level = (ServerLevel) context.getLevel();
        ServerPlayer player = (ServerPlayer) context.getPlayer();
        var chunks = level.getChunkSource();
        var map = chunks.chunkMap;
        int x = player.chunkPosition().x();
        int z = player.chunkPosition().z();
        long here = ChunkPos.pack(x, z);
        long far = ChunkPos.pack(x + 200, z);
        Checks.check("shouldTickBlocksAt", () -> level.shouldTickBlocksAt(here) && !level.shouldTickBlocksAt(far));
        Checks.check("isPositionTicking", () -> chunks.isPositionTicking(here) && !chunks.isPositionTicking(far));
        Checks.check("getChunk without load", () -> chunks.getChunk(x, z, ChunkStatus.FULL, false) != null && chunks.getChunk(x + 200, z, ChunkStatus.FULL, false) == null);
        Checks.check("getChunk loads", () -> {
            int cx = x + 48;
            return !level.hasChunk(cx, z) && chunks.getChunk(cx, z, ChunkStatus.FULL, true) != null && level.hasChunk(cx, z) && chunks.getChunk(cx, z, ChunkStatus.FULL, false) != null;
        });
        Checks.check("chunk status", () -> level.getChunk(x, z).getFullStatus().isOrAfter(net.minecraft.server.level.FullChunkStatus.BLOCK_TICKING));
        Checks.check("chunk tracking view", () -> player.getChunkTrackingView() instanceof ChunkTrackingView.Positioned view && view.center().equals(player.chunkPosition()) && view.contains(x, z) && !view.contains(x + 200, z));
        Checks.check("isChunkTracked", () -> map.isChunkTracked(player, x, z) && !map.isChunkTracked(player, x + 200, z));
        Checks.check("getPlayers", () -> map.getPlayers(new ChunkPos(x, z), false).contains(player) && map.getPlayers(new ChunkPos(x + 200, z), false).isEmpty());
        var stands = level.getEntitiesOfClass(ArmorStand.class, new AABB(area).inflate(40));
        Checks.check("isTrackedByAnyPlayer", () -> !stands.isEmpty() && map.isTrackedByAnyPlayer(stands.get(0)) && !map.isTrackedByAnyPlayer(player));
        Checks.check("sendToTrackingPlayersAndSelf", () -> {
            var packet = new net.minecraft.network.protocol.game.ClientboundSetTimePacket(0L, java.util.Map.of());
            chunks.sendToTrackingPlayersAndSelf(player, packet);
            chunks.sendToTrackingPlayers(stands.get(0), packet);
            return true;
        });
        BlockState battery = Battery.BLOCK.get().defaultBlockState();
        BlockPos at = area.above(2);
        BlockPos second = at.east(2);
        Checks.check("onPlace runs", () -> {
            int places = Battery.places;
            level.setBlock(at, battery, 3);
            return Battery.places == places + 1 && level.getBlockEntity(at) instanceof Battery.Entity;
        });
        Checks.check("a removed block entity leaves its chunk", () -> {
            BlockPos spot = at.north(2);
            level.setBlock(spot, battery, 3);
            BlockEntity entity = level.getBlockEntity(spot);
            if (!(entity instanceof Battery.Entity)) return false;
            entity.setRemoved();
            // `LevelChunk.getBlockEntity`: a removed one is dropped and answers null, not loaded again.
            boolean gone = level.getBlockEntity(spot) == null;
            level.setBlock(spot, Blocks.AIR.defaultBlockState(), 3);
            return gone;
        });
        Checks.check("neighborChanged hears its neighbour", () -> {
            // `Level.markAndNotifyBlock` names the block that was there.
            level.setBlock(at.above(), Blocks.GLASS.defaultBlockState(), 3);
            boolean placed = Battery.neighbor == Blocks.AIR;
            level.setBlock(at.above(), Blocks.AIR.defaultBlockState(), 3);
            return placed && Battery.neighbor == Blocks.GLASS;
        });
        Checks.check("updateShape hears its neighbour", () -> {
            level.setBlock(at.above(), Blocks.GLASS.defaultBlockState(), 3);
            boolean heard = Battery.shapeDirection == Direction.UP && Battery.shapeNeighbor == Blocks.GLASS;
            level.setBlock(at.above(), Blocks.AIR.defaultBlockState(), 3);
            return heard && Battery.shapeNeighbor == Blocks.AIR;
        });
        Checks.check("UPDATE_SKIP_ON_PLACE", () -> {
            int places = Battery.places;
            level.setBlock(second, battery, 3 | 512);
            return Battery.places == places && level.getBlockEntity(second) instanceof Battery.Entity;
        });
        Checks.check("removal without UPDATE_NEIGHBORS", () -> {
            int removals = Battery.removals;
            int preRemoves = Battery.preRemoves;
            level.setBlock(second, Blocks.AIR.defaultBlockState(), 2);
            return Battery.removals == removals && Battery.preRemoves == preRemoves + 1 && level.getBlockEntity(second) == null;
        });
        Checks.check("UPDATE_MOVE_BY_PISTON", () -> {
            level.setBlock(second, battery, 3);
            int removals = Battery.removals;
            level.setBlock(second, Blocks.AIR.defaultBlockState(), 64);
            return Battery.removals == removals + 1 && Battery.removedByPiston;
        });
        Checks.check("UPDATE_SKIP_BLOCK_ENTITY_SIDEEFFECTS", () -> {
            int preRemoves = Battery.preRemoves;
            int removals = Battery.removals;
            level.setBlock(at, Blocks.AIR.defaultBlockState(), 3 | 256);
            return Battery.preRemoves == preRemoves && Battery.removals == removals + 1 && !Battery.removedByPiston && level.getBlockEntity(at) == null;
        });
    }

    /** Before anything asks for a block entity by position: Java took them as their chunks loaded, and let them go as those unloaded. */
    static void later(boolean reloaded) {
        if (!reloaded) return;
        Checks.check("onLoad at chunk load", () -> Battery.loads >= 2);
        Checks.check("onChunkUnloaded", () -> Battery.unloads >= 2);
    }
}
