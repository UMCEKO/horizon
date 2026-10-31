package dev.horizonlab.javamods.boot;

import com.mojang.authlib.GameProfile;
import dev.horizonlab.javamods.boot.mixin.GameModeAccessor;
import dev.horizonlab.javamods.boot.mixin.ServerPlayerAccessor;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.lang.reflect.Constructor;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import net.minecraft.commands.Commands;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.LayeredRegistryAccess;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.Tag;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.MinecraftServer;
import net.minecraft.server.RegistryLayer;
import net.minecraft.server.ReloadableServerResources;
import net.minecraft.server.WorldLoader;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.level.ServerPlayerGameMode;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.packs.resources.CloseableResourceManager;
import net.minecraft.server.permissions.LevelBasedPermissionSet;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.util.ProblemReporter;
import net.minecraft.util.Unit;
import net.minecraft.util.Util;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.EquipmentSlot;
import net.minecraft.world.entity.player.Inventory;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.GameType;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.WorldDataConfiguration;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.block.state.properties.Property;
import net.minecraft.world.level.dimension.BuiltinDimensionTypes;
import net.minecraft.world.level.dimension.DimensionType;
import net.minecraft.world.level.levelgen.WorldDimensions;
import net.minecraft.world.level.levelgen.presets.WorldPresets;
import net.minecraft.world.level.storage.TagValueOutput;
import net.minecraft.world.level.storage.WritableLevelData;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.common.CommonHooks;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.EventHooks;
import net.neoforged.neoforge.event.level.LevelEvent;
import net.neoforged.neoforge.resource.ResourcePackLoader;
import net.neoforged.neoforge.server.ServerLifecycleHooks;
import sun.reflect.ReflectionFactory;

/** The game's own objects as views of Horizon's state: built without a server, filled for each call, and what the call changed goes back. */
public final class GameBridge {
    private GameBridge() {}

    private record Sound(String id, double x, double y, double z, float volume, float pitch) {}

    private static final int SLOTS = 41;
    private static final Map<String, ServerLevel> LEVELS = new HashMap<>();
    private static final Map<UUID, ServerPlayer> PLAYERS = new HashMap<>();
    private static final List<Sound> SOUNDS = new ArrayList<>();
    private static final List<Entity> SPAWNED = new ArrayList<>();
    private static RegistryAccess.Frozen registries;

    static RegistryAccess.Frozen registries() {
        return registries;
    }

    private static MinecraftServer server;
    private static WorldDimensions dimensions;
    private static int nextEntityId = -1;
    private static int depth;

    public static byte[] call(String op, byte[] input) {
        depth++;
        try {
            return switch (op) {
                case "prepare" -> prepare();
                case "items" -> items();
                case "states" -> states();
                case "refresh" -> refresh(input);
                case "block_events" -> blockEvents(input);
                case "block_entity_tick" -> blockEntityTick(input);
                case "block_entity_load" -> blockEntityLoad(input);
                case "block_entities_save" -> blockEntitiesSave(input);
                case "block_ticks" -> blockTicks(input);
                case "click_block" -> use(input, 2);
                case "event_click_block" -> use(input, 3);
                case "event_use_item" -> use(input, 4);
                case "event_break" -> use(input, 5);
                case "event_attack" -> use(input, 6);
                case "player_tick" -> use(input, 7);
                case "player_left" -> playerLeft(input);
                case "server_start" -> lifecycle(input, true);
                case "server_stop" -> lifecycle(input, false);
                case "server_tick" -> serverTick(input);
                case "level_tick" -> levelTick(input);
                case "use_item" -> use(input, 0);
                case "use_item_on" -> use(input, 1);
                default -> throw new IllegalArgumentException(op);
            };
        } catch (Throwable t) {
            t.printStackTrace();
            if (depth == 1) abandonCall();
            return new byte[0];
        } finally {
            depth--;
        }
    }

    private static byte[] prepare() throws Exception {
        if (registries != null) return new byte[] {1};
        var repository = ServerPacksSource.createVanillaTrustedRepository();
        ResourcePackLoader.populatePackRepository(repository, PackType.SERVER_DATA, true);
        var packs = new WorldLoader.PackConfig(repository, WorldDataConfiguration.DEFAULT, false, true);
        var config = new WorldLoader.InitConfig(packs, Commands.CommandSelection.DEDICATED, LevelBasedPermissionSet.GAMEMASTER);
        record Loaded(CloseableResourceManager manager, ReloadableServerResources resources, LayeredRegistryAccess<RegistryLayer> layers) {}
        Loaded loaded = Util.blockUntilDone(executor -> WorldLoader.load(
                config,
                context -> {
                    var dimensions = context.datapackWorldRegistries().lookupOrThrow(Registries.WORLD_PRESET).getOrThrow(WorldPresets.NORMAL).value().createWorldDimensions();
                    var complete = dimensions.bake(context.datapackDimensions().lookupOrThrow(Registries.LEVEL_STEM));
                    GameBridge.dimensions = dimensions;
                    return new WorldLoader.DataLoadOutput<>(Unit.INSTANCE, complete.dimensionsRegistryAccess());
                },
                (manager, resources, layers, unit) -> new Loaded(manager, resources, layers),
                Util.backgroundExecutor(),
                executor)).get();
        registries = loaded.layers.compositeAccess();
        server = ServerView.create(loaded.manager, loaded.resources, loaded.layers);
        return new byte[] {1};
    }

    private static byte[] items() throws IOException {
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        var ids = BuiltInRegistries.ITEM.keySet().stream().filter(id -> !id.getNamespace().equals("minecraft")).map(Identifier::toString).toList();
        out.writeInt(ids.size());
        for (String id : ids) text(out, id);
        return bytes.toByteArray();
    }

    /** Every Java block state by id: name, then property and value pairs. */
    private static byte[] states() throws IOException {
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(Block.BLOCK_STATE_REGISTRY.size());
        out.writeInt(Block.BLOCK_STATE_REGISTRY.getId(Blocks.AIR.defaultBlockState()));
        for (BlockState state : Block.BLOCK_STATE_REGISTRY) {
            out.writeInt(Block.BLOCK_STATE_REGISTRY.getId(state));
            text(out, BuiltInRegistries.BLOCK.getKey(state.getBlock()).toString());
            var properties = state.getProperties();
            out.writeInt(properties.size());
            for (Property<?> property : properties) {
                text(out, property.getName());
                text(out, valueName(state, property));
            }
        }
        return bytes.toByteArray();
    }

    private static <T extends Comparable<T>> String valueName(BlockState state, Property<T> property) {
        return property.getName(state.getValue(property));
    }

    static <T> T construct(Class<T> type, Constructor<?> parent, Object... args) throws ReflectiveOperationException {
        parent.setAccessible(true);
        return type.cast(ReflectionFactory.getReflectionFactory().newConstructorForSerialization(type, parent).newInstance(args));
    }

    private static ServerLevel level(String id) throws ReflectiveOperationException {
        var level = LEVELS.get(id);
        if (level != null) return level;
        var key = ResourceKey.create(Registries.DIMENSION, Identifier.parse(id));
        ResourceKey<DimensionType> type = key == Level.NETHER ? BuiltinDimensionTypes.NETHER : key == Level.END ? BuiltinDimensionTypes.END : BuiltinDimensionTypes.OVERWORLD;
        Holder<DimensionType> holder = registries.lookupOrThrow(Registries.DIMENSION_TYPE).getOrThrow(type);
        var parent = Level.class.getDeclaredConstructor(WritableLevelData.class, ResourceKey.class, RegistryAccess.class, Holder.class, boolean.class, boolean.class, long.class, int.class);
        level = construct(ServerLevel.class, parent, new ViewLevelData(), key, registries, holder, false, false, 0L, 1000000);
        ServerView.attach(server, level);
        LEVELS.put(id, level);
        return level;
    }

    private static ServerPlayer player(UUID id, String name, ServerLevel level) throws ReflectiveOperationException {
        var player = PLAYERS.get(id);
        if (player == null) {
            player = construct(ServerPlayer.class, Player.class.getConstructor(Level.class, GameProfile.class), level, new GameProfile(id, name));
            ((ServerPlayerAccessor) player).horizon$gameMode(new ServerPlayerGameMode(player));
            ServerView.attach(server, player);
            PLAYERS.put(id, player);
        } else if (player.level() != level) {
            player.setServerLevel(level);
        }
        return player;
    }

    /** The acting players with views in this level. */
    static List<ServerPlayer> playersIn(ServerLevel level) {
        return level.players();
    }

    /** Ids for views of entities, which Horizon gives its own when they reach it; negative so they never meet Horizon's. */
    public static int nextEntityId() {
        return nextEntityId--;
    }

    public static void sound(SoundEvent sound, double x, double y, double z, float volume, float pitch) {
        SOUNDS.add(new Sound(sound.location().toString(), x, y, z, volume, pitch));
    }

    /** Spawned for the rest of the call as vanilla's are, and handed to Horizon as it ends, with what the call did to it since. */
    public static boolean spawn(Entity entity) {
        SPAWNED.add(entity);
        return true;
    }

    static List<Entity> spawned(ServerLevel level) {
        var here = new ArrayList<Entity>();
        for (Entity entity : SPAWNED) if (entity.level() == level && !entity.isRemoved()) here.add(entity);
        return here;
    }

    private static CompoundTag saved(Entity entity) {
        var output = TagValueOutput.createWithContext(ProblemReporter.DISCARDING, entity.level().registryAccess());
        return entity.save(output) ? output.buildResult() : null;
    }

    private static ItemStack decode(byte[] nbt) throws IOException {
        Tag tag = NbtIo.read(new DataInputStream(new ByteArrayInputStream(nbt)), NbtAccounter.unlimitedHeap());
        return ItemStack.CODEC.parse(registries.createSerializationContext(NbtOps.INSTANCE), tag).getOrThrow();
    }

    static void writeTag(DataOutputStream out, CompoundTag tag) throws IOException {
        var bytes = new ByteArrayOutputStream();
        NbtIo.write(tag, new DataOutputStream(bytes));
        out.writeInt(bytes.size());
        bytes.writeTo(out);
    }

    private static void text(DataOutputStream out, String value) throws IOException {
        byte[] bytes = value.getBytes(java.nio.charset.StandardCharsets.UTF_8);
        out.writeInt(bytes.length);
        out.write(bytes);
    }

    private static String text(DataInputStream in) throws IOException {
        return new String(in.readNBytes(in.readInt()), java.nio.charset.StandardCharsets.UTF_8);
    }

    private static byte[] useItem(byte[] input) throws Exception {
        return use(input, 0);
    }

    private record Call(DataInputStream in, ServerLevel level, ServerPlayer player, InteractionHand hand, ItemStack[] before) {}

    /** Fills the views from what Horizon holds now. */
    private static Call fill(byte[] input, boolean refresh) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        var id = new UUID(in.readLong(), in.readLong());
        String name = text(in);
        var level = level(text(in));
        ((ViewLevelData) level.getLevelData()).gameTime = in.readLong();
        boolean joined = !PLAYERS.containsKey(id);
        var player = player(id, name, level);
        player.setPosRaw(in.readDouble(), in.readDouble(), in.readDouble());
        player.setYRot(in.readFloat());
        player.setXRot(in.readFloat());
        player.setShiftKeyDown(in.readBoolean());
        var type = GameType.byId(in.readInt());
        ((GameModeAccessor) player.gameMode).horizon$type(type);
        type.updatePlayerAbilities(player.getAbilities());
        var hand = in.readInt() == 0 ? InteractionHand.MAIN_HAND : InteractionHand.OFF_HAND;
        Inventory inventory = player.getInventory();
        inventory.setSelectedSlot(in.readInt());
        int experienceLevel = in.readInt();
        float experienceProgress = in.readFloat();
        int totalExperience = in.readInt();
        ServerView.setHealth(player, in.readFloat());
        ServerView.setFood(player, in.readInt(), in.readFloat());
        ServerView.setMotion(player, new Vec3(in.readDouble(), in.readDouble(), in.readDouble()), in.readBoolean());
        var before = new ItemStack[SLOTS];
        java.util.Arrays.fill(before, ItemStack.EMPTY);
        // A refresh mid-call leaves what Java itself holds until the call's end: experience and stacks.
        if (!refresh) {
            player.experienceLevel = experienceLevel;
            player.experienceProgress = experienceProgress;
            player.totalExperience = totalExperience;
            inventory.clearContent();
            for (int n = in.readInt(); n > 0; n--) {
                int slot = in.readInt();
                ItemStack stack = decode(in.readNBytes(in.readInt()));
                before[slot] = stack.copy();
                inventory.setItem(slot, stack);
            }
        }
        if (joined) safely("PlayerLoggedIn", () -> EventHooks.firePlayerLoggedIn(player));
        return new Call(in, level, player, hand, before);
    }

    private static byte[] refresh(byte[] input) throws Exception {
        fill(input, true);
        return new byte[] {1};
    }

    private static byte[] use(byte[] input, int kind) throws Exception {
        var call = fill(input, false);
        var in = call.in;
        var level = call.level;
        var player = call.player;
        var hand = call.hand;
        var before = call.before;
        var inventory = player.getInventory();
        InteractionResult result = InteractionResult.PASS;
        ItemStack held = player.getItemInHand(hand);
        switch (kind) {
            case 0 -> result = player.gameMode.useItem(player, level, held, hand);
            case 1, 2, 3 -> {
                var pos = BlockPos.of(in.readLong());
                var face = Direction.from3DDataValue(in.readInt());
                var hit = new BlockHitResult(new Vec3(in.readDouble(), in.readDouble(), in.readDouble()), face, pos, false);
                if (kind == 1) result = held.useOn(new UseOnContext(player, hand, hit));
                else if (kind == 2) result = player.gameMode.useItemOn(player, level, held, hand, hit);
                else if (CommonHooks.onRightClickBlock(player, hand, pos, hit).isCanceled()) result = InteractionResult.SUCCESS;
            }
            case 4 -> {
                var answer = CommonHooks.onItemRightClick(player, hand);
                if (answer != null) result = answer;
            }
            case 5 -> {
                var pos = BlockPos.of(in.readLong());
                if (CommonHooks.fireBlockBreak(level, type(player), player, pos, level.getBlockState(pos)).isCanceled()) result = InteractionResult.FAIL;
            }
            case 6 -> {
                var target = level.getEntity(in.readInt());
                if (target != null && !CommonHooks.onPlayerAttackTarget(player, target)) result = InteractionResult.FAIL;
            }
            default -> {
                EventHooks.firePlayerTickPre(player);
                for (int slot = 0; slot < Inventory.INVENTORY_SIZE; slot++) {
                    ItemStack stack = inventory.getItem(slot);
                    if (!stack.isEmpty() && !BuiltInRegistries.ITEM.getKey(stack.getItem()).getNamespace().equals("minecraft")) {
                        stack.inventoryTick(level, player, slot == inventory.getSelectedSlot() ? EquipmentSlot.MAINHAND : null);
                    }
                }
                EventHooks.firePlayerTickPost(player);
            }
        }
        if (System.getenv("HORIZON_JAVA_DEBUG") != null) System.err.println("Horizon: use kind=" + kind + " result=" + result + " held=" + player.getItemInHand(hand));
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeBoolean(result != InteractionResult.PASS && result != InteractionResult.TRY_WITH_EMPTY_HAND);
        out.writeInt(player.experienceLevel);
        out.writeFloat(player.experienceProgress);
        out.writeInt(player.totalExperience);
        var ops = registries.createSerializationContext(NbtOps.INSTANCE);
        var changed = new ArrayList<Integer>();
        for (int slot = 0; slot < SLOTS; slot++) {
            if (!ItemStack.matches(before[slot], inventory.getItem(slot))) changed.add(slot);
        }
        out.writeInt(changed.size());
        for (int slot : changed) {
            out.writeInt(slot);
            ItemStack stack = inventory.getItem(slot);
            if (stack.isEmpty()) out.writeInt(0);
            else writeTag(out, (CompoundTag) ItemStack.CODEC.encodeStart(ops, stack).getOrThrow());
        }
        writeEffects(out);
        return bytes.toByteArray();
    }

    /** A call that threw has no reply to carry its effects, and the next must not read its copies. */
    private static void abandonCall() {
        SOUNDS.clear();
        SPAWNED.clear();
        BlockEntities.abandonCopies();
        EntityViews.abandon();
    }

    /** What Java did to the world since the last reply. */
    private static void writeEffects(DataOutputStream out) throws IOException {
        var sounds = new ArrayList<>(SOUNDS);
        var spawned = new ArrayList<CompoundTag>();
        var copies = depth == 1 ? BlockEntities.takeChangedCopies() : List.<CompoundTag>of();
        SOUNDS.clear();
        if (depth == 1) {
            for (Entity entity : SPAWNED) {
                CompoundTag tag = entity.isRemoved() ? null : saved(entity);
                if (tag != null) spawned.add(tag);
            }
            SPAWNED.clear();
        }
        out.writeInt(sounds.size());
        for (Sound s : sounds) {
            text(out, s.id);
            out.writeDouble(s.x);
            out.writeDouble(s.y);
            out.writeDouble(s.z);
            out.writeFloat(s.volume);
            out.writeFloat(s.pitch);
        }
        out.writeInt(spawned.size());
        for (CompoundTag tag : spawned) writeTag(out, tag);
        out.writeInt(copies.size());
        for (CompoundTag tag : copies) writeTag(out, tag);
        if (depth == 1) {
            EntityViews.writeChanges(out);
        } else {
            out.writeInt(0);
            out.writeInt(0);
        }
    }

    private static byte[] effects() throws IOException {
        var bytes = new ByteArrayOutputStream();
        writeEffects(new DataOutputStream(bytes));
        return bytes.toByteArray();
    }

    private static GameType type(ServerPlayer player) {
        return player.gameMode.getGameModeForPlayer();
    }

    private static void safely(String what, Runnable step) {
        try {
            step.run();
        } catch (Throwable t) {
            System.err.println("Horizon: a Java mod threw during " + what);
            t.printStackTrace();
        }
    }

    private static boolean serverStarted;
    private static Path worldFolder = Path.of("world");

    public static Path worldFolder() {
        return worldFolder;
    }

    private static byte[] lifecycle(byte[] input, boolean start) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        var here = levelOf(in);
        if (start && !serverStarted) {
            serverStarted = true;
            worldFolder = Path.of(text(in));
            for (String id : List.of("minecraft:overworld", "minecraft:the_nether", "minecraft:the_end")) level(id);
            ServerView.openWorld(server, LEVELS.values(), worldFolder, here.getSeed(), dimensions);
            safely("ServerAboutToStart", () -> ServerLifecycleHooks.handleServerAboutToStart(server));
            safely("ServerStarting", () -> ServerLifecycleHooks.handleServerStarting(server));
            for (ServerLevel level : LEVELS.values()) safely("LevelEvent.Load", () -> NeoForge.EVENT_BUS.post(new LevelEvent.Load(level)));
            safely("ServerStarted", () -> ServerLifecycleHooks.handleServerStarted(server));
        } else if (!start && serverStarted) {
            serverStarted = false;
            safely("ServerStopping", () -> ServerLifecycleHooks.handleServerStopping(server));
            for (ServerLevel level : LEVELS.values()) safely("LevelEvent.Unload", () -> NeoForge.EVENT_BUS.post(new LevelEvent.Unload(level)));
            safely("ServerStopped", () -> ServerLifecycleHooks.handleServerStopped(server));
            safely("saving saved data", () -> ServerView.saveWorld(server, LEVELS.values()));
        }
        return effects();
    }

    private static byte[] serverTick(byte[] input) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        levelOf(in);
        if (in.readBoolean()) {
            safely("ServerTickEvent.Post", () -> EventHooks.fireServerTickPost(() -> true, server));
        } else {
            ServerView.tick(server);
            safely("ServerTickEvent.Pre", () -> EventHooks.fireServerTickPre(() -> true, server));
        }
        return effects();
    }

    private static byte[] levelTick(byte[] input) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        var level = levelOf(in);
        if (in.readBoolean()) safely("LevelTickEvent.Post", () -> EventHooks.fireLevelTickPost(level, () -> true));
        else safely("LevelTickEvent.Pre", () -> EventHooks.fireLevelTickPre(level, () -> true));
        return effects();
    }

    private static byte[] playerLeft(byte[] input) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        var player = PLAYERS.remove(new UUID(in.readLong(), in.readLong()));
        if (player != null) {
            safely("PlayerLoggedOut", () -> EventHooks.firePlayerLoggedOut(player));
            ServerView.detach(server, player);
        }
        return effects();
    }

    /** The level a call acts on, with the time it arrived at. */
    private static ServerLevel levelOf(DataInputStream in) throws Exception {
        var level = level(text(in));
        ((ViewLevelData) level.getLevelData()).gameTime = in.readLong();
        level.environmentAttributes().invalidateTickCache();
        return level;
    }

    private static byte[] blockEvents(byte[] input) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        var level = levelOf(in);
        for (int n = in.readInt(); n > 0; n--) {
            int kind = in.readInt();
            var pos = BlockPos.of(in.readLong());
            int a = in.readInt();
            int b = in.readInt();
            BlockState state = Block.BLOCK_STATE_REGISTRY.byId(a);
            try {
                switch (kind) {
                    case 0 -> BlockEntities.blockChanged(level, pos, state, Block.BLOCK_STATE_REGISTRY.byId(b));
                    case 1 -> state.handleNeighborChanged(level, pos, state.getBlock(), null, b != 0);
                    case 2 -> state.tick(level, pos, level.getRandom());
                    default -> {
                        if (state.isRandomlyTicking()) state.randomTick(level, pos, level.getRandom());
                    }
                }
            } catch (Throwable t) {
                t.printStackTrace();
            }
        }
        return effects();
    }

    private static byte[] blockEntityTick(byte[] input) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        var level = levelOf(in);
        boolean held = BlockEntities.tick(level, BlockPos.of(in.readLong()));
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeBoolean(held);
        writeEffects(out);
        return bytes.toByteArray();
    }

    private static byte[] blockEntityLoad(byte[] input) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        var level = levelOf(in);
        var pos = BlockPos.of(in.readLong());
        var tag = NbtIo.read(new DataInputStream(new ByteArrayInputStream(in.readNBytes(in.readInt()))), NbtAccounter.unlimitedHeap());
        BlockEntities.load(level, pos, tag);
        return effects();
    }

    private static byte[] blockEntitiesSave(byte[] input) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        safely("saving saved data", () -> ServerView.saveWorld(server, LEVELS.values()));
        var saved = BlockEntities.saveAll(level(text(in)));
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(saved.size());
        for (CompoundTag tag : saved) writeTag(out, tag);
        return bytes.toByteArray();
    }

    /** Whether a block's block entity ticks; an empty reply says no. */
    private static byte[] blockTicks(byte[] input) throws Exception {
        var id = Identifier.tryParse(new String(input, java.nio.charset.StandardCharsets.UTF_8));
        if (id == null || !BuiltInRegistries.BLOCK.containsKey(id)) return new byte[0];
        return BlockEntities.ticks(BuiltInRegistries.BLOCK.getValue(id), level("minecraft:overworld")) ? new byte[] {1} : new byte[0];
    }

    /** A dev run without the game: real data packs, then a level and a player view of each dimension; a mod's reload listener or a mixin on these classes shows here. */
    static String worldProbe() throws Exception {
        prepare();
        var ids = List.of("minecraft:overworld", "minecraft:the_nether", "minecraft:the_end");
        for (String id : ids) level(id);
        ServerView.openWorld(server, LEVELS.values(), Path.of("world").toAbsolutePath(), 1L, dimensions);
        worldFolder = Path.of("world").toAbsolutePath();
        ServerLifecycleHooks.handleServerAboutToStart(server);
        ServerLifecycleHooks.handleServerStarting(server);
        for (ServerLevel level : LEVELS.values()) NeoForge.EVENT_BUS.post(new LevelEvent.Load(level));
        ServerLifecycleHooks.handleServerStarted(server);
        int views = 0;
        for (String id : ids) player(new UUID(1, views++), "Probe", level(id));
        ServerView.tick(server);
        EventHooks.fireServerTickPre(() -> true, server);
        EventHooks.fireServerTickPost(() -> true, server);
        return "WORLD OK " + views + " levels, " + BuiltInRegistries.ITEM.size() + " items, " + Block.BLOCK_STATE_REGISTRY.size() + " states";
    }

    /** A dev run without the game: one use of item `id` by a level 10 player, as text. */
    static String probe(String id, boolean sneaking) throws Exception {
        prepare();
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeLong(1);
        out.writeLong(2);
        text(out, "Probe");
        text(out, "minecraft:overworld");
        out.writeLong(100);
        out.writeDouble(0.5);
        out.writeDouble(64);
        out.writeDouble(0.5);
        out.writeFloat(0);
        out.writeFloat(0);
        out.writeBoolean(sneaking);
        out.writeInt(0);
        out.writeInt(0);
        out.writeInt(0);
        out.writeInt(10);
        out.writeFloat(0);
        out.writeInt(160);
        out.writeFloat(20);
        out.writeInt(20);
        out.writeFloat(5);
        out.writeDouble(0);
        out.writeDouble(0);
        out.writeDouble(0);
        out.writeBoolean(true);
        out.writeInt(1);
        out.writeInt(0);
        var tag = new CompoundTag();
        tag.putString("id", id);
        tag.putInt("count", 1);
        var stack = new ByteArrayOutputStream();
        NbtIo.write(tag, new DataOutputStream(stack));
        out.writeInt(stack.size());
        stack.writeTo(out);
        long began = System.nanoTime();
        var reply = new DataInputStream(new ByteArrayInputStream(useItem(bytes.toByteArray())));
        long took = System.nanoTime() - began;
        var sb = new StringBuilder("claimed=" + reply.readBoolean() + " level=" + reply.readInt() + " progress=" + reply.readFloat() + " total=" + reply.readInt());
        for (int n = reply.readInt(); n > 0; n--) {
            int slot = reply.readInt();
            int size = reply.readInt();
            sb.append(" slot").append(slot).append('=');
            sb.append(size == 0 ? "empty" : NbtIo.read(new DataInputStream(new ByteArrayInputStream(reply.readNBytes(size))), NbtAccounter.unlimitedHeap()).toString());
        }
        sb.append(" took_us=").append(took / 1000);
        return sb.toString();
    }
}
