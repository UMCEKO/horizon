package dev.horizonlab.javamods.testmod;

import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.tick.PlayerTickEvent;

/** The acting player's view between ticks: its stacks stay the same objects, and a native change reaches it. */
final class PlayerView {
    private PlayerView() {}

    private static ItemStack lastSlot0;
    private static ItemStack lastCopy = ItemStack.EMPTY;
    private static boolean identityTold;
    private static int apples = -1;
    private static boolean appleTold;

    static void register() {
        NeoForge.EVENT_BUS.addListener((PlayerTickEvent.Post event) -> {
            if (!(event.getEntity() instanceof ServerPlayer player) || identityTold) return;
            ItemStack now = player.getInventory().getItem(0);
            if (!now.isEmpty() && lastSlot0 != null && ItemStack.matches(now, lastCopy)) {
                identityTold = true;
                var before = lastSlot0;
                Checks.check("player_stack_keeps_identity", () -> System.identityHashCode(now) == System.identityHashCode(before));
            }
            lastSlot0 = now;
            lastCopy = now.copy();
        });
        NeoForge.EVENT_BUS.addListener((PlayerTickEvent.Pre event) -> {
            if (!(event.getEntity() instanceof ServerPlayer player) || appleTold) return;
            var inventory = player.getInventory();
            int count = 0, slots = 0;
            for (int slot = 0; slot < inventory.getContainerSize(); slot++) {
                if (inventory.getItem(slot).is(Items.APPLE)) {
                    count += inventory.getItem(slot).getCount();
                    slots++;
                }
            }
            if (apples == 0 && count > 0) {
                appleTold = true;
                int seen = count, in = slots;
                Checks.check("player_view_sees_native_give", () -> seen == 1 && in == 1);
            }
            apples = count;
        });
    }
}
