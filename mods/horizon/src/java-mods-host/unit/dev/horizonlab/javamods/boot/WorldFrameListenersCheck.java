package dev.horizonlab.javamods.boot;

import net.neoforged.bus.api.BusBuilder;
import net.neoforged.neoforge.client.event.RenderLevelStageEvent;

/** With no listener of a level-frame event nothing records: `record` is never called. A listener, even of a stage event, makes the world recorded. */
public final class WorldFrameListenersCheck {
    public static void main(String[] args) {
        var bus = BusBuilder.builder().build();
        bus.start();
        if (WorldFrame.listened(bus)) throw new AssertionError("an empty bus");
        WorldFrame.needed = WorldFrame.listened(bus);
        for (int tick = 0; tick < 5; tick++) {
            if (WorldFrame.take(new WorldFrame.Cam(0, 0, 0, 0, 0, 70, 512), 0, null, 854, 480, tick % 2 == 0) != null) throw new AssertionError("a recording");
        }
        if (WorldFrame.records != 0) throw new AssertionError("record ran " + WorldFrame.records + " times");
        bus.addListener(RenderLevelStageEvent.AfterLevel.class, event -> {});
        if (!WorldFrame.listened(bus)) throw new AssertionError("a listener of the stage events");
        System.out.println("WorldFrameListenersCheck ok");
    }
}
