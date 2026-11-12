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

        Checks.check("profile_resolver_player", () -> {
            var id = context.getPlayer().getUUID();
            return server.services().profileResolver().fetchById(id).map(profile -> profile.id().equals(id)).orElse(false);
        });
        Checks.check("profile_resolver_unknown", () -> server.services().profileResolver().fetchById(new java.util.UUID(1, 2)).isEmpty());
        Checks.check("fake_player_for_profile", () -> {
            var profile = context.getPlayer().getGameProfile();
            return net.neoforged.neoforge.common.util.FakePlayerFactory.get((net.minecraft.server.level.ServerLevel) level, profile).getUUID().equals(profile.id());
        });
        Checks.check("mod_command_registered", () -> server.getCommands().getDispatcher().getRoot().getChild("horizon_java_test") != null);
        Checks.check("commands_selection_follows_dist", () -> TestMod.commandSelection == (net.neoforged.fml.loading.FMLEnvironment.getDist().isClient() ? net.minecraft.commands.Commands.CommandSelection.INTEGRATED : net.minecraft.commands.Commands.CommandSelection.DEDICATED));
        Checks.check("craft_facts", () -> craftFacts(server));
        Checks.check("tool_enchantments", () -> toolEnchantments(level));
    }

    /** What the loot roll's tool copy holds: a charm's code answers fortune 3 and silk touch 1, a plain pickaxe nothing. */
    private static boolean toolEnchantments(net.minecraft.world.level.Level level) {
        try {
            var type = Class.forName("dev.horizonlab.javamods.boot.Loot", true, ClassLoader.getSystemClassLoader());
            var effective = type.getMethod("effective", net.minecraft.world.item.ItemStack.class, net.minecraft.core.RegistryAccess.class);
            var charm = new net.minecraft.world.item.ItemStack(net.minecraft.core.registries.BuiltInRegistries.ITEM.getValue(net.minecraft.resources.Identifier.parse("horizon_java_test:charm")));
            var charmed = (net.minecraft.world.item.ItemStack) effective.invoke(null, charm, level.registryAccess());
            var plain = (net.minecraft.world.item.ItemStack) effective.invoke(null, new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.DIAMOND_PICKAXE), level.registryAccess());
            var levels = charmed.get(net.minecraft.core.component.DataComponents.ENCHANTMENTS);
            var fortune = level.registryAccess().lookupOrThrow(net.minecraft.core.registries.Registries.ENCHANTMENT).getOrThrow(net.minecraft.world.item.enchantment.Enchantments.FORTUNE);
            var silk = level.registryAccess().lookupOrThrow(net.minecraft.core.registries.Registries.ENCHANTMENT).getOrThrow(net.minecraft.world.item.enchantment.Enchantments.SILK_TOUCH);
            return levels.getLevel(fortune) == 3 && levels.getLevel(silk) == 1 && levels.size() == 2 && plain.get(net.minecraft.core.component.DataComponents.ENCHANTMENTS).isEmpty();
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    private static String text(java.io.DataInputStream in) throws java.io.IOException {
        return new String(in.readNBytes(in.readInt()), java.nio.charset.StandardCharsets.UTF_8);
    }

    /** The facts the boot answers, read as Rust reads them, hold what the test mod's crafting content changes. */
    private static boolean craftFacts(net.minecraft.server.MinecraftServer server) {
        try {
            return readCraftFacts(server);
        } catch (Exception e) {
            throw new IllegalStateException(e);
        }
    }

    private static boolean readCraftFacts(net.minecraft.server.MinecraftServer server) throws Exception {
        var type = Class.forName("dev.horizonlab.javamods.boot.CraftFacts", true, ClassLoader.getSystemClassLoader());
        var in = new java.io.DataInputStream(new java.io.ByteArrayInputStream((byte[]) type.getMethod("bin", net.minecraft.server.MinecraftServer.class).invoke(null, server)));
        boolean crafting = (in.readInt() & 1) != 0;
        boolean crafted = (in.readInt() & 1) != 0;
        int charm = 0;
        for (int n = in.readInt(); n > 0; n--) {
            String id = text(in);
            int bits = in.readInt();
            if (id.equals("horizon_java_test:charm")) charm = bits;
        }
        boolean sand = false;
        for (int n = in.readInt(); n > 0; n--) {
            text(in);
            boolean any = in.readBoolean();
            java.util.Set<String> tables = new java.util.HashSet<>();
            for (int t = in.readInt(); t > 0; t--) tables.add(text(in));
            sand |= !any && tables.equals(java.util.Set.of("minecraft:blocks/sand"));
        }
        java.util.Set<String> functions = new java.util.HashSet<>();
        for (int n = in.readInt(); n > 0; n--) functions.add(text(in));
        java.util.Set<String> conditions = new java.util.HashSet<>();
        for (int n = in.readInt(); n > 0; n--) conditions.add(text(in));
        return crafting && crafted && (charm & 0b1101) == 0b1101 && sand && functions.contains("horizon_java_test:stamp") && conditions.contains("horizon_java_test:always");
    }
}
