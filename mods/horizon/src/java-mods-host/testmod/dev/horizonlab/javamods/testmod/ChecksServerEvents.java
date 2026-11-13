package dev.horizonlab.javamods.testmod;

import com.mojang.serialization.Codec;
import java.util.List;
import java.util.Map;
import java.util.function.Supplier;
import net.minecraft.core.BlockPos;
import net.minecraft.server.level.ServerPlayer;
import net.minecraft.world.level.Level;
import net.minecraft.world.item.context.UseOnContext;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.attachment.AttachmentType;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.entity.EntityTravelToDimensionEvent;
import net.neoforged.neoforge.event.entity.player.PlayerEvent;
import net.neoforged.neoforge.event.level.BlockEvent;
import net.neoforged.neoforge.event.level.ExplosionEvent;
import net.neoforged.neoforge.registries.DeferredRegister;
import net.neoforged.neoforge.registries.NeoForgeRegistries;

/** The server census: what the test mod listens to is listened, a subclass through its supertype's listener, and Horizon's own listeners never count. */
final class ChecksServerEvents {
    private ChecksServerEvents() {}

    private static final DeferredRegister<AttachmentType<?>> TYPES = DeferredRegister.create(NeoForgeRegistries.Keys.ATTACHMENT_TYPES, "horizon_java_test");
    /** Copied to the new player by NeoForge's own Clone listener, as `copyOnDeath` documents. */
    static final Supplier<AttachmentType<Integer>> DEATH_DATA = TYPES.register("death_data", () -> AttachmentType.builder(() -> 0).serialize(Codec.INT.fieldOf("death_data")).copyOnDeath().build());
    private static int clones, respawns, toNether, toOverworld;

    /** Listeners that do nothing but the census read: and the respawn and dimension ones, which print their checks as the events come. */
    static void listen(IEventBus bus) {
        TYPES.register(bus);
        NeoForge.EVENT_BUS.addListener((PlayerEvent.Clone event) -> {
            if (!(event.getEntity() instanceof ServerPlayer player) || !event.isWasDeath()) return;
            boolean two = event.getOriginal() != player;
            Checks.check("clone_has_two_objects", () -> ++clones == 1 && two);
            Checks.check("clone_copies_death_data", () -> player.getData(DEATH_DATA) == event.getOriginal().getData(DEATH_DATA));
        });
        NeoForge.EVENT_BUS.addListener((PlayerEvent.PlayerRespawnEvent event) -> Checks.check("respawn_not_end_conquered", () -> ++respawns == 1 && !event.isEndConquered()));
        NeoForge.EVENT_BUS.addListener((EntityTravelToDimensionEvent event) -> {
            if (event.getEntity() instanceof ServerPlayer && event.getDimension() == Level.END) {
                event.setCanceled(true);
                Checks.check("travel_cancel_keeps_level", () -> event.getEntity().level().dimension() != Level.END);
            }
        });
        NeoForge.EVENT_BUS.addListener((PlayerEvent.PlayerChangedDimensionEvent event) -> {
            if (event.getFrom() == Level.OVERWORLD && event.getTo() == Level.NETHER) Checks.check("changed_dimension_once", () -> ++toNether == 1);
            else if (event.getFrom() == Level.NETHER && event.getTo() == Level.OVERWORLD) Checks.check("changed_dimension_back_once", () -> ++toOverworld == 1);
        });
        NeoForge.EVENT_BUS.addListener((BlockEvent.EntityPlaceEvent event) -> {});
        NeoForge.EVENT_BUS.addListener((ExplosionEvent.Start event) -> {});
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
