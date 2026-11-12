package dev.horizonlab.javamods.boot;

import dev.horizonlab.javamods.boot.mixin.CompositeConditionAccess;
import dev.horizonlab.javamods.boot.mixin.LootModifierAccess;
import dev.horizonlab.javamods.boot.mixin.LootTableIdAccess;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.lang.reflect.Method;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Set;
import java.util.TreeSet;
import java.util.function.Predicate;
import net.minecraft.core.Holder;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.HolderSet;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.server.MinecraftServer;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemInstance;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.crafting.Ingredient;
import net.minecraft.world.item.crafting.Recipe;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.minecraft.world.level.storage.loot.predicates.AllOfCondition;
import net.minecraft.world.level.storage.loot.predicates.CompositeLootItemCondition;
import net.minecraft.world.level.storage.loot.predicates.LootItemCondition;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.common.loot.IGlobalLootModifier;
import net.neoforged.neoforge.common.loot.LootModifier;
import net.neoforged.neoforge.common.loot.LootModifierManager;
import net.neoforged.neoforge.common.loot.LootTableIdCondition;
import net.neoforged.neoforge.resource.NeoForgeReloadListeners;

/** What the mods change in crafting, loot and stations, answered once at the first server start (`craft_facts`); see docs/java-mods-map.md. */
public final class CraftFacts {
    private CraftFacts() {}

    /** Bit i of the recipe type bits is STATIONS[i]. */
    static final String[] STATIONS = {"crafting", "smithing", "smelting", "blasting", "smoking", "campfire_cooking", "stonecutting"};

    /** Bit i of the event bits is EVENTS[i]. */
    static final List<Class<?>> EVENTS = List.of(
            net.neoforged.neoforge.event.entity.player.PlayerEvent.ItemCraftedEvent.class,
            net.neoforged.neoforge.event.entity.player.PlayerEvent.ItemSmeltedEvent.class,
            net.neoforged.neoforge.event.entity.player.PlayerEvent.ItemSmithedEvent.class,
            net.neoforged.neoforge.event.AnvilUpdateEvent.class,
            net.neoforged.neoforge.event.entity.player.AnvilCraftEvent.Pre.class,
            net.neoforged.neoforge.event.entity.player.AnvilCraftEvent.Post.class,
            net.neoforged.neoforge.event.GrindstoneEvent.OnPlaceItem.class,
            net.neoforged.neoforge.event.GrindstoneEvent.OnTakeItem.class,
            net.neoforged.neoforge.event.enchanting.EnchantmentLevelSetEvent.class,
            net.neoforged.neoforge.event.entity.player.PlayerEnchantItemEvent.class,
            net.neoforged.neoforge.event.enchanting.GetEnchantmentLevelEvent.class);

    /** Bit i of an item's bits is the method METHODS[i] (name, then parameter types). */
    private static final Object[][] METHODS = {
        {"onCraftedBy", ItemStack.class, Player.class},
        {"getCraftingRemainder", ItemInstance.class},
        {"getEnchantmentLevel", ItemInstance.class, Holder.class},
        {"getAllEnchantments", ItemStack.class, HolderLookup.RegistryLookup.class},
        {"supportsEnchantment", ItemStack.class, Holder.class},
        {"isPrimaryItemFor", ItemStack.class, Holder.class},
        {"isCombineRepairable", ItemStack.class},
        {"getXpRepairRatio", ItemStack.class},
        {"canGrindstoneRepair", ItemStack.class},
        {"getMaxDamage", ItemStack.class},
        {"isDamageable", ItemStack.class},
        {"applyEnchantments", ItemStack.class, List.class},
    };

    /** Condition types a loot modifier's own tests may use without being a risk to table-level asking. */
    private static final Set<String> PLAIN_CONDITIONS = Set.of("neoforge:loot_table_id", "minecraft:random_chance", "minecraft:all_of", "minecraft:any_of", "minecraft:inverted");

    /** Whether the recipe is mod code: its serializer is a mod's, or an ingredient is a custom one. */
    static boolean recipeHasCode(Recipe<?> recipe, Predicate<Identifier> ownsSerializer) {
        Identifier serializer = BuiltInRegistries.RECIPE_SERIALIZER.getKey(recipe.getSerializer());
        if (serializer != null && ownsSerializer.test(serializer)) return true;
        return recipe.placementInfo().ingredients().stream().anyMatch(Ingredient::isCustom);
    }

    /** The bits of the methods the item's class declares below `Item`, vanilla's own classes aside. */
    static int itemBits(Class<?> type) {
        int bits = 0;
        for (Class<?> c = type; c != null && c != Item.class; c = c.getSuperclass()) {
            if (c.getName().startsWith("net.minecraft.")) continue;
            for (Method m : c.getDeclaredMethods()) {
                if (m.isBridge() || m.isSynthetic()) continue;
                for (int i = 0; i < METHODS.length; i++) {
                    if (m.getName().equals(METHODS[i][0]) && Arrays.equals(m.getParameterTypes(), Arrays.copyOfRange(METHODS[i], 1, METHODS[i].length, Class[].class))) bits |= 1 << i;
                }
            }
        }
        return bits;
    }

    /** The table ids a loot modifier's condition limits it to, or null when it applies to any. */
    static Set<Identifier> tables(LootItemCondition condition, Set<String> otherTypes) {
        Set<Identifier> found = new TreeSet<>();
        collect(condition, true, found, otherTypes);
        return found.isEmpty() ? null : found;
    }

    private static void collect(LootItemCondition condition, boolean narrows, Set<Identifier> tables, Set<String> otherTypes) {
        if (condition instanceof LootTableIdCondition) {
            if (narrows) tables.add(((LootTableIdAccess) condition).horizon$target());
            return;
        }
        Identifier type = BuiltInRegistries.LOOT_CONDITION_TYPE.getKey(condition.codec());
        if (type != null && !PLAIN_CONDITIONS.contains(type.toString())) otherTypes.add(type.toString());
        if (condition instanceof CompositeLootItemCondition) {
            boolean all = narrows && condition instanceof AllOfCondition;
            HolderSet<LootItemCondition> terms = ((CompositeConditionAccess) condition).horizon$terms();
            for (Holder<LootItemCondition> term : terms) collect(term.value(), all, tables, otherTypes);
        }
    }

    private static void text(DataOutputStream out, String value) throws IOException {
        byte[] bytes = value.getBytes(StandardCharsets.UTF_8);
        out.writeInt(bytes.length);
        out.write(bytes);
    }

    private static void owned(DataOutputStream out, Registry<?> registry) throws IOException {
        List<String> ids = registry.keySet().stream().filter(id -> BootFacts.owns(registry, id)).map(Identifier::toString).sorted().toList();
        out.writeInt(ids.size());
        for (String id : ids) text(out, id);
    }

    public static byte[] bin(MinecraftServer server) throws IOException {
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(recipeTypeBits(server));
        IEventBus bus = NeoForge.EVENT_BUS;
        int events = 0;
        for (int i = 0; i < EVENTS.size(); i++) if (Listeners.listened(bus, EVENTS.get(i))) events |= 1 << i;
        out.writeInt(events);
        var items = new ArrayList<String>();
        var bits = new ArrayList<Integer>();
        for (Identifier id : new TreeSet<>(BuiltInRegistries.ITEM.keySet())) {
            Item item = BuiltInRegistries.ITEM.getValue(id);
            if (!BootFacts.ownsItem(item)) continue;
            int b = itemBits(item.getClass());
            if (b != 0) {
                items.add(id.toString());
                bits.add(b);
            }
        }
        out.writeInt(items.size());
        for (int i = 0; i < items.size(); i++) {
            text(out, items.get(i));
            out.writeInt(bits.get(i));
        }
        modifiers(server, out);
        owned(out, BuiltInRegistries.LOOT_FUNCTION_TYPE);
        owned(out, BuiltInRegistries.LOOT_CONDITION_TYPE);
        return bytes.toByteArray();
    }

    private static int recipeTypeBits(MinecraftServer server) {
        var counts = new int[STATIONS.length];
        for (RecipeHolder<?> holder : server.getRecipeManager().getRecipes()) {
            if (!recipeHasCode(holder.value(), id -> BootFacts.owns(BuiltInRegistries.RECIPE_SERIALIZER, id))) continue;
            Identifier type = BuiltInRegistries.RECIPE_TYPE.getKey(holder.value().getType());
            int bit = type == null || !type.getNamespace().equals("minecraft") ? -1 : Arrays.asList(STATIONS).indexOf(type.getPath());
            if (bit >= 0) counts[bit]++;
        }
        int bits = 0;
        for (int i = 0; i < counts.length; i++) {
            if (counts[i] == 0) continue;
            bits |= 1 << i;
            System.err.println("Horizon: " + counts[i] + " recipes of mods' code in minecraft:" + STATIONS[i]);
        }
        return bits;
    }

    private static void modifiers(MinecraftServer server, DataOutputStream out) throws IOException {
        var manager = (LootModifierManager) server.getServerResources().managers().getListener(NeoForgeReloadListeners.LOOT_MODIFIERS_KEY);
        var all = new ArrayList<IGlobalLootModifier>();
        manager.getSortedModifiers().forEach(all::add);
        out.writeInt(all.size());
        for (IGlobalLootModifier modifier : all) {
            text(out, manager.getId(modifier).toString());
            Set<Identifier> tables = null;
            if (modifier instanceof LootModifier loot) {
                var condition = ((LootModifierAccess) loot).horizon$condition();
                var other = new TreeSet<String>();
                if (condition.isPresent()) tables = tables(condition.get().value(), other);
                if (!other.isEmpty()) System.err.println("Horizon: loot modifier " + manager.getId(modifier) + " tests " + String.join(", ", other));
            }
            out.writeBoolean(tables == null);
            out.writeInt(tables == null ? 0 : tables.size());
            if (tables != null) for (Identifier table : tables) text(out, table.toString());
        }
    }
}
