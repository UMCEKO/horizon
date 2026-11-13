package dev.horizonlab.javamods.boot;

import com.google.gson.JsonParser;
import com.mojang.serialization.JsonOps;
import dev.horizonlab.javamods.boot.mixin.LegacySeedAccess;
import it.unimi.dsi.fastutil.objects.ObjectArrayList;
import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.HashSet;
import java.util.Optional;
import java.util.Set;
import java.util.UUID;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.LongArrayTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.NbtOps;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.util.RandomSource;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.levelgen.LegacyRandomSource;
import net.minecraft.world.level.levelgen.XoroshiroRandomSource;
import net.minecraft.world.level.storage.loot.LootContext;
import net.minecraft.world.level.storage.loot.LootParams;
import net.minecraft.world.level.storage.loot.parameters.LootContextParamSets;
import net.minecraft.world.level.storage.loot.parameters.LootContextParams;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.common.CommonHooks;
import net.neoforged.neoforge.common.loot.LootTableIdCondition;

/** `loot_modify`: NeoForge's global loot modifiers on a native roll's stacks, with a random that continues native's. */
public final class LootModifiers {
    private LootModifiers() {}

    private static final boolean DEBUG = System.getenv("HORIZON_JAVA_DEBUG") != null;
    private static final byte LEGACY = 1, XOROSHIRO = 2;
    private static final String NO_GAME = ": outside a call from the game";
    private static final Set<String> UNRESOLVED = new HashSet<>();
    private static final Set<String> ISOLATED = new HashSet<>();

    /** The request as `java_loot_modify.rs` writes it; an empty reply leaves the roll as it was. */
    static void modify(Crossing.In in, Crossing.Out out) throws IOException {
        String table = in.text();
        ServerLevel level = level(in.text());
        var registries = level.registryAccess();
        var ops = registries.createSerializationContext(JsonOps.INSTANCE);
        byte kind = in.readByte();
        RandomSource random = switch (kind) {
            case LEGACY -> legacy(in.readLong());
            case XOROSHIRO -> new XoroshiroRandomSource(in.readLong(), in.readLong());
            default -> throw new IllegalArgumentException("loot_modify: random kind " + kind);
        };
        var params = new LootParams.Builder(level);
        Vec3 origin = in.readBoolean() ? new Vec3(in.readDouble(), in.readDouble(), in.readDouble()) : null;
        if (origin != null) params.withParameter(LootContextParams.ORIGIN, origin);
        int thisId = in.readInt();
        UUID thisUuid = new UUID(in.readLong(), in.readLong());
        if (thisId != -1) {
            Entity entity = entity(thisId, thisUuid);
            if (entity != null) params.withParameter(LootContextParams.THIS_ENTITY, entity);
            else unresolved("this_entity");
        }
        boolean killed = in.readBoolean();
        UUID killer = new UUID(in.readLong(), in.readLong());
        if (killed) {
            ServerPlayer player = player(killer);
            if (player != null) params.withParameter(LootContextParams.LAST_DAMAGE_PLAYER, player);
            else unresolved("last_damage_player");
        }
        if (in.readBoolean()) {
            var type = registries.lookupOrThrow(Registries.DAMAGE_TYPE).get(ResourceKey.create(Registries.DAMAGE_TYPE, Identifier.parse(in.text())));
            if (type.isPresent()) params.withParameter(LootContextParams.DAMAGE_SOURCE, new DamageSource(type.get()));
            else unresolved("damage_source");
        }
        String tool = in.text();
        if (!tool.isEmpty()) {
            var parsed = ItemStack.CODEC.parse(ops, JsonParser.parseString(tool)).result();
            if (parsed.isPresent()) params.withParameter(LootContextParams.TOOL, parsed.get());
            else unresolved("tool");
        }
        int stateId = in.readInt();
        BlockState state = stateId >= 0 ? Block.stateById(stateId) : null;
        if (state != null) params.withParameter(LootContextParams.BLOCK_STATE, state);
        byte[] blockEntity = in.readNBytes(in.readInt());
        if (blockEntity.length > 0) {
            var tag = NbtIo.read(new DataInputStream(new ByteArrayInputStream(blockEntity)), NbtAccounter.unlimitedHeap());
            BlockEntity loaded = state == null || origin == null ? null : BlockEntity.loadStatic(BlockPos.containing(origin), state, tag, registries);
            if (loaded != null) params.withParameter(LootContextParams.BLOCK_ENTITY, loaded);
            else unresolved("block_entity");
        }
        if (in.readBoolean()) params.withParameter(LootContextParams.EXPLOSION_RADIUS, in.readFloat());
        params.withLuck(in.readFloat());
        int n = in.readInt();
        var stacks = new ObjectArrayList<ItemStack>(n);
        for (int i = 0; i < n; i++) {
            String json = in.text();
            var parsed = ItemStack.CODEC.parse(ops, JsonParser.parseString(json));
            if (parsed.result().isPresent()) stacks.add(parsed.result().get());
            else System.err.println("Horizon: loot_modify could not read the stack " + json);
        }
        var context = new LootContext.Builder(params.create(LootContextParamSets.ALL_PARAMS)).withOptionalRandomSource(random).create(Optional.empty());
        Identifier id = table.isEmpty() ? LootTableIdCondition.UNKNOWN_LOOT_TABLE : Identifier.parse(table);
        ObjectArrayList<ItemStack> result;
        try {
            result = CommonHooks.modifyLoot(id, stacks, context);
        } catch (Throwable t) {
            failed(table, t);
            return;
        }
        if (random instanceof LegacyRandomSource legacy) {
            out.writeByte(LEGACY);
            out.writeLong(((LegacySeedAccess) legacy).horizon$seed().get());
        } else {
            long[] words = ((LongArrayTag) XoroshiroRandomSource.CODEC.encodeStart(NbtOps.INSTANCE, (XoroshiroRandomSource) random).getOrThrow()).getAsLongArray();
            out.writeByte(XOROSHIRO);
            out.writeLong(words[0]);
            out.writeLong(words[1]);
        }
        var written = new ObjectArrayList<String>(result.size());
        for (ItemStack stack : result) {
            if (!stack.isEmpty()) written.add(ItemStack.CODEC.encodeStart(ops, stack).getOrThrow().toString());
        }
        out.writeInt(written.size());
        for (String json : written) {
            byte[] bytes = json.getBytes(StandardCharsets.UTF_8);
            out.writeInt(bytes.length);
            out.write(bytes);
        }
        if (DEBUG) System.err.println("Horizon: loot_modify " + table + " " + n + " -> " + written.size());
    }

    /** A request by bytes and its reply, for checks that build one by hand. */
    public static byte[] call(byte[] request) throws IOException {
        var out = new Crossing.Out();
        modify(Crossing.In.of(request), out);
        return out.toByteArray();
    }

    private static RandomSource legacy(long state) {
        var random = new LegacyRandomSource(0);
        // setSeed scrambles with the multiplier, so this leaves the raw state as it was.
        random.setSeed(state ^ 0x5DEECE66DL);
        return random;
    }

    private static ServerLevel level(String dimension) {
        try {
            return GameBridge.levelNamed(dimension);
        } catch (IllegalStateException e) {
            return GameBridge.level(GameBridge.OVERWORLD);
        }
    }

    private static Entity entity(int id, UUID uuid) {
        ServerPlayer player = player(uuid);
        return player != null ? player : EntityViews.byId(id);
    }

    private static ServerPlayer player(UUID uuid) {
        ServerPlayer view = GameBridge.playerView(uuid);
        if (view != null) return view;
        var server = GameBridge.server();
        return server == null ? null : server.getPlayerList().getPlayer(uuid);
    }

    private static void unresolved(String parameter) {
        if (DEBUG && UNRESOLVED.add(parameter)) System.err.println("Horizon: loot_modify left out " + parameter + ": no view answers it");
    }

    private static void failed(String table, Throwable t) {
        for (Throwable cause = t; cause != null; cause = cause.getCause()) {
            String message = cause.getMessage();
            if (cause instanceof IllegalStateException && message != null && message.endsWith(NO_GAME)) {
                String kind = modifierOf(cause);
                if (ISOLATED.add(kind)) System.err.println("Horizon: " + kind + " read the world during a loot roll; loot code runs without world access (W11 D4)");
                return;
            }
        }
        System.err.println("Horizon: loot modifiers threw on " + table + ": " + t);
        if (DEBUG) t.printStackTrace();
    }

    /** The modifier class a throw came from: the innermost `doApply` or `apply` frame outside NeoForge's own loot code. */
    private static String modifierOf(Throwable t) {
        for (var frame : t.getStackTrace()) {
            String method = frame.getMethodName();
            if ((method.equals("doApply") || method.equals("apply")) && !frame.getClassName().startsWith("net.neoforged.neoforge.common.loot.")) return frame.getClassName();
        }
        return "a loot modifier";
    }
}
