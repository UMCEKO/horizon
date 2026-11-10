package dev.horizonlab.javamods.testmod;

import com.mojang.serialization.Codec;
import java.util.function.Supplier;
import net.minecraft.server.level.ServerLevel;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.attachment.AttachmentType;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.level.ChunkDataEvent;
import net.neoforged.neoforge.event.level.ChunkEvent;
import net.neoforged.neoforge.registries.DeferredRegister;
import net.neoforged.neoforge.registries.NeoForgeRegistries;

/** A chunk attachment counting each chunk's loads, as a mod keeps its own per-chunk data: what a save wrote must come back on the next load. */
final class ChunkData {
    private ChunkData() {}

    private static final DeferredRegister<AttachmentType<?>> TYPES = DeferredRegister.create(NeoForgeRegistries.Keys.ATTACHMENT_TYPES, "horizon_java_test");
    static final Supplier<AttachmentType<Integer>> LOADS = TYPES.register("loads", () -> AttachmentType.builder(() -> 0).serialize(Codec.INT.fieldOf("loads")).build());
    private static int read, carried, maxLoads, saves;

    static void register(IEventBus bus) {
        TYPES.register(bus);
        NeoForge.EVENT_BUS.addListener(ChunkEvent.Load.class, event -> {
            if (!(event.getLevel() instanceof ServerLevel)) return;
            var chunk = event.getChunk();
            chunk.setData(LOADS, chunk.getData(LOADS) + 1);
            chunk.markUnsaved();
        });
        NeoForge.EVENT_BUS.addListener(ChunkDataEvent.Load.class, event -> {
            read++;
            int loads = event.getChunk().getData(LOADS);
            if (loads > 0) carried++;
            maxLoads = Math.max(maxLoads, loads);
            if (read % 100 == 0) System.out.println("Horizon test: " + read + " chunks read, " + carried + " brought back their load count, the highest " + maxLoads);
        });
        NeoForge.EVENT_BUS.addListener(ChunkDataEvent.Save.class, event -> {
            if (++saves % 100 == 0) System.out.println("Horizon test: " + saves + " chunks saved, the last with attachments " + event.getData().attachmentData());
        });
    }
}
