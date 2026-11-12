package dev.horizonlab.javamods.testmod;

import net.minecraft.commands.Commands;
import net.minecraft.network.chat.Component;
import net.neoforged.neoforge.client.ClientCommandSourceStack;
import net.neoforged.neoforge.client.event.ClientChatEvent;
import net.neoforged.neoforge.client.event.RegisterClientCommandsEvent;
import net.neoforged.neoforge.common.NeoForge;

/** A client command and `ClientChatEvent` edits, reached from the window's chat box. */
final class ClientChat {
    private ClientChat() {}

    static void register() {
        NeoForge.EVENT_BUS.addListener((RegisterClientCommandsEvent event) -> {
            Checks.check("client_commands_registered", () -> true);
            event.getDispatcher().register(Commands.literal("horizon_java_test_client").executes(ctx -> {
                Checks.check("client_command_runs", () -> ctx.getSource() instanceof ClientCommandSourceStack);
                ctx.getSource().sendSuccess(() -> Component.literal("client pong"), false);
                return 1;
            }));
        });
        NeoForge.EVENT_BUS.addListener((ClientChatEvent event) -> {
            if (event.getMessage().equals("horizon chat edit")) event.setMessage("horizon chat edited");
            else if (event.getMessage().equals("horizon chat cancel")) event.setCanceled(true);
        });
    }
}
