package dev.horizonlab.javamods.testmod;

import net.minecraft.core.BlockPos;
import net.minecraft.world.item.context.UseOnContext;

final class ChecksServer {
    private ChecksServer() {}

    static void run(UseOnContext context, BlockPos area) {
        var level = context.getLevel();
        var server = level.getServer();
        Checks.check("effective_side_server", () -> net.neoforged.fml.util.thread.EffectiveSide.get().isServer());
        Checks.check("server_same_thread", () -> server.isSameThread());
        Checks.check("level_thread_is_this", () -> level.getBlockState(area) != null);
        Checks.check("server_current", () -> net.neoforged.neoforge.server.ServerLifecycleHooks.getCurrentServer() == server);
        Checks.check("server_tick_count", () -> server.getTickCount() > 0);
        Checks.check("server_execute", () -> {
            server.execute(() -> System.out.println("CHECK server_task_ran ok"));
            return true;
        });
        Checks.check("server_players", () -> server.getPlayerList().getPlayers().contains(context.getPlayer()));
        Checks.check("server_world_folder", () -> server.getWorldPath(net.minecraft.world.level.storage.LevelResource.ROOT).resolve("level.dat").toFile().exists());
        Checks.check("server_seed", () -> server.getWorldGenSettings().options().seed() == ((net.minecraft.server.level.ServerLevel) level).getSeed());
        Checks.check("saved_data", () -> level.getServer().getDataStorage() != null && ((net.minecraft.server.level.ServerLevel) level).getDataStorage() != null);
        Checks.check("recipes_loaded", () -> server.getRecipeManager().getRecipes().size() > 100);
    }
}
