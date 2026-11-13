package dev.horizonlab.javamods.boot;

import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import net.minecraft.core.BlockPos;
import net.minecraft.world.inventory.AnvilMenu;
import net.minecraft.world.inventory.ContainerLevelAccess;
import net.minecraft.world.item.ItemStack;
import net.neoforged.neoforge.common.CommonHooks;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.AnvilUpdateEvent;
import net.neoforged.neoforge.event.GrindstoneEvent;

/** The anvil's and the grindstone's events, posted where Rust's hooks fire them (`java_station_events.rs`); a stack is its length and saved form, empty for none. */
final class StationEvents {
    private StationEvents() {}

    private interface Fields {
        void write(DataOutputStream out) throws IOException;
    }

    private static EventPosts.Posted answer(boolean canceled, Fields fields) throws IOException {
        var bytes = new ByteArrayOutputStream();
        fields.write(new DataOutputStream(bytes));
        return new EventPosts.Posted(canceled, bytes.toByteArray());
    }

    private static ItemStack stack(Crossing.In in) throws IOException {
        int length = in.readInt();
        return length == 0 ? ItemStack.EMPTY : GameBridge.decode(in.readNBytes(length));
    }

    private static void stack(DataOutputStream out, ItemStack stack) throws IOException {
        byte[] bytes = Effects.stack(stack);
        out.writeInt(bytes.length);
        out.write(bytes);
    }

    /** The inputs, the rename text, vanilla's result and the answer so far; the reply is the output and the costs. */
    static EventPosts.Posted anvilUpdate(GameBridge.Call call) throws IOException {
        var in = call.in();
        var left = stack(in);
        var right = stack(in);
        boolean named = in.readBoolean();
        String text = in.text();
        var vanilla = stack(in);
        int vanillaCost = in.readInt();
        int vanillaMaterial = in.readInt();
        var output = stack(in);
        int cost = in.readInt();
        int material = in.readInt();
        var event = new AnvilUpdateEvent(left, right, named ? text : null, vanilla, vanillaCost, vanillaMaterial, call.player());
        event.setOutput(output);
        event.setXpCost(cost);
        event.setMaterialCost(material);
        boolean canceled = NeoForge.EVENT_BUS.post(event).isCanceled();
        return answer(canceled, out -> {
            stack(out, event.getOutput());
            out.writeInt(event.getXpCost());
            out.writeInt(event.getMaterialCost());
        });
    }

    /** A menu to name in the event: the take itself stays native, so nothing is read from it. */
    private static AnvilMenu menu(GameBridge.Call call, long pos) {
        return new AnvilMenu(0, call.player().getInventory(), ContainerLevelAccess.create(call.level(), BlockPos.of(pos)));
    }

    static EventPosts.Posted anvilCraftPre(GameBridge.Call call) throws IOException {
        var in = call.in();
        var menu = menu(call, in.readLong());
        var left = stack(in);
        var right = stack(in);
        var output = stack(in);
        return new EventPosts.Posted(CommonHooks.fireAnvilCraftPre(menu, call.player(), left, right, output).isCanceled(), EventPosts.Posted.NONE.result());
    }

    static EventPosts.Posted anvilCraftPost(GameBridge.Call call) throws IOException {
        var in = call.in();
        var menu = menu(call, in.readLong());
        var left = stack(in);
        var right = stack(in);
        var output = stack(in);
        CommonHooks.fireAnvilCraftPost(menu, call.player(), left, right, output);
        return EventPosts.Posted.NONE;
    }

    /** The inputs and the experience (-1 for vanilla's); the reply is the output a listener set and the experience. */
    static EventPosts.Posted grindstonePlace(GameBridge.Call call) throws IOException {
        var in = call.in();
        var event = new GrindstoneEvent.OnPlaceItem(stack(in), stack(in), in.readInt());
        boolean canceled = NeoForge.EVENT_BUS.post(event).isCanceled();
        return answer(canceled, out -> {
            stack(out, event.getOutput());
            out.writeInt(event.getXp());
        });
    }

    /** The position, the inputs, the experience (-1 for vanilla's) and what the inputs hold afterwards; the reply is the experience and those. */
    static EventPosts.Posted grindstoneTake(GameBridge.Call call) throws IOException {
        var in = call.in();
        var access = ContainerLevelAccess.create(call.level(), BlockPos.of(in.readLong()));
        var event = new GrindstoneEvent.OnTakeItem(access, call.player(), stack(in), stack(in), in.readInt());
        event.setNewTopItem(stack(in));
        event.setNewBottomItem(stack(in));
        boolean canceled = NeoForge.EVENT_BUS.post(event).isCanceled();
        return answer(canceled, out -> {
            out.writeInt(event.getXp());
            stack(out, event.getNewTopItem());
            stack(out, event.getNewBottomItem());
        });
    }
}
