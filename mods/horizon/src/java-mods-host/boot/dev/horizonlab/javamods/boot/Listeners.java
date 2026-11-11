package dev.horizonlab.javamods.boot;

import java.util.List;
import java.util.Set;
import net.neoforged.bus.api.Event;
import net.neoforged.bus.api.IEventBus;

/** Whether a mod listens to some of NeoForge's events, read from the bus's own listener lists. */
final class Listeners {
    private Listeners() {}

    private static java.lang.reflect.Method list;

    private static java.lang.reflect.Method listMethod() throws ReflectiveOperationException {
        if (list == null) {
            var method = net.neoforged.bus.EventBus.class.getDeclaredMethod("getListenerList", Class.class);
            method.setAccessible(true);
            list = method;
        }
        return list;
    }

    static Class<?> ownerOf(net.neoforged.bus.api.EventListener listener) throws ReflectiveOperationException {
        if (listener instanceof net.neoforged.bus.IWrapperListener wrapper) listener = wrapper.getWithoutCheck();
        if (listener instanceof net.neoforged.bus.ConsumerEventHandler) {
            var consumer = net.neoforged.bus.ConsumerEventHandler.class.getDeclaredField("consumer");
            consumer.setAccessible(true);
            return consumer.get(listener).getClass();
        }
        return listener.getClass();
    }

    /** Whether a mod's class listens to one of `events`; logs `Horizon: <what> heard by <owner>` once per owner in `heard`. */
    static boolean modListens(IEventBus bus, List<Class<? extends Event>> events, Set<String> heard, String what) {
        try {
            var list = listMethod();
            boolean found = false;
            for (var event : events) {
                for (var listener : ((net.neoforged.bus.ListenerList) list.invoke(bus, event)).getListeners()) {
                    Class<?> owner = ownerOf(listener);
                    if (!Shells.isMods(owner)) continue;
                    found = true;
                    if (heard.add(owner.getName())) System.err.println("Horizon: " + what + " heard by " + owner.getName());
                }
            }
            return found;
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException(e);
        }
    }
}
