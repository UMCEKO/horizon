package dev.horizonlab.javamods.testmod;

import net.minecraft.network.chat.Component;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.ServerChatEvent;

/** `ServerChatEvent`, which native posts when a guest's message reaches the chat decorator. */
final class ChatEvents {
    private ChatEvents() {}

    static void register() {
        NeoForge.EVENT_BUS.addListener((ServerChatEvent event) -> {
            Checks.check("server_chat_event_player", () -> event.getUsername().equals(event.getPlayer().getGameProfile().name()));
            String raw = event.getRawText();
            if (raw.equals("horizon server edit")) event.setMessage(Component.literal("horizon server edited"));
            if (raw.equals("horizon server cancel")) event.setCanceled(true);
        });
    }
}
