package dev.horizonlab.javamods.testmod;

import net.minecraft.client.Minecraft;
import net.minecraft.client.gui.screens.inventory.InventoryScreen;
import net.minecraft.world.inventory.ContainerInput;
import net.neoforged.neoforge.client.event.ScreenEvent;
import net.neoforged.neoforge.common.NeoForge;

/** A mod's own click on the inventory shell, as Inventory Profiles Next makes them: the harness checks the server moved the stack. */
final class ClientShellClick {
    private ClientShellClick() {}

    private static boolean done;

    static void register() {
        if (System.getProperty("horizon.test.shellclick") == null) return;
        NeoForge.EVENT_BUS.addListener((ScreenEvent.Init.Post event) -> {
            if (done || !(event.getScreen() instanceof InventoryScreen)) return;
            done = true;
            Minecraft minecraft = Minecraft.getInstance();
            Checks.check("shell_mod_click_reaches_native", () -> {
                minecraft.gameMode.handleContainerInput(0, 36, 0, ContainerInput.QUICK_MOVE, minecraft.player);
                return true;
            });
        });
    }
}
