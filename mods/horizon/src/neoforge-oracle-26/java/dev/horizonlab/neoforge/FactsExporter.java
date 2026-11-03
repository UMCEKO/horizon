package dev.horizonlab.neoforge;

import com.google.gson.JsonElement;
import com.google.gson.stream.JsonWriter;
import com.mojang.serialization.DataResult;
import com.mojang.serialization.JsonOps;
import dev.horizonlab.facts.FactsWriter;
import java.io.IOException;
import java.io.Writer;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import java.util.TreeSet;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Registry;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.item.BlockItem;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.context.BlockPlaceContext;
import net.minecraft.world.item.crafting.Recipe;
import net.minecraft.world.item.crafting.RecipeHolder;
import net.minecraft.world.item.enchantment.Enchantments;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.SoundType;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.Property;
import net.minecraft.world.level.storage.loot.LootParams;
import net.minecraft.world.level.storage.loot.parameters.LootContextParams;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.fml.common.Mod;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.server.ServerStartedEvent;

/**
 * Dev-only oracle for Horizon's mod ports (docs/modpacks.md): once the server has started,
 * writes every block, item, data component type, recipe and tag of the namespaces named in {@code
 * horizon.oracle.namespaces} (plus the vanilla blocks, so templates can be compared) to {@code
 * horizon.oracle.out}, then stops the server. Never shipped to players.
 */
@Mod("horizon_oracle")
public final class FactsExporter {
    private static final BlockPos FLOOR = new BlockPos(0, 99, 0);

    public FactsExporter(IEventBus modBus) {
        NeoForge.EVENT_BUS.addListener(this::started);
    }

    private void started(ServerStartedEvent event) {
        MinecraftServer server = event.getServer();
        Path out = Path.of(System.getProperty("horizon.oracle.out", "horizon-oracle.json"));
        Set<String> namespaces =
                new TreeSet<>(Arrays.asList(System.getProperty("horizon.oracle.namespaces", "").split(",")));
        namespaces.remove("");
        try (Writer w = Files.newBufferedWriter(out, StandardCharsets.UTF_8);
                JsonWriter json = new JsonWriter(w)) {
            new Export(server, namespaces, json).run();
        } catch (Throwable e) {
            e.printStackTrace();
            System.err.flush();
            // Exit's shutdown hooks wait for this server thread, so it would hang here.
            Runtime.getRuntime().halt(3);
        }
        System.out.println("horizon-oracle: wrote " + out);
        server.halt(false);
    }

    /** The oracle's scope is the namespaces it was given; it runs on a real server, so it also samples drops and placement in its level. */
    private static final class Export extends FactsWriter {
        final MinecraftServer server;
        final ServerLevel level;
        final Set<String> namespaces;
        final List<ItemStack> tools = new ArrayList<>();
        final List<String> toolNames = new ArrayList<>();

        Export(MinecraftServer server, Set<String> namespaces, JsonWriter json) {
            super(json, server.registryAccess().createSerializationContext(JsonOps.INSTANCE), new Scope() {
                public boolean mod(Registry<?> registry, Identifier id) {
                    return namespaces.contains(id.getNamespace());
                }

                public boolean vanilla(Registry<?> registry, Identifier id) {
                    return id.getNamespace().equals("minecraft");
                }

                public boolean wholeTag(Identifier tag) {
                    return tag.getNamespace().equals("c") || namespaces.contains(tag.getNamespace());
                }
            });
            this.server = server;
            this.level = server.overworld();
            this.namespaces = namespaces;
            tool("hand", ItemStack.EMPTY);
            tool("diamond_pickaxe", new ItemStack(Items.DIAMOND_PICKAXE));
            tool("diamond_axe", new ItemStack(Items.DIAMOND_AXE));
            tool("diamond_shovel", new ItemStack(Items.DIAMOND_SHOVEL));
            tool("diamond_hoe", new ItemStack(Items.DIAMOND_HOE));
            tool("shears", new ItemStack(Items.SHEARS));
            ItemStack silk = new ItemStack(Items.DIAMOND_PICKAXE);
            silk.enchant(server.registryAccess().getOrThrow(Enchantments.SILK_TOUCH), 1);
            tool("silk_touch_pickaxe", silk);
        }

        void tool(String name, ItemStack stack) {
            toolNames.add(name);
            tools.add(stack);
        }

        void run() throws IOException {
            json.beginObject();
            json.name("schema").value(3);
            json.name("minecraft").value(net.minecraft.SharedConstants.getCurrentVersion().name());
            json.name("namespaces");
            json.beginArray();
            for (String n : namespaces) json.value(n);
            json.endArray();
            json.name("tools");
            json.beginArray();
            for (String n : toolNames) json.value(n);
            json.endArray();
            owned();
            level.setBlock(FLOOR, Blocks.STONE.defaultBlockState(), 3);
            blocks();
            vanillaItems();
            items();
            dataComponentTypes();
            recipes();
            tags();
            json.endObject();
        }

        @Override
        protected SoundType sound(BlockState state) {
            return state.getSoundType(level, FLOOR.above(), null);
        }

        @Override
        protected void stateExtras(Block block, BlockState state) throws IOException {
            if (lootSample(block, state)) drops(state);
        }

        @Override
        protected void blockExtras(Block block) throws IOException {
            if (block.asItem() instanceof BlockItem bi) placement(bi);
        }

        /** Every state of a small block; else the default and each one-property change from it. */
        boolean lootSample(Block block, BlockState state) {
            if (block.getStateDefinition().getPossibleStates().size() <= 32) return true;
            BlockState def = block.defaultBlockState();
            int differ = 0;
            for (Property<?> p : block.getStateDefinition().getProperties()) {
                if (!state.getValue(p).equals(def.getValue(p))) differ++;
            }
            return differ <= 1;
        }

        void drops(BlockState state) throws IOException {
            json.name("drops");
            json.beginObject();
            for (int i = 0; i < tools.size(); i++) {
                LootParams.Builder params =
                        new LootParams.Builder(level)
                                .withParameter(LootContextParams.ORIGIN, Vec3.atCenterOf(FLOOR.above()))
                                .withParameter(LootContextParams.TOOL, tools.get(i).copy());
                Map<String, Integer> counts = new TreeMap<>();
                for (ItemStack s : state.getDrops(params)) {
                    counts.merge(BuiltInRegistries.ITEM.getKey(s.getItem()).toString(), s.getCount(), Integer::sum);
                }
                json.name(toolNames.get(i));
                json.beginObject();
                for (Map.Entry<String, Integer> e : counts.entrySet()) json.name(e.getKey()).value(e.getValue());
                json.endObject();
            }
            json.endObject();
        }

        void placement(BlockItem item) throws IOException {
            BlockPos at = FLOOR.above();
            level.setBlock(at, Blocks.AIR.defaultBlockState(), 3);
            json.name("placement");
            json.beginObject();
            for (Direction face : new Direction[] {Direction.UP, Direction.NORTH}) {
                BlockPos clicked = face == Direction.UP ? FLOOR : at.relative(face.getOpposite());
                level.setBlock(clicked, Blocks.STONE.defaultBlockState(), 3);
                Vec3 hit = Vec3.atCenterOf(clicked).add(face.getUnitVec3().scale(0.5));
                // A player-less context (the constructor is protected, hence the subclass).
                BlockPlaceContext ctx =
                        new BlockPlaceContext(
                                level, null, InteractionHand.MAIN_HAND, new ItemStack(item),
                                new BlockHitResult(hit, face, clicked, false)) {};
                BlockState placed = null;
                try {
                    placed = item.getBlock().getStateForPlacement(ctx);
                } catch (RuntimeException e) {
                    // Needs a player: recorded as no placement.
                }
                json.name(face.getName()).value(placed == null ? null : stateKey(placed));
                if (face != Direction.UP) level.setBlock(clicked, Blocks.AIR.defaultBlockState(), 3);
            }
            json.endObject();
        }

        void raw(String name, DataResult<JsonElement> encoded) throws IOException {
            var value = encoded.result();
            if (value.isPresent()) {
                json.name(name);
                json.jsonValue(value.get().toString());
            }
        }

        void recipes() throws IOException {
            json.name("recipes");
            json.beginObject();
            for (RecipeHolder<?> holder : server.getRecipeManager().getRecipes()) {
                if (!namespaces.contains(holder.id().identifier().getNamespace())) continue;
                var recipe = holder.value();
                json.name(holder.id().identifier().toString());
                json.beginObject();
                json.name("type").value(BuiltInRegistries.RECIPE_TYPE.getKey(recipe.getType()).toString());
                json.name("serializer").value(BuiltInRegistries.RECIPE_SERIALIZER.getKey(recipe.getSerializer()).toString());
                try {
                    raw("json", Recipe.DIRECT_CODEC.encodeStart(server.registryAccess().createSerializationContext(JsonOps.INSTANCE), (Recipe<?>) recipe));
                } catch (RuntimeException e) {
                    // A mod's codec that can't encode one of its recipes costs that recipe only.
                    json.name("unencodable").value(e.toString());
                }
                json.endObject();
            }
            json.endObject();
        }
    }
}
