package dev.horizonlab.javamods.testmod;

import net.minecraft.commands.CommandSourceStack;
import net.minecraft.commands.Commands;
import net.minecraft.network.chat.Component;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.CommandEvent;
import net.neoforged.neoforge.event.RegisterCommandsEvent;

/** The test mod's commands and its `CommandEvent` listener, for `java_mods.py bridges`. */
final class TestCommands {
    private TestCommands() {}

    static volatile Commands.CommandSelection selection;
    private static int daytimePosts;

    static void listen() {
        NeoForge.EVENT_BUS.addListener(TestCommands::register);
        NeoForge.EVENT_BUS.addListener(TestCommands::posted);
    }

    private static int say(CommandSourceStack source, String text) {
        source.sendSuccess(() -> Component.literal(text), false);
        return 1;
    }

    private static void register(RegisterCommandsEvent event) {
        selection = event.getCommandSelection();
        event.getDispatcher().register(Commands.literal("horizon_java_test")
                .executes(context -> 1)
                .then(Commands.literal("ping").executes(context -> {
                    var source = context.getSource();
                    if (source.isPlayer()) Checks.check("mod_command_runs_in_java", () -> source.getEntity() instanceof ServerPlayer);
                    return say(source, "pong " + source.getTextName());
                }))
                .then(Commands.literal("op").requires(Commands.hasPermission(Commands.LEVEL_GAMEMASTERS)).executes(context -> say(context.getSource(), "op ok")))
                .then(Commands.literal("refuse").executes(context -> say(context.getSource(), "refuse ran")))
                .then(Commands.literal("give_probe").executes(context -> {
                    Checks.check("command_event_once", () -> daytimePosts == 1);
                    context.getSource().getPlayerOrException().getInventory().add(new ItemStack(Items.DIAMOND));
                    return 1;
                })));
        event.getDispatcher().register(Commands.literal("time").then(Commands.literal("horizon_java_test_sub").executes(context -> say(context.getSource(), "sub ran"))));
    }

    private static void posted(CommandEvent event) {
        String input = event.getParseResults().getReader().getString();
        if (input.equals("horizon_java_test refuse")) event.setCanceled(true);
        if (input.equals("time query daytime")) {
            daytimePosts++;
            Checks.check("command_event_for_native", () -> event.getParseResults().getContext().getSource().getEntity() instanceof ServerPlayer);
        }
    }
}
