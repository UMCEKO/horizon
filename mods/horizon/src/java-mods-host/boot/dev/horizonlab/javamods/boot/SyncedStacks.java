package dev.horizonlab.javamods.boot;

import java.io.IOException;
import java.util.Arrays;
import java.util.HashMap;
import java.util.Map;
import java.util.UUID;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;

/** A player view's stacks as Horizon last sent them: a slot whose bytes didn't change is not decoded again, and an inventory that already holds them is left as it is. */
final class SyncedStacks {
    interface Decoder {
        ItemStack decode(byte[] nbt) throws IOException;
    }

    private static final Map<UUID, SyncedStacks> BY_PLAYER = new HashMap<>();

    private final Player owner;
    private final byte[][] bytes;
    // Decoded once and never handed to mod code: the inventory gets copies.
    private final ItemStack[] stacks;

    private SyncedStacks(Player owner, int slots) {
        this.owner = owner;
        this.bytes = new byte[slots][];
        this.stacks = new ItemStack[slots];
        Arrays.fill(stacks, ItemStack.EMPTY);
    }

    static SyncedStacks of(Player player, int slots) {
        var synced = BY_PLAYER.get(player.getUUID());
        if (synced == null || synced.owner != player || synced.stacks.length != slots) {
            synced = new SyncedStacks(player, slots);
            BY_PLAYER.put(player.getUUID(), synced);
        }
        return synced;
    }

    static void forget(UUID player) {
        BY_PLAYER.remove(player);
    }

    static void reset() {
        BY_PLAYER.clear();
    }

    /** One call's stacks, `slot` to its saved form; slots it leaves out are empty. */
    final class Fill {
        private final boolean[] sent = new boolean[stacks.length];

        void slot(int slot, byte[] nbt, Decoder decoder) throws IOException {
            if (!Arrays.equals(bytes[slot], nbt)) {
                stacks[slot] = decoder.decode(nbt);
                bytes[slot] = nbt;
            }
            sent[slot] = true;
        }

        /** The stacks as sent, for the reply's diff; read only. */
        ItemStack[] apply(Inventory inventory) {
            for (int slot = 0; slot < stacks.length; slot++) {
                if (!sent[slot]) {
                    bytes[slot] = null;
                    stacks[slot] = ItemStack.EMPTY;
                }
            }
            if (!holds(inventory)) {
                inventory.clearContent();
                for (int slot = 0; slot < stacks.length; slot++) {
                    if (!stacks[slot].isEmpty()) inventory.setItem(slot, stacks[slot].copy());
                }
            }
            return stacks.clone();
        }
    }

    Fill fill() {
        return new Fill();
    }

    /** Whether `inventory` already holds exactly these stacks, with nothing in the equipment slots it doesn't map. */
    private boolean holds(Inventory inventory) {
        if (!owner.getItemBySlot(EquipmentSlot.BODY).isEmpty() || !owner.getItemBySlot(EquipmentSlot.SADDLE).isEmpty()) return false;
        for (int slot = 0; slot < stacks.length; slot++) {
            if (!ItemStack.matches(inventory.getItem(slot), stacks[slot])) return false;
        }
        return true;
    }
}
