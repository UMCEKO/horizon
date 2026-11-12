package dev.horizonlab.javamods.testmod;

import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.level.LevelEvent;
import net.neoforged.neoforge.event.server.ServerAboutToStartEvent;

/** The first world start's order as this mod's listeners saw it: `ServerAboutToStart`, then `LevelEvent.Load`. */
final class WorldStart {
    private WorldStart() {}

    private static int seen;
    static int aboutToStart = -1;
    static int levelLoad = -1;
    /** Worldgen exports Java had started when `ServerAboutToStart` first fired. */
    static int exportsBefore = -1;

    static void register() {
        NeoForge.EVENT_BUS.addListener((ServerAboutToStartEvent event) -> {
            if (aboutToStart >= 0) return;
            aboutToStart = seen++;
            exportsBefore = exports();
        });
        NeoForge.EVENT_BUS.addListener((LevelEvent.Load event) -> {
            if (levelLoad < 0) levelLoad = seen++;
        });
    }

    private static int exports() {
        try {
            var export = Class.forName("dev.horizonlab.javamods.boot.WorldgenExport", true, WorldStart.class.getClassLoader());
            return (int) export.getMethod("writes").invoke(null);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }

    /** `ServerAboutToStart` came first: before any level loaded and before any worldgen export. */
    static boolean inOrder() {
        return aboutToStart >= 0 && aboutToStart < levelLoad && exportsBefore == 0;
    }
}
