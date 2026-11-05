package dev.horizonlab.javamods.testmod;

import net.minecraft.client.Minecraft;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.common.NeoForge;

/** The client's profile resolver answers from the player info the server's packets gave it. */
final class ClientProfiles {
    private ClientProfiles() {}

    private static boolean done;

    static void register() {
        NeoForge.EVENT_BUS.addListener((ClientTickEvent.Post event) -> {
            Minecraft minecraft = Minecraft.getInstance();
            if (done || minecraft.player == null || minecraft.getConnection() == null || minecraft.getConnection().getPlayerInfo(minecraft.player.getUUID()) == null) return;
            done = true;
            var id = minecraft.player.getUUID();
            Checks.check("client_profile_resolver", () -> minecraft.services().profileResolver().fetchById(id).map(profile -> profile.id().equals(id)).orElse(false)
                    && minecraft.services().profileResolver().fetchById(new java.util.UUID(1, 2)).isEmpty());
        });
    }
}
