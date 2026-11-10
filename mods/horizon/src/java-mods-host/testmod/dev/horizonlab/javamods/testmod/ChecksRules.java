package dev.horizonlab.javamods.testmod;

import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.item.context.UseOnContext;

final class ChecksRules {
    private ChecksRules() {}

    static void run(UseOnContext context, BlockPos area) {
        Checks.check("canSpreadFireAround", () -> {
            ServerLevel level = (ServerLevel) context.getLevel();
            return level.canSpreadFireAround(area) && !level.canSpreadFireAround(area.offset(1000, 0, 0));
        });
        Checks.check("setChunkForced", () -> {
            ServerLevel level = (ServerLevel) context.getLevel();
            int cx = area.getX() >> 4;
            int cz = area.getZ() >> 4;
            return level.setChunkForced(cx, cz, true) && !level.setChunkForced(cx, cz, true)
                && level.setChunkForced(cx, cz, false) && !level.setChunkForced(cx, cz, false);
        });
        Checks.check("resetWeatherCycle", () -> {
            ((ServerLevel) context.getLevel()).resetWeatherCycle();
            return true;
        });
    }
}
