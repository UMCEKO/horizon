package dev.horizonlab.javamods.boot;

import java.util.ArrayList;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtOps;
import net.minecraft.world.item.ItemStack;
import net.neoforged.neoforge.capabilities.Capabilities;
import net.neoforged.neoforge.transfer.ResourceHandler;
import net.neoforged.neoforge.transfer.item.ItemResource;
import net.neoforged.neoforge.transfer.transaction.Transaction;

/** Mods' block entities' item handlers (`Capabilities.Item.BLOCK`) for Horizon's `ITEMS`: each call asks the level anew, and writes run in transactions that follow Horizon's open ones. */
final class ItemHandlers {
    private ItemHandlers() {}

    private static final int SIZE = 0, SLOT = 1, CAPACITY = 2, VALID = 3, INSERT = 4, EXTRACT = 5;
    private static final int ANY_INDEX = -1;
    /** One per depth of Horizon's transactions a write reached, the root first. */
    private static final ArrayList<Transaction> OPEN = new ArrayList<>();

    /** `item_handler`: false when the block has no handler for the side, else true and the answer. */
    static void call(Crossing.In in, Crossing.Out out) throws Exception {
        var level = GameBridge.levelOf(in);
        var pos = BlockPos.of(in.readLong());
        int side = in.readInt();
        int ask = in.readInt();
        ResourceHandler<ItemResource> handler = level.getCapability(Capabilities.Item.BLOCK, pos, side < 0 ? null : Direction.from3DDataValue(side));
        out.writeBoolean(handler != null);
        if (handler != null) answer(handler, ask, in, out);
        GameBridge.writeEffects(out);
    }

    private static void answer(ResourceHandler<ItemResource> handler, int ask, Crossing.In in, Crossing.Out out) throws Exception {
        if (ask == SIZE) {
            out.writeInt(handler.size());
            return;
        }
        if (ask == INSERT || ask == EXTRACT) {
            int depth = in.readInt();
            int index = in.readInt();
            var resource = resource(in);
            int amount = in.readInt();
            out.writeInt(index != ANY_INDEX && !inside(handler, index) ? 0 : write(handler, ask == INSERT, index, resource, amount, at(depth)));
            return;
        }
        int index = in.readInt();
        boolean inside = inside(handler, index);
        if (ask == SLOT) {
            var held = inside ? handler.getResource(index) : ItemResource.EMPTY;
            GameBridge.writeTag(out, held.isEmpty() ? new CompoundTag() : (CompoundTag) ItemStack.CODEC.encodeStart(GameBridge.registries().createSerializationContext(NbtOps.INSTANCE), held.toStack(1)).getOrThrow());
            out.writeLong(inside ? handler.getAmountAsLong(index) : 0);
            return;
        }
        var resource = resource(in);
        if (ask == CAPACITY) out.writeLong(inside ? handler.getCapacityAsLong(index, resource) : 0);
        else if (ask == VALID) out.writeLong(inside && handler.isValid(index, resource) ? 1 : 0);
        else throw new IllegalArgumentException("item_handler: unknown ask " + ask);
    }

    private static int write(ResourceHandler<ItemResource> handler, boolean insert, int index, ItemResource resource, int amount, Transaction tx) {
        if (index == ANY_INDEX) return insert ? handler.insert(resource, amount, tx) : handler.extract(resource, amount, tx);
        return insert ? handler.insert(index, resource, amount, tx) : handler.extract(index, resource, amount, tx);
    }

    private static boolean inside(ResourceHandler<ItemResource> handler, int index) {
        return index >= 0 && index < handler.size();
    }

    private static ItemResource resource(Crossing.In in) throws Exception {
        return ItemResource.of(GameBridge.decode(in.readNBytes(in.readInt())));
    }

    /** The transaction at `depth`, opening those up to it; deeper ones Horizon left behind are aborted first. */
    private static Transaction at(int depth) {
        abortBelow(depth);
        while (OPEN.size() < depth) OPEN.add(OPEN.isEmpty() ? Transaction.openRoot() : Transaction.open(OPEN.getLast()));
        return OPEN.get(depth - 1);
    }

    private static void abortBelow(int depth) {
        while (OPEN.size() > depth) OPEN.removeLast().close();
    }

    /** `item_handler_close`: Horizon's transaction at `depth` closed, so the one following it commits or aborts the same way. */
    static void close(Crossing.In in, Crossing.Out out) throws Exception {
        GameBridge.levelOf(in);
        int depth = in.readInt();
        boolean commit = in.readBoolean();
        abortBelow(depth);
        if (OPEN.size() == depth && depth > 0) {
            var tx = OPEN.removeLast();
            if (commit) tx.commit();
            tx.close();
        }
        GameBridge.writeEffects(out);
    }
}
