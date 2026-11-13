package dev.horizonlab.javamods.testmod;

import net.minecraft.client.gui.components.debug.DebugEntryCategory;
import net.minecraft.client.gui.components.debug.DebugScreenEntry;
import net.minecraft.client.gui.components.debug.DebugScreenEntryStatus;
import net.minecraft.client.gui.components.debug.DebugScreenProfile;
import net.minecraft.resources.Identifier;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.client.event.RegisterDebugEntriesEvent;

/** An F3 entry in the default profile: native lists it, enables it with the overlay, and draws what its `display` adds. */
final class ClientDebug {
    private ClientDebug() {}

    static final Identifier PROBE = Identifier.parse("horizon_java_test:probe");
    private static final Identifier GROUP = Identifier.parse("horizon_java_test:g");
    private static boolean checked;

    static void register(IEventBus bus) {
        bus.addListener((RegisterDebugEntriesEvent event) -> {
            event.register(PROBE, new DebugScreenEntry() {
                @Override
                public void display(net.minecraft.client.gui.components.debug.DebugScreenDisplayer displayer, net.minecraft.world.level.Level level,
                        net.minecraft.world.level.chunk.LevelChunk clientChunk, net.minecraft.world.level.chunk.LevelChunk serverChunk) {
                    displayer.addLine("probe");
                    displayer.addToGroup(GROUP, "g1");
                    if (checked || level == null) return;
                    checked = true;
                    Checks.check("debug_entry_lines_recorded", () -> clientChunk != null);
                }

                @Override
                public DebugEntryCategory category() {
                    return DebugEntryCategory.SCREEN_TEXT;
                }
            });
            event.includeInProfile(PROBE, DebugScreenProfile.DEFAULT, DebugScreenEntryStatus.IN_OVERLAY);
        });
    }
}
