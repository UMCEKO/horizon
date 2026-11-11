package dev.horizonlab.facts;

import com.google.gson.JsonElement;
import com.google.gson.stream.JsonWriter;
import com.mojang.serialization.DynamicOps;
import java.io.IOException;
import java.lang.reflect.Method;
import java.lang.reflect.Modifier;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeMap;
import java.util.TreeSet;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Registry;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.component.TypedDataComponent;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.world.item.BlockItem;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.LevelAccessor;
import net.minecraft.world.level.LevelReader;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.LiquidBlockContainer;
import net.minecraft.world.level.block.SoundType;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.Property;
import net.minecraft.world.level.material.FlowingFluid;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.level.material.FluidState;
import net.minecraft.world.level.material.Fluids;
import net.neoforged.neoforge.common.SoundAction;
import net.neoforged.neoforge.fluids.FluidType;
import net.neoforged.neoforge.registries.NeoForgeRegistries;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.VoxelShape;

/** Facts as horizon-port-pack's oracle.rs reads them, shared by the oracle and the game's Java boot; the oracle overrides the level-dependent hooks. */
public class FactsWriter {
    /** Which entries the facts are about: the mods' (written in full) and vanilla's (blocks only, for templates). */
    public interface Scope {
        boolean mod(Registry<?> registry, Identifier id);

        boolean vanilla(Registry<?> registry, Identifier id);

        /** A tag written whole, whatever its members. */
        boolean wholeTag(Identifier tag);
    }

    protected final JsonWriter json;
    protected final DynamicOps<JsonElement> ops;
    protected final Scope scope;

    public FactsWriter(JsonWriter json, DynamicOps<JsonElement> ops, Scope scope) {
        this.json = json;
        this.ops = ops;
        this.scope = scope;
    }

    /** The mods' entries of the registries Horizon's tables hold. */
    public void owned() throws IOException {
        json.name("owned");
        json.beginObject();
        ownedOf("block", BuiltInRegistries.BLOCK);
        ownedOf("item", BuiltInRegistries.ITEM);
        ownedOf("block_entity_type", BuiltInRegistries.BLOCK_ENTITY_TYPE);
        ownedOf("data_component_type", BuiltInRegistries.DATA_COMPONENT_TYPE);
        ownedOf("fluid", BuiltInRegistries.FLUID);
        json.endObject();
    }

    private void ownedOf(String name, Registry<?> registry) throws IOException {
        json.name(name);
        json.beginArray();
        for (Identifier id : registry.keySet().stream().sorted().toList()) {
            if (scope.mod(registry, id)) json.value(id.toString());
        }
        json.endArray();
    }

    public void blocks() throws IOException {
        json.name("blocks");
        json.beginObject();
        for (Block block : BuiltInRegistries.BLOCK) {
            Identifier id = BuiltInRegistries.BLOCK.getKey(block);
            boolean mod = scope.mod(BuiltInRegistries.BLOCK, id);
            if (!mod && !scope.vanilla(BuiltInRegistries.BLOCK, id)) continue;
            json.name(id.toString());
            json.beginObject();
            json.name("class");
            json.beginArray();
            for (Class<?> c = block.getClass(); c != null && c != Object.class; c = c.getSuperclass()) {
                json.value(c.getName());
            }
            json.endArray();
            BlockState def = block.defaultBlockState();
            json.name("properties");
            json.beginObject();
            for (Property<?> p : block.getStateDefinition().getProperties()) {
                json.name(p.getName());
                json.beginArray();
                for (Object v : p.getPossibleValues()) json.value(name(p, v));
                json.endArray();
            }
            json.endObject();
            json.name("default").value(stateKey(def));
            json.name("item").value(BuiltInRegistries.ITEM.getKey(block.asItem()).toString());
            json.name("explosion_resistance").value(block.getExplosionResistance());
            json.name("friction").value(block.getFriction());
            json.name("speed_factor").value(block.getSpeedFactor());
            json.name("jump_factor").value(block.getJumpFactor());
            blockSetType(block);
            json.name("states");
            json.beginObject();
            for (BlockState state : block.getStateDefinition().getPossibleStates()) {
                json.name(stateKey(state));
                json.beginObject();
                json.name("hardness").value(state.getDestroySpeed(EmptyBlockGetter.INSTANCE, BlockPos.ZERO));
                json.name("emission").value(state.getLightEmission());
                json.name("requires_tool").value(state.requiresCorrectToolForDrops());
                json.name("can_occlude").value(state.canOcclude());
                json.name("replaceable").value(state.canBeReplaced());
                json.name("random_ticks").value(state.isRandomlyTicking());
                json.name("light_dampening").value(state.getLightDampening());
                json.name("skylight_down").value(state.propagatesSkylightDown());
                json.name("solid_render").value(state.isSolidRender());
                json.name("shape_occlusion").value(state.useShapeForLightOcclusion());
                json.name("occlusion");
                boxes(state.getOcclusionShape());
                var color = state.getMapColor(EmptyBlockGetter.INSTANCE, BlockPos.ZERO);
                json.name("map_color").value(color.col);
                json.name("map_color_id").value(color.id);
                sound(sound(state));
                json.name("outline");
                boxes(state.getShape(EmptyBlockGetter.INSTANCE, BlockPos.ZERO));
                json.name("collision");
                boxes(state.getCollisionShape(EmptyBlockGetter.INSTANCE, BlockPos.ZERO));
                stateFluid(state);
                if (mod && block instanceof LiquidBlockContainer container) canPlaceLiquid(container, state);
                if (mod) stateExtras(block, state);
                json.endObject();
            }
            json.endObject();
            if (mod) blockExtras(block);
            json.endObject();
        }
        json.endObject();
    }

    private static final BlockPos PROBE_POS = new BlockPos(0, 64, 0);

    private static final Class<?>[] NO_PARAMS = {};

    /** The fluid hooks whose declaring package (vanilla, NeoForge or a mod) is written, with their parameter types. */
    private static final Map<String, Class<?>[]> CODE = new LinkedHashMap<>();

    static {
        CODE.put("tick", new Class<?>[] {net.minecraft.server.level.ServerLevel.class, BlockPos.class, BlockState.class, FluidState.class});
        CODE.put("randomTick", new Class<?>[] {net.minecraft.server.level.ServerLevel.class, BlockPos.class, FluidState.class, net.minecraft.util.RandomSource.class});
        CODE.put("animateTick", new Class<?>[] {net.minecraft.world.level.Level.class, BlockPos.class, FluidState.class, net.minecraft.util.RandomSource.class});
        CODE.put("canBeReplacedWith", new Class<?>[] {FluidState.class, BlockGetter.class, BlockPos.class, Fluid.class, Direction.class});
        CODE.put("getTickDelay", new Class<?>[] {LevelReader.class});
        CODE.put("getDropOff", new Class<?>[] {LevelReader.class});
        CODE.put("getSlopeFindDistance", new Class<?>[] {LevelReader.class});
        CODE.put("canConvertToSource", new Class<?>[] {net.minecraft.server.level.ServerLevel.class});
        CODE.put("beforeDestroyingBlock", new Class<?>[] {LevelAccessor.class, BlockPos.class, BlockState.class});
        CODE.put("getExplosionResistance", NO_PARAMS);
        CODE.put("getPickupSound", NO_PARAMS);
    }

    private static String fluidKey(Fluid fluid) {
        return BuiltInRegistries.FLUID.getKey(fluid).toString();
    }

    private static String legacyBlock(FluidState fs) {
        BlockState legacy = fs.createLegacyBlock();
        String key = stateKey(legacy);
        String id = BuiltInRegistries.BLOCK.getKey(legacy.getBlock()).toString();
        return key.isEmpty() ? id : id + "[" + key + "]";
    }

    private static boolean falling(FluidState fs) {
        return fs.hasProperty(FlowingFluid.FALLING) && fs.getValue(FlowingFluid.FALLING);
    }

    private void stateFluid(BlockState state) throws IOException {
        FluidState fs = state.getFluidState();
        if (fs.isEmpty()) return;
        json.name("fluid");
        json.beginObject();
        json.name("id").value(fluidKey(fs.getType()));
        json.name("amount").value(fs.getAmount());
        json.name("falling").value(falling(fs));
        json.name("source").value(fs.isSource());
        json.endObject();
    }

    /** A getter answering `state` at the probe position, `above` over it when given, and air elsewhere; no block entities. */
    private static BlockGetter probe(BlockState state, BlockState above) {
        return new BlockGetter() {
            @Override
            public BlockEntity getBlockEntity(BlockPos pos) {
                return null;
            }

            @Override
            public BlockState getBlockState(BlockPos pos) {
                if (pos.equals(PROBE_POS)) return state;
                if (above != null && pos.equals(PROBE_POS.above())) return above;
                return Blocks.AIR.defaultBlockState();
            }

            @Override
            public FluidState getFluidState(BlockPos pos) {
                return getBlockState(pos).getFluidState();
            }

            @Override
            public int getMinY() {
                return -64;
            }

            @Override
            public int getHeight() {
                return 384;
            }
        };
    }

    /** The fluids a fluid-loggable mod block takes: native can't ask Java inside its cascade. */
    private void canPlaceLiquid(LiquidBlockContainer container, BlockState state) throws IOException {
        BlockGetter probe = probe(state, null);
        json.name("can_place_liquid");
        json.beginArray();
        for (Fluid fluid : BuiltInRegistries.FLUID) {
            if (fluid == Fluids.EMPTY) continue;
            if (container.canPlaceLiquid(null, probe, PROBE_POS, state, fluid)) json.value(fluidKey(fluid));
        }
        json.endArray();
    }

    /** Every fluid in registry order, empty first: what `replaceable`'s `incoming` indexes. */
    private static List<Fluid> fluidOrder() {
        List<Fluid> order = new ArrayList<>();
        order.add(Fluids.EMPTY);
        for (Fluid fluid : BuiltInRegistries.FLUID) if (fluid != Fluids.EMPTY) order.add(fluid);
        return order;
    }

    private static String fluidStateKey(FluidState fs) {
        List<String> parts = new ArrayList<>();
        for (Property<?> p : fs.getProperties()) {
            if (p.getName().equals("level")) parts.add("amount=" + fs.getAmount());
            else if (p.getName().equals("falling")) parts.add("falling=" + falling(fs));
        }
        return String.join(",", parts);
    }

    private static String codeOwner(Fluid fluid, String name, Class<?>[] params) {
        for (Class<?> c = fluid.getClass(); c != null; c = c.getSuperclass()) {
            try {
                c.getDeclaredMethod(name, params);
            } catch (NoSuchMethodException e) {
                continue;
            }
            String n = c.getName();
            return n.startsWith("net.minecraft.") ? "vanilla" : n.startsWith("net.neoforged.") ? "neoforge" : "mod";
        }
        return null;
    }

    /** Every fluid, its states and which code runs it, after `fluid_order`. */
    public void fluids() throws IOException {
        List<Fluid> order = fluidOrder();
        json.name("fluid_order");
        json.beginArray();
        for (Fluid fluid : order) json.value(fluidKey(fluid));
        json.endArray();
        json.name("fluids");
        json.beginObject();
        for (Fluid fluid : BuiltInRegistries.FLUID) {
            if (fluid == Fluids.EMPTY) continue;
            Identifier id = BuiltInRegistries.FLUID.getKey(fluid);
            boolean mod = scope.mod(BuiltInRegistries.FLUID, id);
            if (!mod && !scope.vanilla(BuiltInRegistries.FLUID, id)) continue;
            json.name(id.toString());
            json.beginObject();
            json.name("class");
            json.beginArray();
            for (Class<?> c = fluid.getClass(); c != null && c != Object.class; c = c.getSuperclass()) json.value(c.getName());
            json.endArray();
            json.name("source").value(fluid instanceof FlowingFluid f ? fluidKey(f.getSource()) : id.toString());
            json.name("flowing").value(fluid instanceof FlowingFluid f ? fluidKey(f.getFlowing()) : id.toString());
            json.name("bucket").value(BuiltInRegistries.ITEM.getKey(fluid.getBucket()).toString());
            List<FluidState> states = new ArrayList<>(fluid.getStateDefinition().getPossibleStates());
            json.name("states");
            json.beginObject();
            for (FluidState fs : states) {
                json.name(fluidStateKey(fs));
                json.beginObject();
                json.name("amount").value(fs.getAmount());
                json.name("falling").value(falling(fs));
                json.name("source").value(fs.isSource());
                json.name("random").value(fs.isRandomlyTicking());
                json.name("block").value(legacyBlock(fs));
                json.endObject();
            }
            json.endObject();
            json.name("code");
            json.beginObject();
            for (var e : CODE.entrySet()) {
                String owner = codeOwner(fluid, e.getKey(), e.getValue());
                if (owner != null) json.name(e.getKey()).value(owner);
            }
            json.endObject();
            if (mod) replaceable(fluid, states, order);
            fluidType(fluid);
            json.endObject();
        }
        json.endObject();
    }

    private void replaceable(Fluid fluid, List<FluidState> states, List<Fluid> order) throws IOException {
        StringBuilder bits = new StringBuilder();
        BlockState sourceAbove = fluid instanceof FlowingFluid f ? f.getSource().defaultFluidState().createLegacyBlock() : null;
        for (FluidState fs : states) {
            BlockState own = fs.createLegacyBlock();
            BlockGetter alone = probe(own, null);
            BlockGetter covered = probe(own, sourceAbove == null ? own : sourceAbove);
            for (Fluid incoming : order) {
                for (Direction direction : Direction.values()) {
                    bits.append(fs.canBeReplacedWith(alone, PROBE_POS, incoming, direction) ? '1' : '0');
                    bits.append(fs.canBeReplacedWith(covered, PROBE_POS, incoming, direction) ? '1' : '0');
                }
            }
        }
        json.name("replaceable").value(bits.toString());
        json.name("replaceable_fluids").value(order.size());
    }

    /** NeoForge's FluidType of a fluid: its numbers, sounds and which hooks the type's own class overrides. */
    private void fluidType(Fluid fluid) throws IOException {
        FluidType type = fluid.getFluidType();
        json.name("fluid_type");
        json.beginObject();
        json.name("id").value(NeoForgeRegistries.FLUID_TYPES.getKey(type).toString());
        try {
            for (String field : new String[] {"motionScale", "canPushEntity", "canSwim", "canDrown", "fallDistanceModifier", "canExtinguish", "canConvertToSource", "supportsBoating", "canHydrate", "lightLevel", "density", "temperature", "viscosity", "isWaterLike"}) {
                var f = FluidType.class.getDeclaredField(field);
                f.setAccessible(true);
                json.name(field);
                json.jsonValue(String.valueOf(f.get(type)));
            }
            json.name("sounds");
            json.beginObject();
            var sounds = FluidType.class.getDeclaredField("sounds");
            sounds.setAccessible(true);
            for (var e : ((Map<?, ?>) sounds.get(type)).entrySet()) {
                json.name(((SoundAction) e.getKey()).name()).value(((SoundEvent) e.getValue()).location().toString());
            }
            json.endObject();
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("FluidType isn't readable", e);
        }
        Set<String> hooks = new TreeSet<>();
        for (Method m : FluidType.class.getDeclaredMethods()) {
            int mods = m.getModifiers();
            if (Modifier.isPublic(mods) && !Modifier.isStatic(mods) && !Modifier.isFinal(mods)) hooks.add(m.getName());
        }
        Set<String> overrides = new TreeSet<>();
        for (Class<?> c = type.getClass(); c != null && c != FluidType.class; c = c.getSuperclass()) {
            if (c.getName().startsWith("net.neoforged.")) continue;
            for (Method m : c.getDeclaredMethods()) if (hooks.contains(m.getName()) && !m.isSynthetic()) overrides.add(m.getName());
        }
        json.name("overrides");
        json.beginArray();
        for (String name : overrides) json.value(name);
        json.endArray();
        json.endObject();
    }

    /** The sound type a state plays; the oracle asks its level. */
    protected SoundType sound(BlockState state) {
        return state.getSoundType();
    }

    /** More facts about one state of a mod's block. */
    protected void stateExtras(Block block, BlockState state) throws IOException {}

    /** More facts about a mod's block. */
    protected void blockExtras(Block block) throws IOException {}

    /** The BlockSetType (doors, trapdoors, buttons, plates) and WoodType (fence gates) a block keeps. */
    void blockSetType(Block block) throws IOException {
        String setType = null, woodType = null;
        Boolean byHand = null;
        for (Class<?> c = block.getClass(); c != null && c != Object.class; c = c.getSuperclass()) {
            for (var f : c.getDeclaredFields()) {
                try {
                    if (f.getType() == net.minecraft.world.level.block.state.properties.BlockSetType.class && setType == null) {
                        f.setAccessible(true);
                        var t = (net.minecraft.world.level.block.state.properties.BlockSetType) f.get(block);
                        if (t != null) {
                            setType = t.name();
                            byHand = t.canOpenByHand();
                        }
                    } else if (f.getType() == net.minecraft.world.level.block.state.properties.WoodType.class && woodType == null) {
                        f.setAccessible(true);
                        var t = (net.minecraft.world.level.block.state.properties.WoodType) f.get(block);
                        if (t != null) woodType = t.name();
                    }
                } catch (ReflectiveOperationException | RuntimeException e) {
                    // Not readable: left out.
                }
            }
        }
        if (setType != null) {
            json.name("block_set_type");
            json.beginObject();
            json.name("name").value(setType);
            json.name("can_open_by_hand").value(byHand);
            json.endObject();
        }
        if (woodType != null) json.name("wood_type").value(woodType);
    }

    void sound(SoundType s) throws IOException {
        json.name("sound");
        json.beginObject();
        json.name("volume").value(s.getVolume());
        json.name("pitch").value(s.getPitch());
        event("break", s.getBreakSound());
        event("step", s.getStepSound());
        event("place", s.getPlaceSound());
        event("hit", s.getHitSound());
        event("fall", s.getFallSound());
        json.endObject();
    }

    void event(String name, SoundEvent e) throws IOException {
        json.name(name).value(e.location().toString());
    }

    protected void boxes(VoxelShape shape) throws IOException {
        json.beginArray();
        for (AABB b : shape.toAabbs()) {
            json.beginArray();
            json.value(b.minX).value(b.minY).value(b.minZ).value(b.maxX).value(b.maxY).value(b.maxZ);
            json.endArray();
        }
        json.endArray();
    }

    /** Vanilla's items, so harnesses can tell later versions' additions apart. */
    public void vanillaItems() throws IOException {
        json.name("vanilla_items");
        json.beginArray();
        for (Item item : BuiltInRegistries.ITEM) {
            Identifier id = BuiltInRegistries.ITEM.getKey(item);
            if (scope.vanilla(BuiltInRegistries.ITEM, id)) json.value(id.toString());
        }
        json.endArray();
    }

    public void items() throws IOException {
        json.name("items");
        json.beginObject();
        for (Item item : BuiltInRegistries.ITEM) {
            Identifier id = BuiltInRegistries.ITEM.getKey(item);
            if (!scope.mod(BuiltInRegistries.ITEM, id)) continue;
            ItemStack stack = new ItemStack(item);
            json.name(id.toString());
            json.beginObject();
            json.name("class").value(item.getClass().getName());
            json.name("max_stack_size").value(stack.getMaxStackSize());
            json.name("rarity").value(stack.getRarity().getSerializedName());
            if (item instanceof BlockItem bi) {
                json.name("block").value(BuiltInRegistries.BLOCK.getKey(bi.getBlock()).toString());
            }
            var food = stack.get(DataComponents.FOOD);
            if (food != null) {
                json.name("food");
                json.beginObject();
                json.name("nutrition").value(food.nutrition());
                json.name("saturation").value(food.saturation());
                json.endObject();
            }
            components(stack);
            json.endObject();
        }
        json.endObject();
    }

    /** The default stack's every component, each by its type's codec, sorted by type. */
    void components(ItemStack stack) throws IOException {
        Map<String, JsonElement> sorted = new TreeMap<>();
        for (TypedDataComponent<?> c : stack.getComponents()) {
            var encoded = c.encodeValue(ops).result();
            if (encoded.isPresent()) sorted.put(BuiltInRegistries.DATA_COMPONENT_TYPE.getKey(c.type()).toString(), encoded.get());
        }
        json.name("components");
        json.beginObject();
        for (var e : sorted.entrySet()) {
            json.name(e.getKey());
            json.jsonValue(e.getValue().toString());
        }
        json.endObject();
    }

    public void dataComponentTypes() throws IOException {
        json.name("data_component_types");
        json.beginObject();
        for (var type : BuiltInRegistries.DATA_COMPONENT_TYPE) {
            Identifier id = BuiltInRegistries.DATA_COMPONENT_TYPE.getKey(type);
            if (!scope.mod(BuiltInRegistries.DATA_COMPONENT_TYPE, id)) continue;
            json.name(id.toString());
            json.beginObject();
            json.name("persistent").value(!type.isTransient());
            json.endObject();
        }
        json.endObject();
    }

    /** Tags that hold one of the mods' entries, with only those members; and the whole tags `wholeTag` names. */
    public void tags() throws IOException {
        json.name("tags");
        json.beginObject();
        for (String kind : new String[] {"block", "item"}) {
            json.name(kind);
            json.beginObject();
            Map<String, Set<String>> out = new TreeMap<>();
            Registry<?> registry = kind.equals("block") ? BuiltInRegistries.BLOCK : BuiltInRegistries.ITEM;
            registry.getTags().forEach(pair -> {
                Identifier tag = pair.key().location();
                boolean whole = scope.wholeTag(tag);
                Set<String> members = new LinkedHashSet<>();
                for (var holder : pair.contents()) {
                    Identifier id = holder.unwrapKey().orElseThrow().identifier();
                    if (whole || scope.mod(registry, id)) members.add(id.toString());
                }
                if (!members.isEmpty()) out.put(tag.toString(), new TreeSet<>(members));
            });
            for (var e : out.entrySet()) {
                json.name(e.getKey());
                json.beginArray();
                for (String m : e.getValue()) json.value(m);
                json.endArray();
            }
            json.endObject();
        }
        json.endObject();
    }

    public static String stateKey(BlockState state) {
        StringBuilder b = new StringBuilder();
        for (Property<?> p : state.getProperties()) {
            if (b.length() > 0) b.append(',');
            b.append(p.getName()).append('=').append(name(p, state.getValue(p)));
        }
        return b.toString();
    }

    @SuppressWarnings({"unchecked", "rawtypes"})
    public static String name(Property p, Object v) {
        return p.getName((Comparable) v);
    }
}
