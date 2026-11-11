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
    private static RegistryAccess.Frozen registries;

    static RegistryAccess.Frozen registries() {
        return registries;
    }

    private static MinecraftServer server;

    static MinecraftServer server() {
        return server;
    }
    private static WorldDimensions dimensions;
    private static int nextEntityId = -1;
    private static int depth;

    /** The client side's own calls, from the window's thread: set where the client classes exist. */
    static volatile java.util.function.BiFunction<String, byte[], byte[]> client;
    private static final Object LOCK = new Object();

    /** One call at a time, as the window's thread and the server's both call in; the views between them are shared. */
    static void locked(Crossing.Handler handler, Crossing.Frame frame) throws Throwable {
        long queued = Profile.ON ? System.nanoTime() : 0;
        synchronized (LOCK) {
            if (Profile.ON) Profile.wait(handler.name(), System.nanoTime() - queued);
            if (handler.kind() == Crossing.Kind.CLIENT) Crossing.run(handler, frame);
            else game(handler, frame);
        }
    }

    /** Every op Rust may ask, in the order it reads them. */
    static void register() {
        // Codecs touch no view, and a window's network thread must not wait out a server tick for one.
        Crossing.free("component_read", StackComponents::read);
        Crossing.free("component_write", StackComponents::write);
        Crossing.free("profile_take", input -> Profile.take().getBytes(java.nio.charset.StandardCharsets.UTF_8));
        Crossing.free("profile_dump", GameBridge::profileDump);
        Crossing.free("bench_echo_free", input -> input);
        Crossing.game("prepare", input -> prepare());
        Crossing.game("items", input -> items());
        Crossing.game("states", input -> states());
        Crossing.game("refresh", GameBridge::refresh);
        Crossing.game("clear_use", GameBridge::clearUse);
        Crossing.game("block_events", GameBridge::blockEvents);
        Crossing.game("block_entity_tick", GameBridge::blockEntityTick);
        Crossing.game("block_entity_load", GameBridge::blockEntityLoad);
        Crossing.game("block_entity_drop", GameBridge::blockEntityDrop);
        Crossing.game("entity_view", GameBridge::entityView);
        Crossing.game("entity_left", GameBridge::entityLeft);
        Crossing.game("entity_refresh", GameBridge::entityRefresh);
        Crossing.game("entities_save", GameBridge::entitiesSave);
        Crossing.game("block_entities_save", GameBridge::blockEntitiesSave);
        Crossing.game("chunks_changed", GameBridge::chunksChanged);
        Crossing.game("chunk_events", GameBridge::chunkEvents);
        Crossing.game("tracking", GameBridge::tracking);
        Crossing.game("block_entity_data", GameBridge::blockEntityData);
        Crossing.game("block_entity_set", GameBridge::blockEntitySet);
        Crossing.game("hopper_transfer", GameBridge::hopperTransfer);
        Crossing.game("block_ticks", GameBridge::blockTicks);
        Crossing.game("entity_facts", input -> EntityFacts.bin());
        Crossing.game("routes_bin", input -> Routing.bin());
        Crossing.game("route", Routing::call);
        Crossing.game("bench_route", Routing::benchCall);
        Crossing.game("entity_types", input -> EntityFacts.types(level("minecraft:overworld")));
        Crossing.game("click_block", (in, out) -> use(in, out, 2));
        Crossing.game("event_click_block", (in, out) -> use(in, out, 3));
        Crossing.game("event_use_item", (in, out) -> use(in, out, 4));
        Crossing.game("event_break", (in, out) -> use(in, out, 5));
        Crossing.game("destroy_progress", GameBridge::destroyProgress);
        Crossing.game("correct_tool", GameBridge::correctTool);
        Crossing.game("mine_block", (in, out) -> use(in, out, 12));
        Crossing.game("destroy_block", (in, out) -> use(in, out, 13));
        Crossing.game("event_attack", (in, out) -> use(in, out, 6));
        Crossing.game("player_tick", (in, out) -> use(in, out, 7));
        Crossing.game("player_left", GameBridge::playerLeft);
        Crossing.game("remote_join", GameBridge::remoteJoin);
        Crossing.game("remote_in", GameBridge::remoteIn);
        Crossing.game("remote_config", input -> remoteConfig());
        Crossing.game("server_start", input -> lifecycle(input, true));
        Crossing.game("server_stop", input -> lifecycle(input, false));
        Crossing.game("server_stopping", GameBridge::serverStopping);
        Crossing.game("players_removed", GameBridge::playersRemoved);
        Crossing.game("server_tick", GameBridge::serverTick);
        Crossing.game("level_tick", GameBridge::levelTick);
        Crossing.game("use_item", (in, out) -> use(in, out, 0));
        Crossing.game("use_item_on", (in, out) -> use(in, out, 1));
        Crossing.game("use_tick", (in, out) -> use(in, out, 8));
        Crossing.game("release_using", (in, out) -> use(in, out, 9));
        Crossing.game("stop_using", (in, out) -> use(in, out, 10));
        Crossing.game("interact_entity", (in, out) -> use(in, out, 11));
        Crossing.game("bench_empty", input -> new byte[] {1});
        Crossing.game("bench_empty_direct", (in, out) -> out.writeByte(1));
        Crossing.game("bench_echo", (in, out) -> out.write(in.rest()));
        Crossing.game("bench_big", (in, out) -> out.write(new byte[in.readInt()]));
        Crossing.game("bench_nested", GameBridge::benchNested);
        Crossing.game("bench_throw", input -> {
            throw new IllegalStateException("bench_throw");
        });
    }

    private static byte[] profileDump(byte[] input) {
        try {
            return Profile.dump(new String(input, java.nio.charset.StandardCharsets.UTF_8)).getBytes(java.nio.charset.StandardCharsets.UTF_8);
        } catch (Throwable t) {
            return ("ERROR " + t).getBytes(java.nio.charset.StandardCharsets.UTF_8);
        }
    }

    private static void game(Crossing.Handler handler, Crossing.Frame frame) {
        depth++;
        try {
            Crossing.run(handler, frame);
        } catch (Throwable t) {
            t.printStackTrace();
            if (depth == 1) abandonCall();
            frame.out.reset();
        } finally {
            if (--depth == 0) PlayerSync.acting(null);
        }
    }

    /** The crossing bench's reentrancy op: a native calls back into `bench_echo`, then this reads the rest of its own input. */
    private static void benchNested(Crossing.In in, Crossing.Out out) throws IOException {
        int tag = in.readInt();
        int back = dev.horizonlab.javamods.host.Host.benchBack(tag);
        out.writeInt(tag);
        out.writeInt(back);
        out.writeInt(in.readInt());
    }

    private static ReloadableServerResources resources;

    static byte[] prepare() throws Exception {
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
        resources = loaded.resources;
        server = ServerView.create(loaded.manager, loaded.resources, loaded.layers);
        var view = server;
        if (net.neoforged.fml.loading.FMLEnvironment.getDist().isClient()) Clients.configuration(new Clients.Configuration(() -> Menus.startConfiguration(view), known -> Menus.knownPacks(view, known)));
        return new byte[] {1};
    }

    private static byte[] items() throws IOException {
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        var ids = BuiltInRegistries.ITEM.keySet().stream().filter(id -> BootFacts.owns(BuiltInRegistries.ITEM, id)).map(Identifier::toString).sorted().toList();
        out.writeInt(ids.size());
        for (String id : ids) {
            text(out, id);
            var item = BuiltInRegistries.ITEM.getValue(Identifier.parse(id));
            out.writeBoolean(ItemCode.use(item.getClass()));
            out.writeBoolean(ItemCode.useOn(item));
            out.writeBoolean(ItemCode.entity(item.getClass()));
            out.writeBoolean(ItemCode.mine(item.getClass()));
        }
        return bytes.toByteArray();
    }

    /** Every Java block state by id: name, then property and value pairs. */
    private static byte[] states() throws IOException {
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        var client = net.neoforged.fml.loading.FMLEnvironment.getDist().isClient();
        out.writeInt(Block.BLOCK_STATE_REGISTRY.size());
        out.writeInt(Block.BLOCK_STATE_REGISTRY.getId(Blocks.AIR.defaultBlockState()));
        out.writeBoolean(client && Animate.wholeLoop());
        for (BlockState state : Block.BLOCK_STATE_REGISTRY) {
            out.writeInt(Block.BLOCK_STATE_REGISTRY.getId(state));
            text(out, BuiltInRegistries.BLOCK.getKey(state.getBlock()).toString());
            var properties = state.getProperties();
            out.writeInt(properties.size());
            for (Property<?> property : properties) {
                text(out, property.getName());
                text(out, valueName(state, property));
            }
            out.writeBoolean(state.hasBlockEntity());
            out.writeBoolean(BootFacts.ownsBlock(state.getBlock()));
            out.writeByte(client ? Animate.route(state) : 0);
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

    static ServerLevel level(String id) throws ReflectiveOperationException {
        var level = LEVELS.get(id);
        if (level != null) return level;
        var key = ResourceKey.create(Registries.DIMENSION, Identifier.parse(id));
        ResourceKey<DimensionType> type = key == Level.NETHER ? BuiltinDimensionTypes.NETHER : key == Level.END ? BuiltinDimensionTypes.END : BuiltinDimensionTypes.OVERWORLD;
        Holder<DimensionType> holder = registries.lookupOrThrow(Registries.DIMENSION_TYPE).getOrThrow(type);
        var parent = Level.class.getDeclaredConstructor(WritableLevelData.class, ResourceKey.class, RegistryAccess.class, Holder.class, boolean.class, boolean.class, long.class, int.class);
        level = construct(ServerLevel.class, parent, new ViewLevelData(), key, registries, holder, false, false, 0L, 1000000);
        ServerView.attach(server, level);
        Inits.run(level, server, key, holder, level.getLevelData());
        LEVELS.put(id, level);
        return level;
    }

    static ServerPlayer playerView(UUID id) {
        return PLAYERS.get(id);
    }

    static ServerLevel levelOrNull(String id) {
        return LEVELS.get(id);
    }

    private static ServerPlayer player(UUID id, String name, ServerLevel level) throws ReflectiveOperationException {
        var player = PLAYERS.get(id);
        if (player == null) {
            player = construct(ServerPlayer.class, Player.class.getConstructor(Level.class, GameProfile.class), level, new GameProfile(id, name));
            ((ServerPlayerAccessor) player).horizon$gameMode(new ServerPlayerGameMode(player));
            ServerView.attach(server, player);
            Inits.run(player, server, level, player.getGameProfile());
            Inits.run(player.connection, server, player);
            Tracking.attach(player);
            PLAYERS.put(id, player);
        } else if (player.level() != level) {
            ServerView.moved(player, level);
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

    /** `ServerLevel.addFreshEntity` from mod code: native adds it at once and the object becomes its live view. */
    public static boolean spawn(Entity entity) {
        return EntityViews.spawn(entity);
    }

    /** A component in `Flat.raw(Component)`'s wrapper. */
    static net.minecraft.network.chat.Component component(byte[] nbt) throws IOException {
        var tag = NbtIo.read(new DataInputStream(new ByteArrayInputStream(nbt)), NbtAccounter.unlimitedHeap());
        return net.minecraft.network.chat.ComponentSerialization.CODEC.parse(registries.createSerializationContext(NbtOps.INSTANCE), tag.get("text")).getOrThrow();
    }

    static ItemStack decode(byte[] nbt) throws IOException {
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

    static String readText(DataInputStream in) throws IOException {
        return text(in);
    }

    static String text(Crossing.In in) {
        return in.name();
    }

    static String text(DataInputStream in) throws IOException {
        return new String(in.readNBytes(in.readInt()), java.nio.charset.StandardCharsets.UTF_8);
    }

    private static byte[] useItem(byte[] input) throws Exception {
        var out = new Crossing.Out();
        use(Crossing.In.of(input), out, 0);
        return out.toByteArray();
    }

    record Call(Crossing.In in, ServerLevel level, ServerPlayer player, InteractionHand hand, ItemStack[] before) {}

    /** Fills the views from what Horizon holds now. */
    static Call fill(Crossing.In in, boolean refresh) throws Exception {
        var id = new UUID(in.readLong(), in.readLong());
        String name = text(in);
        var level = level(text(in));
        ((ViewLevelData) level.getLevelData()).gameTime = in.readLong();
        boolean joined = !PLAYERS.containsKey(id);
        var player = player(id, name, level);
        player.setPose(net.minecraft.world.entity.Pose.valueOf(text(in)));
        double x = in.readDouble(), y = in.readDouble(), z = in.readDouble();
        float yRot = in.readFloat(), xRot = in.readFloat();
        // absSnapTo sets the previous tick's position, rotation and the box too: rays from pick(range, partialTick) start here
        player.absSnapTo(x, y, z, yRot, xRot);
        player.setYHeadRot(yRot);
        player.yHeadRotO = yRot;
        player.setShiftKeyDown(in.readBoolean());
        var type = GameType.byId(in.readInt());
        boolean modeChanged = player.gameMode.getGameModeForPlayer() != type;
        ((GameModeAccessor) player.gameMode).horizon$type(type);
        if (modeChanged) Menus.gameModeChanged(player, type);
        type.updatePlayerAbilities(player.getAbilities());
        var hand = in.readInt() == 0 ? InteractionHand.MAIN_HAND : InteractionHand.OFF_HAND;
        Inventory inventory = player.getInventory();
        ((dev.horizonlab.javamods.boot.mixin.InventoryAccessor) inventory).horizon$selected(in.readInt());
        int experienceLevel = in.readInt();
        float experienceProgress = in.readFloat();
        int totalExperience = in.readInt();
        PlayerSync.fill(player, in);
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
        PlayerSync.acting(player);
        // `PlayerList.placeNewPlayer`: a connected client gets its login once the player is in place.
        if (Clients.joining(player.getUUID())) safely("the client's login", () -> Menus.login(player));
        if (joined) {
            safely("PlayerLoggedIn", () -> EventHooks.firePlayerLoggedIn(player));
            safely("chunks sent before the player's view", () -> Tracking.joined(player, level));
        }
        return new Call(in, level, player, hand, before);
    }

    private static byte[] refresh(byte[] input) throws Exception {
        fill(Crossing.In.of(input), true);
        return new byte[] {1};
    }

    /** Native ended the held use: the view forgets it as `stopUsingItem` leaves it. */
    private static byte[] clearUse(byte[] input) throws Exception {
        var in = Crossing.In.of(input);
        var player = PLAYERS.get(new UUID(in.readLong(), in.readLong()));
        if (player != null) Using.clear(player);
        return new byte[] {1};
    }

    private static void use(Crossing.In in, Crossing.Out out, int kind) throws Exception {
        var call = fill(in, false);
        var level = call.level;
        var player = call.player;
        var hand = call.hand;
        var before = call.before;
        var inventory = player.getInventory();
        InteractionResult result = InteractionResult.PASS;
        // A view keeps no use between calls: Horizon's player holds it, and a call that continues one restores it.
        Using.clear(player);
        ItemStack held = player.getItemInHand(hand);
        switch (kind) {
            case 0 -> result = player.gameMode.useItem(player, level, held, hand);
            case 8 -> {
                Using.resume(player, hand, in.readInt());
                Using.tick(player);
            }
            case 9 -> {
                Using.resume(player, hand, in.readInt());
                player.releaseUsingItem();
            }
            case 10 -> {
                Using.resume(player, hand, in.readInt());
                player.stopUsingItem();
            }
            case 13 -> {
                // `ServerPlayerGameMode.destroyBlock` after its break event, in NeoForge's order.
                var pos = BlockPos.of(in.readLong());
                boolean ownMine = in.readBoolean();
                var state = level.getBlockState(pos);
                var block = state.getBlock();
                var entity = level.getBlockEntity(pos);
                var tool = player.getMainHandItem();
                var fluid = level.getFluidState(pos);
                var adjusted = block.playerWillDestroy(level, pos, state, player);
                boolean harvest = player.hasCorrectToolForDrops(adjusted, level, pos);
                var used = tool.copy();
                boolean drops = !player.preventsBlockDrops();
                if (drops && ownMine) tool.mineBlock(level, adjusted, pos, player);
                boolean removed = adjusted.onDestroyedByPlayer(level, pos, player, tool, harvest, fluid);
                if (removed) block.destroy(level, pos, adjusted);
                if (drops && removed && harvest) block.playerDestroy(level, player, pos, adjusted, entity, used);
                result = removed ? InteractionResult.SUCCESS : InteractionResult.FAIL;
            }
            case 12 -> player.getMainHandItem().mineBlock(level, Block.BLOCK_STATE_REGISTRY.byId(in.readInt()), BlockPos.of(in.readLong()), player);
            case 11 -> {
                if (level.getEntity(in.readInt()) instanceof net.minecraft.world.entity.LivingEntity target) result = held.interactLivingEntity(player, target, hand);
            }
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
                PlayerSync.sendAttributes(player);
                Menus.serverTick(player);
                EventHooks.firePlayerTickPre(player);
                for (int slot = 0; slot < Inventory.INVENTORY_SIZE; slot++) {
                    ItemStack stack = inventory.getItem(slot);
                    if (!stack.isEmpty() && BootFacts.ownsItem(stack.getItem())) {
                        stack.inventoryTick(level, player, slot == inventory.getSelectedSlot() ? EquipmentSlot.MAINHAND : null);
                    }
                }
                EventHooks.firePlayerTickPost(player);
            }
        }
        if (System.getenv("HORIZON_JAVA_DEBUG") != null) System.err.println("Horizon: use kind=" + kind + " result=" + result + " held=" + player.getItemInHand(hand));
        out.writeBoolean(result != InteractionResult.PASS && result != InteractionResult.TRY_WITH_EMPTY_HAND);
        out.writeBoolean(player.isUsingItem());
        out.writeInt(player.isUsingItem() ? player.getTicksUsingItem() : 0);
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
    }

    /** `BlockState.getDestroyProgress` for the acting player: its tool's `getDestroySpeed`, NeoForge's break speed and harvest events, effects and attributes. */
    private static byte[] destroyProgress(byte[] input) throws Exception {
        var call = fill(Crossing.In.of(input), false);
        var pos = BlockPos.of(call.in.readLong());
        PlayerSync.settle(call.player);
        var bytes = new ByteArrayOutputStream();
        new DataOutputStream(bytes).writeFloat(call.level.getBlockState(pos).getDestroyProgress(call.player, call.level, pos));
        return bytes.toByteArray();
    }

    /** `Player.hasCorrectToolForDrops(state, level, pos)` for the acting player, with NeoForge's harvest check. */
    private static byte[] correctTool(byte[] input) throws Exception {
        var call = fill(Crossing.In.of(input), false);
        var pos = BlockPos.of(call.in.readLong());
        PlayerSync.settle(call.player);
        return new byte[] {(byte) (call.player.hasCorrectToolForDrops(call.level.getBlockState(pos), call.level, pos) ? 1 : 0)};
    }

    /** A call that threw has no reply to carry its effects. */
    static boolean outermost() {
        return depth == 1;
    }

    static void abandonCall() {
        SOUNDS.clear();
    }

    private static final Sound[] NO_SOUNDS = new Sound[0];

    /** What Java did to the world since the last reply. */
    static void writeText(DataOutputStream out, String value) throws IOException {
        text(out, value);
    }

    static void writeEffects(DataOutputStream out) throws IOException {
        Sound[] sounds = SOUNDS.isEmpty() ? NO_SOUNDS : SOUNDS.toArray(NO_SOUNDS);
        SOUNDS.clear();
        out.writeInt(sounds.length);
        for (Sound s : sounds) {
            text(out, s.id);
            out.writeDouble(s.x);
            out.writeDouble(s.y);
            out.writeDouble(s.z);
            out.writeFloat(s.volume);
            out.writeFloat(s.pitch);
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
        long start = Profile.ON ? System.nanoTime() : 0;
        try {
            step.run();
        } catch (Throwable t) {
            System.err.println("Horizon: a Java mod threw during " + what);
            t.printStackTrace();
        }
        if (Profile.ON) Profile.event(what, System.nanoTime() - start);
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
            for (ServerLevel level : LEVELS.values()) {
                ChunkEvents.customSpawners(level);
                safely("LevelEvent.Load", () -> NeoForge.EVENT_BUS.post(new LevelEvent.Load(level)));
            }
            safely("ServerStarted", () -> ServerLifecycleHooks.handleServerStarted(server));
        } else if (!start && serverStarted) {
            serverStarted = false;
            if (!stopping) safely("ServerStopping", () -> ServerLifecycleHooks.handleServerStopping(server));
            stopping = false;
            for (ServerLevel level : LEVELS.values()) safely("LevelEvent.Unload", () -> NeoForge.EVENT_BUS.post(new LevelEvent.Unload(level)));
            safely("ServerStopped", () -> ServerLifecycleHooks.handleServerStopped(server));
            safely("saving saved data", () -> ServerView.saveWorld(server, LEVELS.values()));
            closeWorld();
        }
        return effects();
    }

    private static boolean stopping;

    /** `MinecraftServer.stopServer`'s first step, before the players leave and the levels close. */
    private static byte[] serverStopping(byte[] input) throws Exception {
        levelOf(new DataInputStream(new ByteArrayInputStream(input)));
        if (serverStarted && !stopping) {
            stopping = true;
            safely("ServerStopping", () -> ServerLifecycleHooks.handleServerStopping(server));
        }
        return effects();
    }

    /** `PlayerList.removeAll`: every player still in the world logs out before its levels close. */
    private static byte[] playersRemoved(byte[] input) throws Exception {
        levelOf(new DataInputStream(new ByteArrayInputStream(input)));
        for (UUID id : List.copyOf(PLAYERS.keySet())) removePlayer(id);
        return effects();
    }

    /** A server's levels, players and block entities are its own: the next world's start builds new ones. */
    private static void closeWorld() {
        LEVELS.clear();
        PLAYERS.clear();
        SOUNDS.clear();
        nextEntityId = -1;
        ServerView.closeWorld(server);
        BlockEntities.reset();
        ChunkViews.reset();
        ChunkEvents.reset();
        Clients.reset();
        Tracking.reset();
        EntityViews.reset();
    }

    private static void serverTick(Crossing.In in, Crossing.Out out) throws Exception {
        var here = levelOf(in);
        if (in.readBoolean()) {
            safely("ServerTickEvent.Post", () -> EventHooks.fireServerTickPost(() -> true, server));
        } else {
            ServerView.tick(server);
            // `MinecraftServer.tickServer`: clients hear the time once a second.
            if (server.getTickCount() % 20 == 0) server.getPlayerList().broadcastAll(new net.minecraft.network.protocol.game.ClientboundSetTimePacket(here.getGameTime(), Map.of()));
            safely("ServerTickEvent.Pre", () -> EventHooks.fireServerTickPre(() -> true, server));
        }
        writeEffects(out);
    }

    private static void levelTick(Crossing.In in, Crossing.Out out) throws Exception {
        var level = levelOf(in);
        if (in.readBoolean()) safely("LevelTickEvent.Post", () -> EventHooks.fireLevelTickPost(level, () -> true));
        else {
            safely("LevelTickEvent.Pre", () -> EventHooks.fireLevelTickPre(level, () -> true));
            BlockEntities.loadFresh();
            safely("broadcasting block changes", () -> Clients.broadcastChanges(level));
            safely("sending block entities' shapes", () -> Clients.sendShapes(level));
        }
        writeEffects(out);
    }

    /** A player connected from another process with a Java client of its own: its login reaches it over its connection. */
    private static byte[] remoteJoin(byte[] input) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        Clients.joinRemote(new UUID(in.readLong(), in.readLong()));
        return new byte[] {1};
    }

    /** The configuration frames of a client in another process, which the connection sends in its own configuration phase. */
    private static byte[] remoteConfig() throws Exception {
        var frames = Menus.remoteConfiguration(server);
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(frames.size());
        for (var frame : frames) {
            out.writeInt(frame.length);
            out.write(frame);
        }
        return bytes.toByteArray();
    }

    /** A packet a remote Java client sent, as its connection's reader hands it to the player's listener. */
    private static byte[] remoteIn(byte[] input) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        var id = new UUID(in.readLong(), in.readLong());
        var packet = Wire.fromClient(id, in.readNBytes(in.readInt()));
        if (packet != null) Clients.toServer(id, packet);
        return new byte[] {1};
    }

    private static byte[] playerLeft(byte[] input) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        var left = new UUID(in.readLong(), in.readLong());
        removePlayer(left);
        return effects();
    }

    private static void removePlayer(UUID left) throws ReflectiveOperationException {
        var player = PLAYERS.remove(left);
        if (player != null) {
            safely("closing the open menu", () -> Menus.close(player));
            safely("PlayerLoggedOut", () -> EventHooks.firePlayerLoggedOut(player));
            ServerView.detach(server, player);
        }
        if (Clients.remote(left)) Clients.leave(left);
    }

    /** The level a call acts on, with the time it arrived at. */
    private static ServerLevel levelOf(Crossing.In in) throws Exception {
        var level = level(in.name());
        ((ViewLevelData) level.getLevelData()).gameTime = in.readLong();
        level.environmentAttributes().invalidateTickCache();
        return level;
    }

    private static ServerLevel levelOf(DataInputStream in) throws Exception {
        var level = level(text(in));
        ((ViewLevelData) level.getLevelData()).gameTime = in.readLong();
        level.environmentAttributes().invalidateTickCache();
        return level;
    }

    private static void blockEvents(Crossing.In in, Crossing.Out out) throws Exception {
        var level = levelOf(in);
        for (int n = in.readInt(); n > 0; n--) {
            int kind = in.readInt();
            var pos = BlockPos.of(in.readLong());
            int a = in.readInt();
            int b = in.readInt();
            int c = in.readInt();
            try {
                BlockState state = stateOrThrow(a);
                // Horizon ran its own cascade first, so the update reads the block as it is now, as vanilla's reads it when it runs.
                BlockState now = kind == 0 ? state : level.getBlockState(pos);
                switch (kind) {
                    case 0 -> BlockEntities.blockChanged(level, pos, state, stateOrThrow(b), c);
                    case 1 -> {
                        if (BlockEntities.ours(now)) now.handleNeighborChanged(level, pos, stateOrThrow(c).getBlock(), null, b != 0);
                    }
                    case 4 -> {
                        var dir = net.minecraft.core.Direction.from3DDataValue(b);
                        if (now.is(state.getBlock())) {
                            var result = now.updateShape(level, level, pos, dir, pos.relative(dir), stateOrThrow(c), level.getRandom());
                            if (result != now) Block.updateOrDestroy(now, result, level, pos, 3 & ~32);
                        }
                    }
                    case 2 -> {
                        if (now.is(state.getBlock())) now.tick(level, pos, level.getRandom());
                    }
                    default -> {
                        if (now.is(state.getBlock()) && now.isRandomlyTicking()) now.randomTick(level, pos, level.getRandom());
                    }
                }
            } catch (Throwable t) {
                t.printStackTrace();
            }
        }
        writeEffects(out);
    }

    private static BlockState stateOrThrow(int id) {
        BlockState state = Block.BLOCK_STATE_REGISTRY.byId(id);
        if (state == null) throw new IllegalStateException("Horizon: block state " + id + " has no Java state");
        return state;
    }

    private static void blockEntityTick(Crossing.In in, Crossing.Out out) throws Exception {
        var level = levelOf(in);
        boolean held = BlockEntities.tick(level, BlockPos.of(in.readLong()));
        out.writeBoolean(held);
        writeEffects(out);
    }

    private static byte[] tracking(byte[] input) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        Tracking.apply(levelOf(in), in);
        return effects();
    }

    /** The chunks that joined and left this level, in one call: the first's block entities, then the second's going. */
    private static byte[] chunksChanged(byte[] input) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        var level = levelOf(in);
        for (int n = in.readInt(); n > 0; n--) {
            var pos = BlockPos.of(in.readLong());
            var tag = NbtIo.read(new DataInputStream(new ByteArrayInputStream(in.readNBytes(in.readInt()))), NbtAccounter.unlimitedHeap());
            try {
                // A mod's getChunk may have promoted this one already, as `LevelChunk.getBlockEntity` promotes a pending tag.
                if (BlockEntities.held(level).get(pos.asLong()) == null) BlockEntities.load(level, pos, tag);
            } catch (Throwable t) {
                t.printStackTrace();
            }
        }
        var saved = new ArrayList<CompoundTag>();
        for (int n = in.readInt(); n > 0; n--) {
            int x = in.readInt();
            int z = in.readInt();
            ChunkViews.forget(level, x, z);
            BlockEntities.unload(level, x, z, saved);
        }
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(saved.size());
        for (CompoundTag tag : saved) writeTag(out, tag);
        writeEffects(out);
        return bytes.toByteArray();
    }

    /** A chunk event of `ChunkEvents` for this level, with the effects of mods' listeners. */
    private static byte[] chunkEvents(byte[] input) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        var level = levelOf(in);
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        ChunkEvents.run(level, in, out);
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

    /** A native entity joined: its live view exists from here; the reply is the join's verdict, then the call's effects. */
    private static byte[] entityView(byte[] input) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        var level = levelOf(in);
        int id = in.readInt();
        in.readBoolean();
        byte[] tag = in.readNBytes(in.readInt());
        EntityViews.join(level, id, tag);
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeBoolean(true);
        writeEffects(out);
        return bytes.toByteArray();
    }

    /** A native entity left, for the reason (`Entity.RemovalReason` ordinal). */
    private static byte[] entityLeft(byte[] input) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        var level = levelOf(in);
        int id = in.readInt();
        EntityViews.left(level, id, in.readByte());
        return effects();
    }

    /** A bridge changed a native entity: the group bytes of what it wrote reach its view. */
    private static byte[] entityRefresh(byte[] input) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        var level = levelOf(in);
        int id = in.readInt();
        EntityViews.refresh(level, id, in.readInt(), in);
        return effects();
    }

    /** The Java-owned keys of the listed entities' views, at native's save of them. */
    private static byte[] entitiesSave(byte[] input) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        var level = level(text(in));
        var bytes = new ByteArrayOutputStream();
        EntityViews.save(level, in, new DataOutputStream(bytes));
        return bytes.toByteArray();
    }

    /** A block entity Horizon lost on its own: the object Java held goes, as `LevelChunk.removeBlockEntity` would. */
    private static byte[] blockEntityDrop(byte[] input) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        var level = levelOf(in);
        BlockEntities.drop(level, BlockPos.of(in.readLong()));
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

    /** `BlockDataAccessor.getData` of a block entity Java holds; an empty reply when it holds none. */
    private static byte[] blockEntityData(byte[] input) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        var tag = BlockEntities.data(levelOf(in), BlockPos.of(in.readLong()));
        if (tag == null) return new byte[0];
        var bytes = new ByteArrayOutputStream();
        writeTag(new DataOutputStream(bytes), tag);
        return bytes.toByteArray();
    }

    /** `BlockDataAccessor.setData`: the block entity Java holds reads the tag, or one is made from it. */
    private static byte[] blockEntitySet(byte[] input) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        var level = levelOf(in);
        var pos = BlockPos.of(in.readLong());
        var tag = NbtIo.read(new DataInputStream(new ByteArrayInputStream(in.readNBytes(in.readInt()))), NbtAccounter.unlimitedHeap());
        if (!BlockEntities.setData(level, pos, tag)) BlockEntities.load(level, pos, tag);
        return effects();
    }

    /** NeoForge's hopper past vanilla containers: `VanillaInventoryCodeHooks` with the item handler at the target, if there is one. */
    private static byte[] hopperTransfer(byte[] input) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        var level = levelOf(in);
        var hopperPos = BlockPos.of(in.readLong());
        var target = BlockPos.of(in.readLong());
        var side = net.minecraft.core.Direction.from3DDataValue(in.readInt());
        boolean insert = in.readBoolean();
        var handler = level.getCapability(net.neoforged.neoforge.capabilities.Capabilities.Item.BLOCK, target, side);
        boolean moved = false;
        if (handler != null && BlockEntities.get(level, hopperPos) instanceof net.minecraft.world.level.block.entity.HopperBlockEntity hopper) {
            moved = insert
                    ? net.neoforged.neoforge.transfer.item.VanillaInventoryCodeHooks.insertHook(hopper, handler)
                    : net.neoforged.neoforge.transfer.item.VanillaInventoryCodeHooks.extractHook(hopper, handler);
            if (moved) BlockEntities.changed(level, hopperPos);
        }
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeBoolean(handler != null);
        out.writeBoolean(moved);
        writeEffects(out);
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
