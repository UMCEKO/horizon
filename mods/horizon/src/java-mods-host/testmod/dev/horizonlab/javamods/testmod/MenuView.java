package dev.horizonlab.javamods.testmod;

import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.inventory.AnvilMenu;
import net.minecraft.world.inventory.ChestMenu;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.tick.PlayerTickEvent;

/** The server player's open menu as its view holds it, for vanilla menus native opened. */
final class MenuView {
    private MenuView() {}

    private static boolean anvilTold, chestTold, closedTold;

    static void register() {
        NeoForge.EVENT_BUS.addListener((PlayerTickEvent.Post event) -> {
            if (!(event.getEntity() instanceof ServerPlayer player)) return;
            var menu = player.containerMenu;
            if (menu instanceof AnvilMenu anvil && !anvilTold && !anvil.getSlot(0).getItem().isEmpty()) {
                anvilTold = true;
                Checks.check("anvil_view_matches", () -> anvil.getSlot(0).getItem().is(Items.IRON_SWORD) && anvil.getCarried().isEmpty());
            } else if (menu instanceof ChestMenu chest && !chestTold) {
                chestTold = true;
                Checks.check("chest_view_is_the_block_entity", () -> chest.getContainer() instanceof BlockEntity entity && entity == player.level().getBlockEntity(entity.getBlockPos()));
            } else if (anvilTold && chestTold && !closedTold && menu == player.inventoryMenu) {
                closedTold = true;
                Checks.check("menu_view_closed", () -> player.containerMenu == player.inventoryMenu);
            }
        });
    }
}
