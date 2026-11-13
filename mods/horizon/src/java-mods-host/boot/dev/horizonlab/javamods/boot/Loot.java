package dev.horizonlab.javamods.boot;

import com.google.gson.JsonParser;
import com.mojang.serialization.JsonOps;
import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Map;
import java.util.Optional;
import java.util.Set;
import java.util.UUID;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.component.DataComponents;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.registries.Registries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.resources.Identifier;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.RandomSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.enchantment.Enchantment;
import net.minecraft.world.item.enchantment.EnchantmentHelper;
import net.minecraft.world.item.enchantment.ItemEnchantments;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.storage.loot.LootContext;
import net.minecraft.world.level.storage.loot.LootParams;
import net.minecraft.world.level.storage.loot.functions.LootItemFunction;
import net.minecraft.world.level.storage.loot.functions.LootItemFunctions;
import net.minecraft.world.level.storage.loot.parameters.LootContextParamSets;
import net.minecraft.world.level.storage.loot.parameters.LootContextParams;
import net.minecraft.world.level.storage.loot.predicates.LootItemCondition;
import net.minecraft.world.phys.Vec3;

/** Mod loot functions and conditions in tables Horizon rolls (`loot_function`, `loot_condition`), and the tool of a native roll as Java sees its enchantments (`tool_enchantments`). */
public final class Loot {
    private Loot() {}

    /** The params a mod block's `getDrops` was asked with, while its native roll runs. */
    static final ThreadLocal<LootParams.Builder> LIVE = new ThreadLocal<>();

    private static final Map<String, Object> DECODED = new HashMap<>();
    private static final Set<String> TOLD = new HashSet<>();

    static void register() {
        Crossing.game("loot_function", Loot::function);
        Crossing.game("loot_condition", Loot::condition);
    }

    static void reset() {
        DECODED.clear();
        TOLD.clear();
        LIVE.remove();
    }

    /** Reply: the changed stack as codec JSON; empty when unchanged or when the function threw. */
    static void function(Crossing.In in, Crossing.Out out) throws IOException {
        String kind = in.text();
        String json = in.text();
        boolean live = in.readBoolean();
        var request = Request.read(in, live);
        long seed = in.readLong();
        String stackJson = in.text();
        try {
            var ops = request.level.registryAccess().createSerializationContext(JsonOps.INSTANCE);
            var function = (LootItemFunction) DECODED.computeIfAbsent("f " + json, k -> LootItemFunctions.DIRECT_CODEC.parse(ops, JsonParser.parseString(json)).getOrThrow());
            var stack = ItemStack.CODEC.parse(ops, JsonParser.parseString(stackJson)).getOrThrow();
            var result = function.apply(stack, context(request, seed));
            String reply = result.isEmpty() ? "{\"id\":\"minecraft:air\",\"count\":0}" : ItemStack.CODEC.encodeStart(ops, result).getOrThrow().toString();
            writeText(out, reply.equals(stackJson) ? "" : reply);
        } catch (Throwable t) {
            threw("function", kind, t);
            writeText(out, "");
        }
    }

    /** Reply: 1 when the condition holds, 0 when not, 2 when it threw. */
    static void condition(Crossing.In in, Crossing.Out out) throws IOException {
        String kind = in.text();
        String json = in.text();
        boolean live = in.readBoolean();
        var request = Request.read(in, live);
        long seed = in.readLong();
        try {
            var ops = request.level.registryAccess().createSerializationContext(JsonOps.INSTANCE);
            var condition = (LootItemCondition) DECODED.computeIfAbsent("c " + json, k -> LootItemCondition.DIRECT_CODEC.parse(ops, JsonParser.parseString(json)).getOrThrow());
            out.writeByte(condition.test(context(request, seed)) ? 1 : 0);
        } catch (Throwable t) {
            threw("condition", kind, t);
            out.writeByte(2);
        }
    }

    private static void writeText(Crossing.Out out, String text) throws IOException {
        byte[] bytes = text.getBytes(StandardCharsets.UTF_8);
        out.writeInt(bytes.length);
        out.write(bytes);
    }

    private static void threw(String what, String kind, Throwable t) {
        if (!TOLD.add(what + " " + kind)) return;
        boolean noGame = false;
        for (Throwable cause = t; cause != null; cause = cause.getCause()) noGame |= String.valueOf(cause.getMessage()).contains("outside a call from the game");
        if (noGame) System.err.println("Horizon: " + kind + " read the world during a loot roll; loot code runs without world access (W11 D4)");
        else System.err.println("Horizon: loot " + what + " " + kind + " threw: " + t.getMessage());
    }

    private static LootContext context(Request request, long seed) {
        var live = LIVE.get();
        var params = request.live && live != null ? live.create(LootContextParamSets.BLOCK) : request.params();
        return new LootContext.Builder(params).withOptionalRandomSource(RandomSource.create(seed)).create(Optional.empty());
    }

    /** The roll's place, as Rust writes it. */
    private record Request(boolean live, ServerLevel level, Vec3 origin, Entity thisEntity, Player killer, String tool, String block, CompoundTag blockEntity, float luck) {
        static Request read(Crossing.In in, boolean live) throws IOException {
            var level = GameBridge.levelAt(in.text(), in.readLong());
            Vec3 origin = in.readBoolean() ? new Vec3(in.readDouble(), in.readDouble(), in.readDouble()) : null;
            Entity thisEntity = null;
            if (in.readBoolean()) {
                int id = in.readInt();
                var uuid = new UUID(in.readLong(), in.readLong());
                thisEntity = GameBridge.playerView(uuid);
                if (thisEntity == null) thisEntity = EntityViews.byId(id);
            }
            Player killer = in.readBoolean() ? GameBridge.playerView(new UUID(in.readLong(), in.readLong())) : null;
            String tool = in.text();
            String block = in.text();
            int length = in.readInt();
            CompoundTag blockEntity = length == 0 ? null : NbtIo.read(new DataInputStream(new ByteArrayInputStream(in.readNBytes(length))), NbtAccounter.unlimitedHeap());
            return new Request(live, level, origin, thisEntity, killer, tool, block, blockEntity, in.readFloat());
        }

        LootParams params() {
            var builder = new LootParams.Builder(level).withLuck(luck);
            if (origin != null) builder.withParameter(LootContextParams.ORIGIN, origin);
            if (thisEntity != null) builder.withParameter(LootContextParams.THIS_ENTITY, thisEntity);
            if (killer != null) builder.withParameter(LootContextParams.LAST_DAMAGE_PLAYER, killer);
            var ops = level.registryAccess().createSerializationContext(JsonOps.INSTANCE);
            if (!tool.isEmpty()) builder.withParameter(LootContextParams.TOOL, ItemStack.CODEC.parse(ops, JsonParser.parseString(tool)).getOrThrow());
            if (!block.isEmpty()) {
                var state = BuiltInRegistries.BLOCK.getValue(Identifier.parse(block)).defaultBlockState();
                builder.withParameter(LootContextParams.BLOCK_STATE, state);
                if (blockEntity != null && origin != null) {
                    BlockEntity entity = BlockEntity.loadStatic(BlockPos.containing(origin), state, blockEntity, level.registryAccess());
                    if (entity != null) builder.withParameter(LootContextParams.BLOCK_ENTITY, entity);
                }
            }
            return builder.create(LootContextParamSets.ALL_PARAMS);
        }
    }

    /** `tool_enchantments`: the enchantments above level 0 on the tool, in registry order. */
    static void toolEnchantments(Crossing.In in, Crossing.Out out) throws IOException {
        var level = GameBridge.level(in.readInt());
        in.readLong();
        ItemStack tool = GameBridge.decode(in.readNBytes(in.readInt()));
        var found = new ArrayList<Holder.Reference<Enchantment>>();
        var levels = new ArrayList<Integer>();
        level.registryAccess().lookupOrThrow(Registries.ENCHANTMENT).listElements().forEach(holder -> {
            int at = EnchantmentHelper.getItemEnchantmentLevel(holder, tool);
            if (at > 0) {
                found.add(holder);
                levels.add(at);
            }
        });
        out.writeInt(found.size());
        for (int i = 0; i < found.size(); i++) {
            GameBridge.writeText(out, found.get(i).key().identifier().toString());
            out.writeInt(levels.get(i));
        }
    }

    /** A copy of the tool whose enchantments component holds the levels Java gives, for one roll. */
    public static ItemStack effective(ItemStack tool, RegistryAccess access) {
        var levels = new ItemEnchantments.Mutable(ItemEnchantments.EMPTY);
        access.lookupOrThrow(Registries.ENCHANTMENT).listElements().forEach(holder -> {
            int at = EnchantmentHelper.getItemEnchantmentLevel(holder, tool);
            if (at > 0) levels.set(holder, at);
        });
        ItemStack copy = tool.copy();
        copy.set(DataComponents.ENCHANTMENTS, levels.toImmutable());
        return copy;
    }

    /** Whether a roll's tool must be flattened through `effective`. */
    static boolean needsEffective(ItemStack tool) {
        return !tool.isEmpty() && (CraftFacts.enchantCode(tool.getItem()) || CraftFacts.enchantmentEventHeard());
    }
}
