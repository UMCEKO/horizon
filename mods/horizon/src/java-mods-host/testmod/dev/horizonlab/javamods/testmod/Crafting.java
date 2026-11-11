package dev.horizonlab.javamods.testmod;

import com.mojang.serialization.MapCodec;
import com.mojang.serialization.codecs.RecordCodecBuilder;
import it.unimi.dsi.fastutil.objects.ObjectArrayList;
import java.util.Map;
import java.util.Optional;
import java.util.concurrent.ConcurrentHashMap;
import net.minecraft.core.Holder;
import net.minecraft.core.HolderLookup;
import net.minecraft.core.NonNullList;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.chat.Component;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemInstance;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.ItemStackTemplate;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.component.CustomData;
import net.minecraft.world.item.crafting.CraftingInput;
import net.minecraft.world.item.crafting.CraftingRecipe;
import net.minecraft.world.item.crafting.Recipe;
import net.minecraft.world.item.crafting.RecipeSerializer;
import net.minecraft.world.item.crafting.ShapedRecipe;
import net.minecraft.world.item.crafting.ShapedRecipePattern;
import net.minecraft.world.item.enchantment.Enchantment;
import net.minecraft.world.item.enchantment.Enchantments;
import net.minecraft.world.item.enchantment.ItemEnchantments;
import net.minecraft.world.level.storage.loot.LootContext;
import net.minecraft.world.level.storage.loot.functions.LootItemConditionalFunction;
import net.minecraft.world.level.storage.loot.predicates.LootItemCondition;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.common.loot.IGlobalLootModifier;
import net.neoforged.neoforge.common.loot.LootModifier;
import net.neoforged.neoforge.event.entity.player.PlayerEvent;
import net.neoforged.neoforge.registries.DeferredRegister;
import net.neoforged.neoforge.registries.NeoForgeRegistries;

/** What a mod changes in crafting and loot: a recipe with its own code, an item with its own methods, a crafting event, a loot modifier, a loot function and a loot condition. */
final class Crafting {
    private Crafting() {}

    private static final String MOD = "horizon_java_test";

    /** Results crafted on the server, by item id. */
    static final Map<String, Integer> CRAFTED = new ConcurrentHashMap<>();
    /** The non-empty slots of the craft matrix at the last craft of each result item. */
    static final Map<String, Integer> MATRIX = new ConcurrentHashMap<>();

    private static final DeferredRegister<RecipeSerializer<?>> SERIALIZERS = DeferredRegister.create(Registries.RECIPE_SERIALIZER, MOD);
    private static final DeferredRegister<MapCodec<? extends IGlobalLootModifier>> MODIFIERS = DeferredRegister.create(NeoForgeRegistries.GLOBAL_LOOT_MODIFIER_SERIALIZERS, MOD);
    private static final DeferredRegister<MapCodec<? extends net.minecraft.world.level.storage.loot.functions.LootItemFunction>> FUNCTIONS = DeferredRegister.create(Registries.LOOT_FUNCTION_TYPE, MOD);
    private static final DeferredRegister<MapCodec<? extends LootItemCondition>> CONDITIONS = DeferredRegister.create(Registries.LOOT_CONDITION_TYPE, MOD);

    static void register(IEventBus bus, DeferredRegister.Items items) {
        items.registerItem("charm", Charm::new);
        SERIALIZERS.register("named_copy", () -> NamedCopy.SERIALIZER);
        MODIFIERS.register("add_item", () -> AddItem.CODEC);
        FUNCTIONS.register("stamp", () -> Stamp.CODEC);
        CONDITIONS.register("always", () -> Always.CODEC);
        SERIALIZERS.register(bus);
        MODIFIERS.register(bus);
        FUNCTIONS.register(bus);
        CONDITIONS.register(bus);
        NeoForge.EVENT_BUS.addListener((PlayerEvent.ItemCraftedEvent event) -> {
            if (event.getEntity().level().isClientSide()) return;
            String result = BuiltInRegistries.ITEM.getKey(event.getCrafting().getItem()).toString();
            CRAFTED.merge(result, 1, Integer::sum);
            int filled = 0;
            for (int i = 0; i < event.getInventory().getContainerSize(); i++) if (!event.getInventory().getItem(i).isEmpty()) filled++;
            MATRIX.put(result, filled);
        });
    }

    /** Copies the first ingredient's name onto the result and leaves a stick where paper was. */
    static final class NamedCopy extends ShapedRecipe {
        private final ItemStackTemplate template;

        NamedCopy(Recipe.CommonInfo common, CraftingRecipe.CraftingBookInfo book, ShapedRecipePattern pattern, ItemStackTemplate result) {
            super(common, book, pattern, result);
            this.template = result;
        }

        static final MapCodec<NamedCopy> CODEC = RecordCodecBuilder.mapCodec(i -> i.group(
                Recipe.CommonInfo.MAP_CODEC.forGetter(r -> r.commonInfo),
                CraftingRecipe.CraftingBookInfo.MAP_CODEC.forGetter(r -> r.bookInfo),
                ShapedRecipePattern.MAP_CODEC.forGetter(r -> r.pattern),
                ItemStackTemplate.CODEC.fieldOf("result").forGetter(r -> r.template)).apply(i, NamedCopy::new));
        static final RecipeSerializer<NamedCopy> SERIALIZER = new RecipeSerializer<>(CODEC, StreamCodec.composite(
                Recipe.CommonInfo.STREAM_CODEC, r -> r.commonInfo,
                CraftingRecipe.CraftingBookInfo.STREAM_CODEC, r -> r.bookInfo,
                ShapedRecipePattern.STREAM_CODEC, r -> r.pattern,
                ItemStackTemplate.STREAM_CODEC, r -> r.template,
                NamedCopy::new));

        @Override
        @SuppressWarnings("unchecked")
        public RecipeSerializer<ShapedRecipe> getSerializer() {
            return (RecipeSerializer<ShapedRecipe>) (RecipeSerializer<?>) SERIALIZER;
        }

        @Override
        public ItemStack assemble(CraftingInput input) {
            ItemStack result = super.assemble(input);
            for (int i = 0; i < input.size(); i++) {
                ItemStack stack = input.getItem(i);
                if (stack.isEmpty()) continue;
                Component name = stack.get(DataComponents.CUSTOM_NAME);
                if (name != null) result.set(DataComponents.CUSTOM_NAME, name);
                break;
            }
            return result;
        }

        @Override
        public NonNullList<ItemStack> getRemainingItems(CraftingInput input) {
            NonNullList<ItemStack> remaining = super.getRemainingItems(input);
            for (int i = 0; i < input.size(); i++) if (input.getItem(i).is(Items.PAPER)) remaining.set(i, new ItemStack(Items.STICK));
            return remaining;
        }
    }

    /** Records who crafted it and answers enchantment levels its code computes. */
    static final class Charm extends Item {
        Charm(Properties properties) {
            super(properties);
        }

        @Override
        public void onCraftedBy(ItemStack stack, Player player) {
            CustomData.update(DataComponents.CUSTOM_DATA, stack, tag -> tag.putString("crafted_by", player.getGameProfile().name()));
        }

        @Override
        public int getEnchantmentLevel(ItemInstance stack, Holder<Enchantment> enchantment) {
            if (enchantment.is(Enchantments.FORTUNE)) return 3;
            if (enchantment.is(Enchantments.SILK_TOUCH)) return 1;
            return super.getEnchantmentLevel(stack, enchantment);
        }

        @Override
        public ItemEnchantments getAllEnchantments(ItemStack stack, HolderLookup.RegistryLookup<Enchantment> lookup) {
            var all = new ItemEnchantments.Mutable(super.getAllEnchantments(stack, lookup));
            all.set(lookup.getOrThrow(Enchantments.FORTUNE), 3);
            all.set(lookup.getOrThrow(Enchantments.SILK_TOUCH), 1);
            return all.toImmutable();
        }
    }

    /** Adds one of an item to what a loot table drops. */
    static final class AddItem extends LootModifier {
        static final MapCodec<AddItem> CODEC = RecordCodecBuilder.mapCodec(i -> codecStart(i).and(BuiltInRegistries.ITEM.byNameCodec().fieldOf("item").forGetter(m -> m.item)).apply(i, AddItem::new));

        private final Item item;

        AddItem(Optional<Holder<LootItemCondition>> condition, int priority, Item item) {
            super(condition, priority);
            this.item = item;
        }

        @Override
        protected ObjectArrayList<ItemStack> doApply(ObjectArrayList<ItemStack> loot, LootContext context) {
            loot.add(new ItemStack(item));
            return loot;
        }

        @Override
        public MapCodec<? extends IGlobalLootModifier> codec() {
            return CODEC;
        }
    }

    /** Sets `custom_data {stamped: 1}` on what a loot entry makes. */
    static final class Stamp extends LootItemConditionalFunction {
        static final MapCodec<Stamp> CODEC = RecordCodecBuilder.mapCodec(i -> commonFields(i).apply(i, Stamp::new));

        Stamp(Optional<Holder<LootItemCondition>> condition) {
            super(condition);
        }

        @Override
        public MapCodec<Stamp> codec() {
            return CODEC;
        }

        @Override
        protected ItemStack run(ItemStack stack, LootContext context) {
            CustomData.update(DataComponents.CUSTOM_DATA, stack, tag -> tag.putInt("stamped", 1));
            return stack;
        }
    }

    /** A condition that always holds. */
    static final class Always implements LootItemCondition {
        static final MapCodec<Always> CODEC = MapCodec.unit(Always::new);

        @Override
        public MapCodec<Always> codec() {
            return CODEC;
        }

        @Override
        public boolean test(LootContext context) {
            return true;
        }
    }
}
