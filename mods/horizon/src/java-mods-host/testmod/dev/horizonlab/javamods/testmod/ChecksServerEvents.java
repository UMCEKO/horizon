package dev.horizonlab.javamods.testmod;

import java.util.List;
import java.util.Map;
import net.minecraft.core.BlockPos;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.block.Blocks;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.level.BlockEvent;
import net.neoforged.neoforge.event.level.ExplosionEvent;

/** The server census: what the test mod listens to is listened, a subclass through its supertype's listener, and Horizon's own listeners never count. */
final class ChecksServerEvents {
    private ChecksServerEvents() {}

    /** The census reads that these are there; the place listener also refuses diamond blocks and checks a door's event. */
    static void listen() {
        NeoForge.EVENT_BUS.addListener(ChecksServerEvents::placed);
        NeoForge.EVENT_BUS.addListener((ExplosionEvent.Start event) -> {});
    }

    static void run(UseOnContext context, BlockPos area) {
        Checks.check("census_sees_base_listener", () -> census().containsKey("BlockEvent.EntityPlaceEvent") && census().containsKey("BlockEvent.EntityMultiPlaceEvent"));
        Checks.check("census_sees_direct_listener", () -> census().getOrDefault("ExplosionEvent.Start", List.of()).stream().anyMatch(owner -> owner.startsWith("dev.horizonlab.javamods.testmod.")));
        Checks.check("census_skips_horizon", () -> census().values().stream().flatMap(List::stream).noneMatch(owner -> owner.startsWith("dev.horizonlab.javamods.boot.")));
    }

    /** Driven by `java_mods.py bridges`, which places a diamond block (refused here) and an oak door. */
    private static void placed(BlockEvent.EntityPlaceEvent event) {
        if (event.getLevel().isClientSide()) return;
        int count = event instanceof BlockEvent.EntityMultiPlaceEvent multi ? multi.getReplacedBlockSnapshots().size() : 1;
        System.out.println("Horizon test: " + event.getClass().getSimpleName() + " of " + event.getPlacedBlock().getBlock() + " with " + count + " snapshots");
        if (event.getPlacedBlock().is(Blocks.DIAMOND_BLOCK)) event.setCanceled(true);
        if (event.getPlacedBlock().is(Blocks.OAK_DOOR)) Checks.check("door_is_multi_place", () -> event instanceof BlockEvent.EntityMultiPlaceEvent && count == 2);
    }

    @SuppressWarnings("unchecked")
    private static Map<String, List<String>> census() {
        try {
            var type = Class.forName("dev.horizonlab.javamods.boot.EventPosts", true, ClassLoader.getSystemClassLoader());
            return (Map<String, List<String>>) type.getMethod("lastCensus").invoke(null);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }
}
