package dev.horizonlab.javamods.boot;

import dev.horizonlab.javamods.host.BusAgent;
import java.lang.classfile.ClassFile;
import java.lang.classfile.ClassHierarchyResolver;
import java.lang.classfile.CodeBuilder;
import java.lang.classfile.Label;
import java.lang.constant.ClassDesc;
import java.lang.constant.ConstantDescs;
import java.lang.constant.DynamicConstantDesc;
import java.lang.constant.MethodTypeDesc;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.MethodType;
import java.lang.invoke.VarHandle;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.WeakHashMap;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.Consumer;
import java.util.function.Predicate;
import net.neoforged.bus.ConsumerEventHandler;
import net.neoforged.bus.EventBus;
import net.neoforged.bus.ListenerList;
import net.neoforged.bus.SubscribeEventListener;
import net.neoforged.bus.api.Event;
import net.neoforged.bus.api.EventListener;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.bus.api.ICancellableEvent;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.bus.api.IEventExceptionHandler;
import net.neoforged.bus.api.SubscribeEvent;
import net.neoforged.neoforge.common.NeoForge;

/** BOUNDARY §6 item 1: once the game bus has started, each posted event class runs a hidden class calling its listeners as the bus's own loop would. */
public final class CompiledEvents {
    private CompiledEvents() {}

    /** One event class's compiled listeners: the bus's whole list, or one phase's (by `EventPriority` ordinal). */
    public abstract static class Dispatcher {
        protected Dispatcher() {}

        public abstract void post(Event event);

        public abstract void post(int phase, Event event);
    }

    /** No listener applies to the class: a post calls nothing (item 2). */
    private static final Dispatcher NONE = new Dispatcher() {
        @Override
        public void post(Event event) {}

        @Override
        public void post(int phase, Event event) {}
    };

    /** The bus's private members compiled dispatch reads, as its bytecode (javap) names them. */
    private static final class Bus {
        static final VarHandle SHUTDOWN;
        static final VarHandle PER_PHASE;
        static final VarHandle HANDLER;
        static final MethodHandle CHECKS;
        static final MethodHandle LIST;
        static final MethodHandle PHASE_LIST;
        static final MethodHandle FILTER;
        static final VarHandle SUB_HANDLER;
        static final VarHandle SUB_INFO;
        static final VarHandle CONSUMER;
        static final VarHandle PREDICATE;
        static final MethodHandle INVOKE;
        static final MethodHandle ACCEPT;

        static {
            try {
                CompiledEvents.class.getModule().addReads(EventBus.class.getModule());
                var bus = MethodHandles.privateLookupIn(EventBus.class, MethodHandles.lookup());
                var list = MethodHandles.privateLookupIn(ListenerList.class, MethodHandles.lookup());
                SHUTDOWN = bus.findVarHandle(EventBus.class, "shutdown", boolean.class);
                PER_PHASE = bus.findVarHandle(EventBus.class, "allowPerPhasePost", boolean.class);
                HANDLER = bus.findVarHandle(EventBus.class, "exceptionHandler", IEventExceptionHandler.class);
                CHECKS = bus.findVirtual(EventBus.class, "doPostChecks", MethodType.methodType(void.class, Event.class));
                LIST = bus.findVirtual(EventBus.class, "getListenerList", MethodType.methodType(ListenerList.class, Class.class));
                PHASE_LIST = list.findVirtual(ListenerList.class, "getListeners", MethodType.methodType(ArrayList.class, EventPriority.class));
                FILTER = bus.findVirtual(EventBus.class, "passNotGenericFilter", MethodType.methodType(Predicate.class, boolean.class));
                var sub = MethodHandles.privateLookupIn(SubscribeEventListener.class, MethodHandles.lookup());
                SUB_HANDLER = sub.findVarHandle(SubscribeEventListener.class, "handler", EventListener.class);
                SUB_INFO = sub.findVarHandle(SubscribeEventListener.class, "subInfo", SubscribeEvent.class);
                var consumer = MethodHandles.privateLookupIn(ConsumerEventHandler.class, MethodHandles.lookup());
                CONSUMER = consumer.findVarHandle(ConsumerEventHandler.class, "consumer", Consumer.class);
                var predicate = MethodHandles.privateLookupIn(ConsumerEventHandler.WithPredicate.class, MethodHandles.lookup());
                PREDICATE = predicate.findVarHandle(ConsumerEventHandler.WithPredicate.class, "predicate", Predicate.class);
                INVOKE = MethodHandles.publicLookup().findVirtual(EventListener.class, "invoke", MethodType.methodType(void.class, Event.class));
                ACCEPT = MethodHandles.publicLookup().findVirtual(Consumer.class, "accept", MethodType.methodType(void.class, Object.class));
            } catch (ReflectiveOperationException e) {
                throw new ExceptionInInitializerError(e);
            }
        }
    }

    /** An event class's dispatcher, swapped whole when its listeners change: the class's mutable call site. */
    private static final class Compiled {
        final Class<?> type;
        final AtomicInteger version = new AtomicInteger();
        final Dispatcher stale = new Dispatcher() {
            @Override
            public void post(Event event) {
                regenerate().post(event);
            }

            @Override
            public void post(int phase, Event event) {
                regenerate().post(phase, event);
            }
        };
        volatile Dispatcher current = stale;

        Compiled(Class<?> type) {
            this.type = type;
        }

        void invalidate() {
            version.incrementAndGet();
            current = stale;
        }

        Dispatcher regenerate() {
            int seen = version.get();
            Dispatcher built = compile(game, type, this);
            synchronized (this) {
                if (version.get() == seen && current == stale) current = built;
            }
            return built;
        }
    }

    /** The bus whose posts run compiled: set once, when it has started. */
    private static volatile EventBus game;
    private static final ClassValue<Compiled> CLASSES = new ClassValue<>() {
        @Override
        protected Compiled computeValue(Class<?> type) {
            return new Compiled(type);
        }
    };
    private static final Map<ListenerList, Compiled> LISTS = Collections.synchronizedMap(new WeakHashMap<>());
    private static final AtomicInteger GENERATED = new AtomicInteger();
    private static final ThreadLocal<Class<?>> ARRIVING = new ThreadLocal<>();
    private static final AtomicInteger ARRIVALS = new AtomicInteger();
    private static boolean warned;

    /** Aims the game bus at compiled dispatch once it has started; whether its posts run compiled. */
    static boolean freeze() {
        return install((EventBus) NeoForge.EVENT_BUS);
    }

    /** [freeze] for any bus: the first started one wins and keeps it. */
    static synchronized boolean install(EventBus bus) {
        if (game != null) return game == bus;
        if (!BusAgent.loaded()) {
            if (!warned) System.err.println("Horizon: the JVM started without Horizon's bus agent: events post through the bus's own loop");
            warned = true;
            return false;
        }
        try {
            if ((boolean) Bus.SHUTDOWN.getVolatile(bus)) return false;
            var same = MethodHandles.lookup().findStatic(CompiledEvents.class, "same", MethodType.methodType(boolean.class, Object.class, Object.class));
            var isGame = MethodHandles.insertArguments(same, 1, bus).asType(MethodType.methodType(boolean.class, EventBus.class));
            var post = MethodHandles.lookup().findStatic(CompiledEvents.class, "post", MethodType.methodType(Event.class, EventBus.class, Event.class));
            var phasePost = MethodHandles.lookup().findStatic(CompiledEvents.class, "phasePost", MethodType.methodType(Event.class, EventBus.class, EventPriority.class, Event.class));
            var rebuilt = MethodHandles.lookup().findStatic(CompiledEvents.class, "rebuilt", MethodType.methodType(void.class, ListenerList.class));
            game = bus;
            BusAgent.aim("rebuilt", original -> rebuilt);
            BusAgent.aim("post", original -> MethodHandles.guardWithTest(MethodHandles.dropArguments(isGame, 1, Event.class), post, original));
            BusAgent.aim("phasePost", original -> MethodHandles.guardWithTest(MethodHandles.dropArguments(isGame, 1, EventPriority.class, Event.class), phasePost, original));
        } catch (Throwable t) {
            System.err.println("Horizon: compiled event dispatch is off, events post through the bus's own loop: " + t);
            return false;
        }
        System.err.println("Horizon: the game bus posts through compiled dispatch");
        return true;
    }

    private static boolean same(Object a, Object b) {
        return a == b;
    }

    /** `EventBus.post(T)` on the game bus: its shutdown and checks as the bus does them, then the class's dispatcher. */
    private static Event post(EventBus bus, Event event) throws Throwable {
        if ((boolean) Bus.SHUTDOWN.getVolatile(bus)) return event;
        Bus.CHECKS.invokeExact(bus, event);
        if (ARRIVALS.get() != 0) arrive(event);
        Dispatcher dispatcher = CLASSES.get(event.getClass()).current;
        if (!Profile.ON) {
            dispatcher.post(event);
            return event;
        }
        long start = System.nanoTime();
        try {
            dispatcher.post(event);
        } finally {
            Profile.post(event.getClass(), System.nanoTime() - start);
        }
        return event;
    }

    /** `EventBus.post(EventPriority, T)` on the game bus: one phase's listeners. */
    private static Event phasePost(EventBus bus, EventPriority phase, Event event) throws Throwable {
        if (!(boolean) Bus.PER_PHASE.get(bus)) throw new IllegalStateException("This bus does not allow calling phase-specific post.");
        if ((boolean) Bus.SHUTDOWN.getVolatile(bus)) return event;
        Bus.CHECKS.invokeExact(bus, event);
        if (ARRIVALS.get() != 0) arrive(event);
        Dispatcher dispatcher = CLASSES.get(event.getClass()).current;
        if (!Profile.ON) {
            dispatcher.post(phase.ordinal(), event);
            return event;
        }
        long start = System.nanoTime();
        try {
            dispatcher.post(phase.ordinal(), event);
        } finally {
            Profile.post(event.getClass(), System.nanoTime() - start);
        }
        return event;
    }

    /** `ListenerList.forceRebuild` ran: the class compiled from that list compiles again at its next post. */
    private static void rebuilt(ListenerList list) {
        Compiled compiled = LISTS.get(list);
        if (compiled != null) compiled.invalidate();
    }

    /** Runs `post` with this thread's next post of `type` starting cancelled; false when compiled dispatch is off. */
    static boolean carry(Class<?> type, Runnable post) {
        if (game == null || !ICancellableEvent.class.isAssignableFrom(type)) return false;
        ARRIVING.set(type);
        ARRIVALS.incrementAndGet();
        try {
            post.run();
        } finally {
            ARRIVING.remove();
            ARRIVALS.decrementAndGet();
        }
        return true;
    }

    private static void arrive(Event event) {
        Class<?> type = ARRIVING.get();
        if (type != null && type.isInstance(event) && event instanceof ICancellableEvent cancellable) {
            ARRIVING.remove();
            cancellable.setCanceled(true);
        }
    }

    /** How many dispatchers were generated: once per event class posted, again after its listeners change. */
    static int generated() {
        return GENERATED.get();
    }

    /** One listener as compiled: its target taking the event, and whether a cancelled event skips it; null calls nothing. */
    private record Call(MethodHandle target, boolean skipsCanceled) {}

    /** What `listener.invoke` does, taken apart where the bus's wrapper is known; anything else is called as the bus would. */
    private static Call call(EventListener listener, boolean cancellable, Predicate<?> notCanceled) {
        MethodType type = MethodType.methodType(void.class, Event.class);
        if (cancellable && listener instanceof SubscribeEventListener subscribed) {
            var handler = (EventListener) Bus.SUB_HANDLER.get(subscribed);
            if (handler == null) return null;
            boolean receives = ((SubscribeEvent) Bus.SUB_INFO.get(subscribed)).receiveCanceled();
            return new Call(Bus.INVOKE.bindTo(handler), !receives);
        }
        if (cancellable && listener.getClass() == ConsumerEventHandler.WithPredicate.class && Bus.PREDICATE.get(listener) == notCanceled) {
            return new Call(Bus.ACCEPT.bindTo(Bus.CONSUMER.get(listener)).asType(type), true);
        }
        if (listener.getClass() == ConsumerEventHandler.class) {
            return new Call(Bus.ACCEPT.bindTo(Bus.CONSUMER.get(listener)).asType(type), false);
        }
        return new Call(Bus.INVOKE.bindTo(listener), false);
    }

    /** Whether a listener of a cancellable event still hears it once cancelled, as the bus's wrapper decides. */
    static boolean receivesCanceled(EventListener listener) {
        if (listener instanceof SubscribeEventListener subscribed) return ((SubscribeEvent) Bus.SUB_INFO.get(subscribed)).receiveCanceled();
        return listener.getClass() == ConsumerEventHandler.class;
    }

    /** `event`'s listeners of one priority on `bus`, its supertypes' included, wrappers kept. */
    @SuppressWarnings("unchecked")
    static List<EventListener> phaseListeners(IEventBus bus, Class<?> event, EventPriority phase) throws Throwable {
        var list = (ListenerList) Bus.LIST.invoke((EventBus) bus, event);
        return (List<EventListener>) Bus.PHASE_LIST.invoke(list, phase);
    }

    /** The dispatcher of `type` on `bus` as its listeners stand now; `into` hears when they change. */
    static Dispatcher compile(EventBus bus, Class<?> type, Compiled into) {
        try {
            var list = (ListenerList) Bus.LIST.invoke(bus, type);
            if (into != null) LISTS.put(list, into);
            EventListener[] all = list.getListeners();
            if (all.length == 0) return NONE;
            EventListener[][] phases = null;
            if ((boolean) Bus.PER_PHASE.get(bus)) {
                phases = new EventListener[EventPriority.values().length][];
                for (EventPriority phase : EventPriority.values()) phases[phase.ordinal()] = list.getPhaseListeners(phase);
            }
            return generate(bus, type, all, phases);
        } catch (Throwable t) {
            throw new IllegalStateException("Horizon: compiled dispatch of " + type.getName() + " failed", t);
        }
    }

    /** A dispatcher for `type` on `bus` that is not installed: what `post` would run, for checks beside the bus's own. */
    static Dispatcher compileDetached(EventBus bus, Class<?> type) {
        return compile(bus, type, null);
    }

    private static final ClassDesc EVENT = ClassDesc.of(Event.class.getName());
    private static final ClassDesc CANCELLABLE = ClassDesc.of(ICancellableEvent.class.getName());
    private static final ClassDesc HANDLER = ClassDesc.of(IEventExceptionHandler.class.getName());
    private static final ClassDesc BUS = ClassDesc.of(IEventBus.class.getName());
    private static final ClassDesc LISTENERS = ClassDesc.of(EventListener.class.getName()).arrayType();
    private static final ClassDesc DISPATCHER = ClassDesc.of(Dispatcher.class.getName());
    private static final MethodTypeDesc TAKES_EVENT = MethodTypeDesc.of(ConstantDescs.CD_void, EVENT);
    private static final MethodTypeDesc TAKES_PHASE = MethodTypeDesc.of(ConstantDescs.CD_void, ConstantDescs.CD_int, EVENT);
    private static final MethodTypeDesc HANDLE = MethodTypeDesc.of(ConstantDescs.CD_void, BUS, EVENT, LISTENERS, ConstantDescs.CD_int, ConstantDescs.CD_Throwable);

    private static Dispatcher generate(EventBus bus, Class<?> type, EventListener[] all, EventListener[][] phases) throws Throwable {
        boolean cancellable = ICancellableEvent.class.isAssignableFrom(type);
        var notCanceled = (Predicate<?>) Bus.FILTER.invoke(bus, false);
        List<Object> data = new ArrayList<>();
        data.add(Bus.HANDLER.get(bus));
        data.add(bus);
        ClassDesc self = ClassDesc.of(CompiledEvents.class.getPackageName() + ".CompiledEvent");
        var resolver = ClassHierarchyResolver.defaultResolver().orElse(ClassHierarchyResolver.ofClassLoading(CompiledEvents.class.getClassLoader()));
        byte[] bytes = ClassFile.of(ClassFile.ClassHierarchyResolverOption.of(resolver)).build(self, cls -> {
            cls.withSuperclass(DISPATCHER).withFlags(ClassFile.ACC_PUBLIC | ClassFile.ACC_FINAL);
            cls.withMethodBody(ConstantDescs.INIT_NAME, ConstantDescs.MTD_void, ClassFile.ACC_PUBLIC,
                code -> code.aload(0).invokespecial(DISPATCHER, ConstantDescs.INIT_NAME, ConstantDescs.MTD_void).return_());
            cls.withMethodBody("post", TAKES_EVENT, ClassFile.ACC_PUBLIC, code -> calls(code, all, cancellable, notCanceled, data));
            int count = phases == null ? 0 : phases.length;
            cls.withMethodBody("post", TAKES_PHASE, ClassFile.ACC_PUBLIC, code -> {
                for (int phase = 0; phase < count; phase++) {
                    Label next = code.newLabel();
                    code.iload(1).loadConstant(phase).if_icmpne(next);
                    code.aload(0).aload(2).invokevirtual(self, "phase" + phase, TAKES_EVENT).return_();
                    code.labelBinding(next);
                }
                code.return_();
            });
            for (int phase = 0; phase < count; phase++) {
                EventListener[] listeners = phases[phase];
                cls.withMethodBody("phase" + phase, TAKES_EVENT, ClassFile.ACC_PRIVATE, code -> calls(code, listeners, cancellable, notCanceled, data));
            }
        });
        var hidden = MethodHandles.lookup().defineHiddenClassWithClassData(bytes, List.copyOf(data), true);
        GENERATED.incrementAndGet();
        return (Dispatcher) hidden.findConstructor(hidden.lookupClass(), MethodType.methodType(void.class)).invoke();
    }

    /** The bus's loop over `listeners`, unrolled: index, cancel check, the listener's constant handle; a throw goes to the bus's handler, then on. */
    private static void calls(CodeBuilder code, EventListener[] listeners, boolean cancellable, Predicate<?> notCanceled, List<Object> data) {
        var calls = new Call[listeners.length];
        boolean any = false;
        for (int i = 0; i < listeners.length; i++) {
            calls[i] = call(listeners[i], cancellable, notCanceled);
            any |= calls[i] != null;
        }
        if (!any) {
            code.return_();
            return;
        }
        int array = data.size();
        data.add(listeners);
        Label start = code.newLabel();
        Label end = code.newLabel();
        Label caught = code.newLabel();
        code.iconst_0().istore(2);
        code.labelBinding(start);
        for (int i = 0; i < calls.length; i++) {
            if (calls[i] == null) continue;
            code.loadConstant(i).istore(2);
            Label skip = code.newLabel();
            if (calls[i].skipsCanceled()) {
                code.aload(1).checkcast(CANCELLABLE).invokeinterface(CANCELLABLE, "isCanceled", MethodTypeDesc.of(ConstantDescs.CD_boolean)).ifne(skip);
            }
            code.ldc(constant(data, calls[i].target(), ConstantDescs.CD_MethodHandle));
            code.aload(1).invokevirtual(ConstantDescs.CD_MethodHandle, "invokeExact", TAKES_EVENT);
            code.labelBinding(skip);
        }
        code.labelBinding(end);
        code.return_();
        code.labelBinding(caught);
        code.astore(3);
        code.ldc(at(0, HANDLER)).ldc(at(1, BUS)).aload(1).ldc(at(array, LISTENERS)).iload(2).aload(3);
        code.invokeinterface(HANDLER, "handleException", HANDLE);
        code.aload(3).athrow();
        code.exceptionCatch(start, end, caught, ConstantDescs.CD_Throwable);
    }

    private static DynamicConstantDesc<Object> constant(List<Object> data, Object value, ClassDesc type) {
        data.add(value);
        return at(data.size() - 1, type);
    }

    private static DynamicConstantDesc<Object> at(int index, ClassDesc type) {
        return DynamicConstantDesc.ofNamed(ConstantDescs.BSM_CLASS_DATA_AT, ConstantDescs.DEFAULT_NAME, type, index);
    }
}
