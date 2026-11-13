package dev.horizonlab.javamods.boot;

import java.util.ArrayList;
import java.util.List;
import net.neoforged.bus.EventBus;
import net.neoforged.bus.api.BusBuilder;
import net.neoforged.bus.api.Event;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.ICancellableEvent;
import net.neoforged.bus.api.SubscribeEvent;

/** Compiled dispatch against the bus's own `post` on the same listeners: two buses built alike, the second compiled (run with the host jar as `-javaagent`). */
public final class CompiledEventsCheck {
    public abstract static class Base extends Event {
        public final List<String> log = new ArrayList<>();
    }

    public static final class Leaf extends Base implements ICancellableEvent {}

    public static final class Boom extends Event {
        public final List<String> log = new ArrayList<>();
    }

    public static final class Quiet extends Event {}

    /** An annotated listener object, registered then unregistered after the freeze. */
    public static final class Holder {
        @SubscribeEvent(priority = EventPriority.LOWEST, receiveCanceled = true)
        public void last(Leaf e) {
            e.log.add("holder " + e.isCanceled());
        }

        @SubscribeEvent(priority = EventPriority.HIGHEST)
        public void first(Leaf e) {
            e.log.add("holder-first");
        }
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    /** A bus and what its exception handler saw. */
    private record Side(EventBus bus, List<String> handled, Holder holder) {}

    private static Side side() {
        var handled = new ArrayList<String>();
        var bus = (EventBus) BusBuilder.builder().allowPerPhasePost().setExceptionHandler((b, event, listeners, index, t) -> handled.add(index + " " + t.getMessage())).build();
        bus.addListener(EventPriority.HIGH, false, Leaf.class, e -> e.log.add("high"));
        bus.addListener(EventPriority.NORMAL, false, Leaf.class, e -> {
            e.log.add("cancel");
            e.setCanceled(true);
        });
        bus.addListener(EventPriority.NORMAL, false, Leaf.class, e -> e.log.add("skipped-normal"));
        bus.addListener(EventPriority.LOW, true, Leaf.class, e -> {
            e.log.add("receives " + e.isCanceled());
            e.setCanceled(false);
        });
        bus.addListener(EventPriority.LOW, false, Leaf.class, e -> e.log.add("after-uncancel"));
        bus.addListener(EventPriority.LOWEST, false, Base.class, e -> e.log.add("base"));
        bus.addListener(Boom.class, e -> e.log.add("a"));
        bus.addListener(Boom.class, e -> {
            throw new IllegalStateException("boom");
        });
        bus.addListener(Boom.class, e -> e.log.add("c"));
        var holder = new Holder();
        bus.register(holder);
        return new Side(bus, handled, holder);
    }

    private static List<String> leaf(Side side, EventPriority phase) {
        var event = new Leaf();
        if (phase == null) side.bus().post(event);
        else side.bus().post(phase, event);
        var out = new ArrayList<>(event.log);
        out.add("canceled=" + event.isCanceled());
        return out;
    }

    private static List<String> boom(Side side) {
        var event = new Boom();
        String thrown = "none";
        try {
            side.bus().post(event);
        } catch (IllegalStateException e) {
            thrown = e.getMessage();
        }
        var out = new ArrayList<>(event.log);
        out.add("thrown=" + thrown);
        out.addAll(side.handled());
        side.handled().clear();
        return out;
    }

    private static void same(Side bus, Side compiled, String what) {
        var expected = leaf(bus, null);
        var got = leaf(compiled, null);
        check(expected.equals(got), what + ": " + got + " != " + expected);
        for (EventPriority phase : EventPriority.values()) {
            var phaseExpected = leaf(bus, phase);
            var phaseGot = leaf(compiled, phase);
            check(phaseExpected.equals(phaseGot), what + " at " + phase + ": " + phaseGot + " != " + phaseExpected);
        }
        var boomExpected = boom(bus);
        var boomGot = boom(compiled);
        check(boomExpected.equals(boomGot), what + " (exception): " + boomGot + " != " + boomExpected);
    }

    public static void main(String[] args) {
        Side bus = side();
        Side compiled = side();
        check(CompiledEvents.install(compiled.bus()), "the compiled side installs: the JVM runs with the host jar as -javaagent");
        check(!CompiledEvents.install(bus.bus()), "only one bus is the game's");

        same(bus, compiled, "priorities, cancel and receiveCanceled");
        var expected = List.of("holder-first", "high", "cancel", "receives true", "after-uncancel", "holder false", "base", "canceled=false");
        check(leaf(compiled, null).equals(expected), "the bus's own order: " + leaf(compiled, null));
        int generated = CompiledEvents.generated();
        check(generated >= 2, "Leaf and Boom were compiled: " + generated);

        int before = CompiledEvents.generated();
        compiled.bus().post(new Quiet());
        check(CompiledEvents.generated() == before, "a class with no listener compiles nothing and posts to no one");

        before = CompiledEvents.generated();
        leaf(compiled, null);
        check(CompiledEvents.generated() == before, "a second post reuses the compiled class");

        for (Side side : List.of(bus, compiled)) side.bus().addListener(EventPriority.HIGHEST, true, Leaf.class, e -> e.log.add("added"));
        same(bus, compiled, "a listener added after the freeze");
        check(leaf(compiled, null).contains("added"), "the added listener is heard");
        check(CompiledEvents.generated() > before, "adding regenerated the class");

        for (Side side : List.of(bus, compiled)) side.bus().addListener(EventPriority.NORMAL, false, Base.class, e -> e.log.add("base-added"));
        same(bus, compiled, "a supertype's listener added after the freeze");

        before = CompiledEvents.generated();
        for (Side side : List.of(bus, compiled)) side.bus().unregister(side.holder());
        same(bus, compiled, "a listener object unregistered after the freeze");
        check(!leaf(compiled, null).contains("holder-first"), "the unregistered listener is not heard");
        check(CompiledEvents.generated() > before, "unregistering regenerated the class");

        var carried = new ArrayList<List<String>>();
        check(CompiledEvents.carry(Leaf.class, () -> carried.add(leaf(compiled, null))), "a carried cancel needs compiled dispatch");
        check(carried.get(0).get(0).equals("added") && !carried.get(0).contains("high"), "an event arriving cancelled reaches only receivers: " + carried.get(0));
        check(leaf(compiled, null).contains("high"), "the carry ends with its post: the next starts uncancelled");

        System.out.println("CompiledEventsCheck ok");
    }
}
