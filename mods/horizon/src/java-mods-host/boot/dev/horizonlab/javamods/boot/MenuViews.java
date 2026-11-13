package dev.horizonlab.javamods.boot;

import java.io.DataOutputStream;
import java.io.IOException;
import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import net.minecraft.core.BlockPos;
import net.minecraft.core.NonNullList;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.Container;
import net.minecraft.world.MenuProvider;
import net.minecraft.world.SimpleContainer;
import net.minecraft.world.inventory.AbstractContainerMenu;
import net.minecraft.world.inventory.DataSlot;
import net.minecraft.world.inventory.ResultContainer;
import net.minecraft.world.inventory.TransientCraftingContainer;
import net.minecraft.world.item.ItemStack;

/** Vanilla menus native opened, as their real server menus on the player's view: what native's menu holds besides the inventory and the block entity (W12 SPEC-16). */
final class MenuViews {
    private MenuViews() {}

    /** A view's menu and what Java last heard of, or told native about, its menu-owned state. */
    private static final class View {
        final int id;
        final AbstractContainerMenu menu;
        ItemStack carried = ItemStack.EMPTY;
        final Map<Integer, ItemStack> slots = new HashMap<>();
        final Map<Integer, Integer> data = new HashMap<>();

        View(int id, AbstractContainerMenu menu) {
            this.id = id;
            this.menu = menu;
        }
    }

    private static final Map<UUID, View> VIEWS = new HashMap<>();
    /** The last fill's menu group, for the view `open` makes after that fill. */
    private static PlayerGroups pending;

    static boolean backed(AbstractContainerMenu menu) {
        for (var view : VIEWS.values()) {
            if (view.menu == menu) return true;
        }
        return false;
    }

    /** `menu_open`: the player fill, then `int container_id, text menu_type, byte source, long pos`; the reply is the use tail. */
    static void open(Crossing.In in, Crossing.Out out) throws Exception {
        var call = GameBridge.fill(in, false);
        var player = call.player();
        int id = call.in().readInt();
        String type = call.in().text();
        byte source = call.in().readByte();
        var pos = BlockPos.of(call.in().readLong());
        var level = call.level();
        var old = VIEWS.remove(player.getUUID());
        if (old != null && player.containerMenu == old.menu) player.containerMenu = player.inventoryMenu;
        MenuProvider provider = switch (source) {
            case 0 -> BlockEntities.get(level, pos) instanceof MenuProvider own ? own : null;
            case 1 -> level.getBlockState(pos).getMenuProvider(level, pos);
            default -> null;
        };
        AbstractContainerMenu menu = provider == null ? null : provider.createMenu(id, player.getInventory(), player);
        if (menu == null) {
            System.err.println("Horizon: no Java view of menu " + type + " (source " + source + ") at " + pos);
        } else {
            // Raw: native sent OpenScreen and syncs the client; native's open posts no ContainerOpenEvent either (W13's).
            player.containerMenu = menu;
            var view = new View(id, menu);
            var slots = dataSlots(menu);
            for (int i = 0; i < slots.size(); i++) view.data.put(i, slots.get(i).get());
            VIEWS.put(player.getUUID(), view);
            if (pending != null && pending.menuId == id) apply(view, pending);
        }
        pending = null;
        GameBridge.writeUseTail(player, out);
    }

    /** `menu_close`: the player fill, then `int container_id`; the reply is the use tail. */
    static void close(Crossing.In in, Crossing.Out out) throws Exception {
        var call = GameBridge.fill(in, false);
        var player = call.player();
        int id = call.in().readInt();
        var view = VIEWS.get(player.getUUID());
        if (view != null && view.id == id) {
            VIEWS.remove(player.getUUID());
            // No `removed`: native already returned the crafting grid's items, as `doCloseContainer` does.
            if (player.containerMenu == view.menu) player.containerMenu = player.inventoryMenu;
        }
        pending = null;
        GameBridge.writeUseTail(player, out);
    }

    /** A fill's menu group: into the open view if it is that menu, else kept for the view `open` makes next. */
    static void filled(ServerPlayer player, PlayerGroups g) throws IOException {
        pending = g.has(PlayerGroups.MENU) ? g : null;
        var view = VIEWS.get(player.getUUID());
        if (pending != null && view != null && view.id == g.menuId && player.containerMenu == view.menu) apply(view, g);
    }

    /** Raw writes, so no `slotsChanged` → `createResult` cascade runs that native didn't run. */
    private static void apply(View view, PlayerGroups g) throws IOException {
        var carried = stack(g.menuCarried);
        view.menu.setCarried(carried);
        view.carried = carried.copy();
        for (int i = 0; i < g.menuSlots.length; i++) {
            int index = g.menuSlots[i];
            if (index < 0 || index >= view.menu.slots.size()) continue;
            var slot = view.menu.slots.get(index);
            var items = items(slot.container);
            if (items == null) {
                System.err.println("Horizon: menu slot " + index + " of " + view.menu.getClass().getSimpleName() + " is in a " + slot.container.getClass().getName() + ", which the view can't write raw");
                continue;
            }
            var stack = stack(g.menuTags[i]);
            items.set(slot.getContainerSlot(), stack);
            view.slots.put(index, stack.copy());
        }
        var slots = dataSlots(view.menu);
        for (int i = 0; i < g.dataIds.length; i++) {
            int id = g.dataIds[i];
            if (id < 0 || id >= slots.size()) continue;
            slots.get(id).set(g.dataValues[i]);
            view.data.put(id, g.dataValues[i]);
        }
    }

    /** The reply's menu block after the player block: `int container_id` (-1: none, nothing follows), `bytes carried`, the menu-owned slots and data slots Java changed. */
    static void writeChanges(ServerPlayer player, DataOutputStream out) throws IOException {
        var view = VIEWS.get(player.getUUID());
        if (view == null || player.containerMenu != view.menu) {
            out.writeInt(-1);
            return;
        }
        var menu = view.menu;
        out.writeInt(view.id);
        var carried = menu.getCarried();
        writeStack(out, carried);
        view.carried = carried.copy();
        var changed = new ArrayList<Integer>();
        for (int index = 0; index < menu.slots.size(); index++) {
            var container = menu.slots.get(index).container;
            if (items(container) == null) continue;
            var now = menu.slots.get(index).getItem();
            if (!ItemStack.matches(view.slots.getOrDefault(index, ItemStack.EMPTY), now)) changed.add(index);
        }
        out.writeInt(changed.size());
        for (int index : changed) {
            var now = menu.slots.get(index).getItem();
            out.writeInt(index);
            writeStack(out, now);
            view.slots.put(index, now.copy());
        }
        var slots = dataSlots(menu);
        var data = new ArrayList<Integer>();
        for (int i = 0; i < slots.size(); i++) {
            Integer last = view.data.get(i);
            if (last == null || last != slots.get(i).get()) data.add(i);
        }
        out.writeInt(data.size());
        for (int i : data) {
            int value = slots.get(i).get();
            out.writeInt(i);
            out.writeInt(value);
            view.data.put(i, value);
        }
    }

    static void forget(UUID id) {
        VIEWS.remove(id);
    }

    static void reset() {
        VIEWS.clear();
        pending = null;
    }

    private static ItemStack stack(byte[] tag) throws IOException {
        return tag.length == 0 ? ItemStack.EMPTY : GameBridge.decode(tag);
    }

    private static void writeStack(DataOutputStream out, ItemStack stack) throws IOException {
        if (stack.isEmpty()) {
            out.writeInt(0);
            return;
        }
        var ops = GameBridge.registries().createSerializationContext(NbtOps.INSTANCE);
        GameBridge.writeTag(out, (CompoundTag) ItemStack.CODEC.encodeStart(ops, stack).getOrThrow());
    }

    private static final Field DATA_SLOTS = field(AbstractContainerMenu.class, "dataSlots");
    private static final Field SIMPLE_ITEMS = field(SimpleContainer.class, "items");
    private static final Field CRAFTING_ITEMS = field(TransientCraftingContainer.class, "items");
    private static final Field RESULT_ITEMS = field(ResultContainer.class, "itemStacks");

    @SuppressWarnings("unchecked")
    private static List<DataSlot> dataSlots(AbstractContainerMenu menu) {
        try {
            return (List<DataSlot>) DATA_SLOTS.get(menu);
        } catch (IllegalAccessException e) {
            throw new IllegalStateException(e);
        }
    }

    /** A menu-owned container's raw content list; null for the inventory, a block entity or any other container. */
    @SuppressWarnings("unchecked")
    private static NonNullList<ItemStack> items(Container container) {
        Field field;
        if (container instanceof SimpleContainer) field = SIMPLE_ITEMS;
        else if (container instanceof TransientCraftingContainer) field = CRAFTING_ITEMS;
        else if (container instanceof ResultContainer) field = RESULT_ITEMS;
        else return null;
        try {
            return (NonNullList<ItemStack>) field.get(container);
        } catch (IllegalAccessException e) {
            throw new IllegalStateException(e);
        }
    }

    private static Field field(Class<?> owner, String name) {
        try {
            var field = owner.getDeclaredField(name);
            field.setAccessible(true);
            return field;
        } catch (NoSuchFieldException e) {
            throw new IllegalStateException(e);
        }
    }
}
