package dev.horizonlab.javamods.testmod;

import net.minecraft.resources.Identifier;
import net.neoforged.neoforge.client.event.RenderGuiLayerEvent;
import net.neoforged.neoforge.client.event.sound.PlaySoundEvent;
import net.neoforged.neoforge.client.gui.VanillaGuiLayers;
import net.neoforged.neoforge.common.NeoForge;

/** Client events native posts from its hooks: a sound it is about to play, and a HUD layer's `Pre`. */
final class ClientEvents {
    private ClientEvents() {}

    static final Identifier BELL = Identifier.withDefaultNamespace("block.note_block.bell");
    private static boolean sound, layer;

    static void register() {
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
