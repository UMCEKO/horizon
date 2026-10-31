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
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.UUID;
import net.minecraft.commands.Commands;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
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
import net.minecraft.server.WorldLoader;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.server.level.ServerPlayerGameMode;
import net.minecraft.server.packs.PackType;
import net.minecraft.server.packs.repository.ServerPacksSource;
import net.minecraft.server.permissions.LevelBasedPermissionSet;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.util.ProblemReporter;
import net.minecraft.util.Unit;
import net.minecraft.util.Util;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.Entity;
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
import net.minecraft.world.level.storage.TagValueOutput;
import net.minecraft.world.level.storage.WritableLevelData;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.resource.ResourcePackLoader;
import sun.reflect.ReflectionFactory;

/** The game's own objects as views of Horizon's state: built without a server, filled for each call, and what the call changed goes back. */
public final class GameBridge {
    private GameBridge() {}

    private record Sound(String id, double x, double y, double z, float volume, float pitch) {}

    private static final int SLOTS = 41;
    private static final Map<String, ServerLevel> LEVELS = new HashMap<>();
    private static final Map<UUID, ServerPlayer> PLAYERS = new HashMap<>();
    private static final List<Sound> SOUNDS = new ArrayList<>();
    private static final List<CompoundTag> SPAWNED = new ArrayList<>();
    private static RegistryAccess.Frozen registries;
    private static int nextEntityId = -1;

    public static byte[] call(String op, byte[] input) {
        try {
            return switch (op) {
                case "prepare" -> prepare();
                case "items" -> items();
                case "states" -> states();
                case "use_item" -> use(input, false);
                case "use_item_on" -> use(input, true);
                default -> throw new IllegalArgumentException(op);
            };
        } catch (Throwable t) {
            t.printStackTrace();
            return new byte[0];
        }
    }

    private static byte[] prepare() throws Exception {
        if (registries != null) return new byte[] {1};
        var repository = ServerPacksSource.createVanillaTrustedRepository();
        ResourcePackLoader.populatePackRepository(repository, PackType.SERVER_DATA, true);
        var packs = new WorldLoader.PackConfig(repository, WorldDataConfiguration.DEFAULT, false, true);
        var config = new WorldLoader.InitConfig(packs, Commands.CommandSelection.DEDICATED, LevelBasedPermissionSet.GAMEMASTER);
        registries = Util.blockUntilDone(executor -> WorldLoader.load(
                config,
                context -> new WorldLoader.DataLoadOutput<>(Unit.INSTANCE, context.datapackDimensions()),
                (manager, loaded, layers, unit) -> layers.compositeAccess(),
                Util.backgroundExecutor(),
                executor)).get();
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

    private static <T> T construct(Class<T> type, Constructor<?> parent, Object... args) throws ReflectiveOperationException {
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
        LEVELS.put(id, level);
        return level;
    }

    private static ServerPlayer player(UUID id, String name, ServerLevel level) throws ReflectiveOperationException {
        var player = PLAYERS.get(id);
        if (player == null) {
            player = construct(ServerPlayer.class, Player.class.getConstructor(Level.class, GameProfile.class), level, new GameProfile(id, name));
            ((ServerPlayerAccessor) player).horizon$gameMode(new ServerPlayerGameMode(player));
            PLAYERS.put(id, player);
        } else if (player.level() != level) {
            player.setServerLevel(level);
        }
        return player;
    }

    /** Ids for views of entities, which Horizon gives its own when they reach it; negative so they never meet Horizon's. */
    public static int nextEntityId() {
        return nextEntityId--;
    }

    public static void sound(SoundEvent sound, double x, double y, double z, float volume, float pitch) {
        SOUNDS.add(new Sound(sound.location().toString(), x, y, z, volume, pitch));
    }

    public static boolean spawn(Entity entity) {
        var output = TagValueOutput.createWithContext(ProblemReporter.DISCARDING, entity.level().registryAccess());
        if (!entity.save(output)) return false;
        SPAWNED.add(output.buildResult());
        return true;
    }

    private static ItemStack decode(byte[] nbt) throws IOException {
        Tag tag = NbtIo.read(new DataInputStream(new ByteArrayInputStream(nbt)), NbtAccounter.unlimitedHeap());
        return ItemStack.CODEC.parse(registries.createSerializationContext(NbtOps.INSTANCE), tag).getOrThrow();
    }

    private static void writeTag(DataOutputStream out, CompoundTag tag) throws IOException {
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
        return use(input, false);
    }

    private static byte[] use(byte[] input, boolean onBlock) throws Exception {
        var in = new DataInputStream(new ByteArrayInputStream(input));
        var id = new UUID(in.readLong(), in.readLong());
        String name = text(in);
        var level = level(text(in));
        ((ViewLevelData) level.getLevelData()).gameTime = in.readLong();
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
        player.experienceLevel = in.readInt();
        player.experienceProgress = in.readFloat();
        player.totalExperience = in.readInt();
        inventory.clearContent();
        var before = new ItemStack[SLOTS];
        java.util.Arrays.fill(before, ItemStack.EMPTY);
        for (int n = in.readInt(); n > 0; n--) {
            int slot = in.readInt();
            ItemStack stack = decode(in.readNBytes(in.readInt()));
            before[slot] = stack.copy();
            inventory.setItem(slot, stack);
        }
        SOUNDS.clear();
        SPAWNED.clear();
        InteractionResult result;
        if (onBlock) {
            var pos = BlockPos.of(in.readLong());
            var face = Direction.from3DDataValue(in.readInt());
            var hit = new BlockHitResult(new Vec3(in.readDouble(), in.readDouble(), in.readDouble()), face, pos, false);
            result = player.getItemInHand(hand).useOn(new UseOnContext(player, hand, hit));
        } else {
            result = player.gameMode.useItem(player, level, player.getItemInHand(hand), hand);
        }

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
        out.writeInt(SOUNDS.size());
        for (Sound s : SOUNDS) {
            text(out, s.id);
            out.writeDouble(s.x);
            out.writeDouble(s.y);
            out.writeDouble(s.z);
            out.writeFloat(s.volume);
            out.writeFloat(s.pitch);
        }
        out.writeInt(SPAWNED.size());
        for (CompoundTag tag : SPAWNED) writeTag(out, tag);
        return bytes.toByteArray();
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
        sb.append(" sounds=").append(SOUNDS).append(" spawned=").append(SPAWNED).append(" took_us=").append(took / 1000);
        return sb.toString();
    }
}
