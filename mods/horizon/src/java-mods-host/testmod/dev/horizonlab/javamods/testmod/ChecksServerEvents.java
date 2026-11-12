package dev.horizonlab.javamods.testmod;

import java.util.List;
import java.util.Map;
import java.util.concurrent.CopyOnWriteArrayList;
import net.minecraft.core.BlockPos;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.item.Items;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.block.Blocks;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.player.UseItemOnBlockEvent;
import net.neoforged.neoforge.event.level.BlockEvent;
import net.neoforged.neoforge.event.level.ExplosionEvent;

/** The server census: what the test mod listens to is listened, a subclass through its supertype's listener, and Horizon's own listeners never count. */
final class ChecksServerEvents {
    private ChecksServerEvents() {}

    /** Every phase of `UseItemOnBlockEvent` the test mod's listener saw, in order. */
    static final List<UseItemOnBlockEvent.UsePhase> USE_PHASES = new CopyOnWriteArrayList<>();

    /** Listeners that do nothing: the census only reads that they are there. */
    static void listen() {
        NeoForge.EVENT_BUS.addListener((BlockEvent.EntityPlaceEvent event) -> {});
        NeoForge.EVENT_BUS.addListener((ExplosionEvent.Start event) -> {});
        NeoForge.EVENT_BUS.addListener((UseItemOnBlockEvent event) -> {
            USE_PHASES.add(event.getUsePhase());
            if (event.getUsePhase() == UseItemOnBlockEvent.UsePhase.BLOCK && event.getLevel().getBlockState(event.getPos()).is(Blocks.LEVER)) event.cancelWithResult(InteractionResult.FAIL);
            if (event.getUsePhase() == UseItemOnBlockEvent.UsePhase.ITEM_AFTER_BLOCK && event.getItemStack().is(Items.FLINT_AND_STEEL)) event.cancelWithResult(InteractionResult.SUCCESS);
        });
    }

    static void run(UseOnContext context, BlockPos area) {
        Checks.check("census_sees_base_listener", () -> census().containsKey("BlockEvent.EntityPlaceEvent") && census().containsKey("BlockEvent.EntityMultiPlaceEvent"));
        Checks.check("census_sees_direct_listener", () -> census().getOrDefault("ExplosionEvent.Start", List.of()).stream().anyMatch(owner -> owner.startsWith("dev.horizonlab.javamods.testmod.")));
        Checks.check("census_skips_horizon", () -> census().values().stream().flatMap(List::stream).noneMatch(owner -> owner.startsWith("dev.horizonlab.javamods.boot.")));
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
