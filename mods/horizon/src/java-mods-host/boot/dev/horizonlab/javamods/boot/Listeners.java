package dev.horizonlab.javamods.boot;

import net.neoforged.bus.EventBus;
import net.neoforged.bus.ListenerList;
import net.neoforged.bus.api.IEventBus;

/** What a bus has listening. */
final class Listeners {
    private Listeners() {}

    /** Whether the bus has a listener of the event. */
    static boolean listened(IEventBus bus, Class<?> event) {
        try {
            var list = EventBus.class.getDeclaredMethod("getListenerList", Class.class);
            list.setAccessible(true);
            return ((ListenerList) list.invoke(bus, event)).getListeners().length > 0;
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }
}
