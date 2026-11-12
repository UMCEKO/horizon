package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Collection;
import java.util.LinkedHashMap;
import java.util.List;
import net.minecraft.ChatFormatting;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.world.item.CreativeModeTab;
import net.minecraft.world.item.CreativeModeTabs;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.TooltipFlag;
import net.neoforged.neoforge.client.event.ClientPlayerNetworkEvent;
import net.neoforged.neoforge.client.gui.CreativeTabsScreenPage;
import net.neoforged.neoforge.common.CreativeModeTabRegistry;
import net.neoforged.neoforge.common.NeoForge;

/** The client's real creative tabs as data, built once at its join for both permission values: Horizon's native screen never calls back. */
final class CreativeTabs {
    private CreativeTabs() {}

    // How many sorted tabs the real screen's `init` puts on a page, read from a real client.
    private static final int PER_PAGE = 10;

    /** Each distinct stack once, by its saved form, with what `SessionSearchTrees` indexes for it. */
    private static final class Table {
        final RegistryAccess holders;
        // Keyed by the saved form: a mod component's own hashCode needn't be stable between calls.
        final java.util.HashMap<CompoundTag, Integer> ids = new java.util.HashMap<>();
        final ByteArrayOutputStream bytes = new ByteArrayOutputStream();
        final DataOutputStream out = new DataOutputStream(bytes);

        Table(RegistryAccess holders) {
            this.holders = holders;
        }

        int id(ItemStack stack) throws IOException {
            CompoundTag saved;
            try {
                saved = (CompoundTag) ItemStack.CODEC.encodeStart(holders.createSerializationContext(NbtOps.INSTANCE), stack).getOrThrow();
            } catch (RuntimeException e) {
                System.err.println("Horizon: a creative tab's " + stack + " has no saved form");
                e.printStackTrace();
                return -1;
            }
            Integer known = ids.get(saved);
            if (known != null) return known;
            GameBridge.writeTag(out, saved);
            List<String> lines = lines(stack);
            out.writeInt(lines.size());
            for (String line : lines) text(out, line);
            List<String> tags = stack.tags().map(tag -> tag.location().toString()).toList();
            out.writeInt(tags.size());
            for (String tag : tags) text(out, tag);
            int id = ids.size();
            ids.put(saved, id);
            return id;
        }

        // `SessionSearchTrees.getTooltipLines`; a mod's tooltip that throws only leaves its stack to the name search.
        private List<String> lines(ItemStack stack) {
            try {
                return stack.getTooltipLines(Item.TooltipContext.of(holders), null, TooltipFlag.Default.NORMAL.asCreative()).stream()
                        .map(line -> ChatFormatting.stripFormatting(line.getString()).trim())
                        .filter(line -> !line.isEmpty())
                        .toList();
            } catch (RuntimeException e) {
                System.err.println("Horizon: the tooltip of a creative tab's " + stack + " failed");
                e.printStackTrace();
                return List.of();
            }
        }
    }

    private static void text(DataOutputStream out, String value) throws IOException {
        byte[] bytes = value.getBytes(StandardCharsets.UTF_8);
        out.writeInt(bytes.length);
        out.write(bytes);
    }

    private static void stacks(DataOutputStream out, Table table, Collection<ItemStack> stacks) throws IOException {
        var ids = new ArrayList<Integer>(stacks.size());
        for (ItemStack stack : stacks) {
            int id = table.id(stack);
            if (id >= 0) ids.add(id);
        }
        out.writeInt(ids.size());
        for (int id : ids) out.writeInt(id);
    }

    private record Place(int page, boolean top, int column) {}

    /** The tabs as they stand: a sorted tab is on its own page, the loader's fixed ones on every page. */
    private static byte[] set(Table table) throws IOException {
        var sorted = CreativeModeTabRegistry.getSortedCreativeModeTabs();
        var pages = new ArrayList<CreativeTabsScreenPage>();
        for (int from = 0; from < sorted.size(); from += PER_PAGE) {
            pages.add(new CreativeTabsScreenPage(new ArrayList<>(sorted.subList(from, Math.min(from + PER_PAGE, sorted.size())))));
        }
        var places = new LinkedHashMap<CreativeModeTab, Place>();
        for (int page = 0; page < pages.size(); page++) {
            var layout = pages.get(page);
            for (var tab : layout.getVisibleTabs()) {
                int seen = page;
                places.compute(tab, (key, known) -> new Place(known == null ? seen : -1, layout.isTop(key), layout.getColumn(key)));
            }
        }
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(Math.max(1, pages.size()));
        out.writeInt(places.size());
        var context = table.holders.createSerializationContext(NbtOps.INSTANCE);
        for (var entry : places.entrySet()) {
            var tab = entry.getKey();
            var place = entry.getValue();
            text(out, BuiltInRegistries.CREATIVE_MODE_TAB.getKey(tab).toString());
            var title = new CompoundTag();
            title.put("v", ComponentSerialization.CODEC.encodeStart(context, tab.getDisplayName()).getOrThrow());
            GameBridge.writeTag(out, title);
            var icon = tab.getIconItem();
            GameBridge.writeTag(out, icon.isEmpty() ? new CompoundTag() : (CompoundTag) ItemStack.CODEC.encodeStart(context, icon).getOrThrow());
            text(out, tab.getBackgroundTexture().toString());
            text(out, tab.getType().name());
            out.writeBoolean(place.top);
            out.writeInt(place.column);
            out.writeBoolean(tab.isAlignedRight());
            out.writeInt(place.page);
            out.writeBoolean(tab.canScroll());
            out.writeBoolean(tab.showTitle());
            out.writeBoolean(tab.hasSearchBar());
            out.writeInt(tab.getSearchBarWidth());
            out.writeInt(tab.getLabelColor());
            text(out, tab.getScrollerSprite().toString());
            stacks(out, table, tab.getDisplayItems());
            stacks(out, table, tab.getSearchTabDisplayItems());
        }
        return bytes.toByteArray();
    }

    private static volatile byte[] built;

    /** The one build per login: the login packet has given the client its player, registries and feature flags. */
    static void install() {
        NeoForge.EVENT_BUS.addListener(ClientPlayerNetworkEvent.LoggingIn.class, event -> {
            try {
                built = build(event.getPlayer());
            } catch (Throwable t) {
                System.err.println("Horizon: the Java client's creative tabs failed");
                t.printStackTrace();
            }
        });
    }

    static boolean ready() {
        return built != null;
    }

    static byte[] take() {
        byte[] bytes = built;
        built = null;
        return bytes == null ? new byte[0] : bytes;
    }

    /** Both of `hasPermissions`, as `CreativeModeInventoryScreen`'s constructor builds them, the permitted set first so the contents stay at the unpermitted one. */
    private static byte[] build(net.minecraft.client.player.LocalPlayer player) throws IOException {
        var holders = player.level().registryAccess();
        var features = player.connection.enabledFeatures();
        var table = new Table(holders);
        CreativeModeTabs.tryRebuildTabContents(features, true, holders);
        byte[] with = set(table);
        CreativeModeTabs.tryRebuildTabContents(features, false, holders);
        byte[] without = set(table);
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(table.ids.size());
        table.bytes.writeTo(out);
        out.write(without);
        out.write(with);
        return bytes.toByteArray();
    }
}
