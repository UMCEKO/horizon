package dev.horizonlab.javamods.testmod;

import net.minecraft.client.Minecraft;
import net.minecraft.network.chat.Component;
import net.minecraft.resources.Identifier;
import net.neoforged.neoforge.client.event.ClientChatReceivedEvent;
import net.neoforged.neoforge.client.event.RenderGuiLayerEvent;
import net.neoforged.neoforge.client.event.sound.PlaySoundEvent;
import net.neoforged.neoforge.client.gui.VanillaGuiLayers;
import net.neoforged.neoforge.common.NeoForge;

/** Client events native posts from its hooks: a sound it is about to play, a HUD layer's `Pre`, and the chat lines it shows. */
final class ClientEvents {
    private ClientEvents() {}

    static final Identifier BELL = Identifier.withDefaultNamespace("block.note_block.bell");
    private static boolean sound, layer;

    static void register() {
        NeoForge.EVENT_BUS.addListener((ClientChatReceivedEvent.System event) -> {
            String text = event.getMessage().getString();
            if (text.contains("horizon recv cancel")) event.setCanceled(true);
            else if (text.contains("horizon recv edit")) event.setMessage(Component.literal("horizon recv edited"));
            else if (event.isOverlay() && text.contains("horizon recv bar")) Checks.check("chat_received_overlay", () -> true);
        });
        NeoForge.EVENT_BUS.addListener((ClientChatReceivedEvent.Player event) -> {
            if (!event.getPlayerChatMessage().signedContent().equals("horizon recv player")) return;
            Checks.check("chat_received_player", () -> event.getSender().equals(Minecraft.getInstance().player.getUUID()));
        });
        NeoForge.EVENT_BUS.addListener((PlaySoundEvent event) -> {
            if (sound || event.getSound() == null || !event.getSound().getIdentifier().equals(BELL)) return;
            sound = true;
            event.setSound(null);
            Checks.check("play_sound_posted_from_native", () -> event.getOriginalSound().getIdentifier().equals(BELL));
        });
        NeoForge.EVENT_BUS.addListener((RenderGuiLayerEvent.Pre event) -> {
            if (!event.getName().equals(VanillaGuiLayers.AIR_LEVEL)) return;
            event.setCanceled(true);
            if (layer) return;
            layer = true;
            Checks.check("gui_layer_pre_cancel", event::isCanceled);
        });
    }
}
