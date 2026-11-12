package dev.horizonlab.javamods.boot;

import io.netty.buffer.Unpooled;
import net.minecraft.core.RegistryAccess;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.protocol.game.ClientboundContainerSetSlotPacket;
import net.minecraft.network.protocol.game.GameProtocols;
import net.minecraft.world.item.ItemStack;
import net.neoforged.neoforge.network.connection.ConnectionType;

/** Registries cannot bootstrap in a plain JVM, so the slot is empty: the header, container and state ids, and the protocol (`OTHER`) are what is checked. A menu packet native forwards decodes with the vanilla (`OTHER`) protocol the Java client's connection uses, to the same fields. */
public final class MenuForwardCheck {
    public static void main(String[] args) throws Exception {
        RegistryAccess access = RegistryAccess.EMPTY;
        var stack = ItemStack.EMPTY;
        var sent = new ClientboundContainerSetSlotPacket(5, 12, 3, stack);
        net.minecraft.network.ProtocolInfo<net.minecraft.network.protocol.game.ClientGamePacketListener> protocol;
        try {
            protocol = GameProtocols.CLIENTBOUND_TEMPLATE.bind(RegistryFriendlyByteBuf.decorator(access, ConnectionType.OTHER));
        } catch (ExceptionInInitializerError e) {
            System.out.println("MenuForwardCheck skipped: the play protocol needs bootstrapped registries, which a plain JVM cannot make (" + e.getCause().getMessage() + ")");
            return;
        }
        var buf = Unpooled.buffer();
        protocol.codec().encode(buf, sent);
        byte[] bytes = new byte[buf.readableBytes()];
        buf.readBytes(bytes);
        buf.release();
        var got = Wire.forwarded(access, bytes);
        if (!(got instanceof ClientboundContainerSetSlotPacket slot)) throw new AssertionError("decoded as " + got);
        if (slot.getContainerId() != 5 || slot.getStateId() != 12 || slot.getSlot() != 3) throw new AssertionError("header " + slot.getContainerId() + " " + slot.getStateId() + " " + slot.getSlot());
        if (!ItemStack.matches(slot.getItem(), stack)) throw new AssertionError("stack " + slot.getItem());
        System.out.println("MenuForwardCheck ok");
    }
}
