package dev.horizonlab.javamods.host;

import java.lang.classfile.ClassBuilder;
import java.lang.classfile.ClassElement;
import java.lang.classfile.ClassFile;
import java.lang.classfile.ClassTransform;
import java.lang.classfile.CodeBuilder;
import java.lang.classfile.MethodModel;
import java.lang.constant.ClassDesc;
import java.lang.constant.ConstantDescs;
import java.lang.constant.DirectMethodHandleDesc;
import java.lang.constant.DynamicCallSiteDesc;
import java.lang.constant.MethodHandleDesc;
import java.lang.constant.MethodTypeDesc;
import java.lang.instrument.ClassFileTransformer;
import java.lang.instrument.Instrumentation;
import java.lang.invoke.CallSite;
import java.lang.invoke.MethodHandle;
import java.lang.invoke.MethodHandles;
import java.lang.invoke.MethodType;
import java.lang.invoke.MutableCallSite;
import java.security.ProtectionDomain;
import java.util.HashMap;
import java.util.Map;
import java.util.function.UnaryOperator;

/** `-javaagent`: the bus's `post` forms and `ListenerList.forceRebuild` call through mutable sites the boot aims at `CompiledEvents` (bus read with javap only). */
public final class BusAgent {
    private BusAgent() {}

    static final String BUS = "net/neoforged/bus/EventBus";
    static final String LIST = "net/neoforged/bus/ListenerList";
    private static final ClassDesc EVENT = ClassDesc.of("net.neoforged.bus.api.Event");
    private static final ClassDesc PRIORITY = ClassDesc.of("net.neoforged.bus.api.EventPriority");
    private static final MethodTypeDesc POST = MethodTypeDesc.of(EVENT, EVENT);
    private static final MethodTypeDesc PHASE_POST = MethodTypeDesc.of(EVENT, PRIORITY, EVENT);
    private static final MethodTypeDesc REBUILD = MethodTypeDesc.of(ConstantDescs.CD_void);
    private static final MethodTypeDesc LINK = MethodTypeDesc.of(ConstantDescs.CD_CallSite, ConstantDescs.CD_MethodHandles_Lookup, ConstantDescs.CD_String, ConstantDescs.CD_MethodType);

    /** Whether the JVM started with this agent. */
    private static volatile boolean loaded;
    /** Each linked site by name (`post`, `phasePost`, `rebuilt`), with the bus's own code it first called. */
    private static final Map<String, MutableCallSite> SITES = new HashMap<>();
    private static final Map<String, MethodHandle> ORIGINALS = new HashMap<>();
    private static final Map<String, UnaryOperator<MethodHandle>> AIMS = new HashMap<>();

    public static void premain(String args, Instrumentation instrumentation) {
        instrumentation.addTransformer(new ClassFileTransformer() {
            @Override
            public byte[] transform(Module module, ClassLoader loader, String name, Class<?> redefined, ProtectionDomain domain, byte[] bytes) {
                if (redefined != null || !(BUS.equals(name) || LIST.equals(name))) return null;
                try {
                    return BUS.equals(name) ? patchBus(bytes) : patchList(bytes);
                } catch (Throwable t) {
                    System.err.println("Horizon: NeoForge's " + name + " is not patched, events post through the bus's own loop: " + t);
                    return null;
                }
            }
        });
        loaded = true;
    }

    /** Whether the bus's posts can be aimed: the JVM started with this agent. */
    public static boolean loaded() {
        return loaded;
    }

    /** Aims the site `name` at `aim(the bus's own code)`, now if it is linked and when it links otherwise. */
    public static synchronized void aim(String name, UnaryOperator<MethodHandle> aim) {
        AIMS.put(name, aim);
        MutableCallSite site = SITES.get(name);
        if (site == null) return;
        site.setTarget(aim.apply(ORIGINALS.get(name)));
        MutableCallSite.syncAll(new MutableCallSite[] {site});
    }

    /** The bootstrap of the bus's patched methods (through their `horizon$link`): one site per name, shared by every bus. */
    public static synchronized CallSite link(MethodHandles.Lookup lookup, String name, MethodType type) throws ReflectiveOperationException {
        MutableCallSite site = SITES.get(name);
        if (site != null) return site;
        MethodHandle original = name.equals("rebuilt")
            ? MethodHandles.empty(type)
            : lookup.findVirtual(lookup.lookupClass(), "horizon$" + name, type.dropParameterTypes(0, 1));
        UnaryOperator<MethodHandle> aim = AIMS.get(name);
        site = new MutableCallSite(aim == null ? original : aim.apply(original));
        ORIGINALS.put(name, original);
        SITES.put(name, site);
        return site;
    }

    /** `post(Event)` and `post(EventPriority, Event)` become `horizon$post`/`horizon$phasePost`, called through a site. */
    static byte[] patchBus(byte[] bytes) {
        var file = ClassFile.of();
        var model = file.parse(bytes);
        ClassDesc self = model.thisClass().asSymbol();
        return file.transformClass(model, new ClassTransform() {
            @Override
            public void accept(ClassBuilder builder, ClassElement element) {
                if (element instanceof MethodModel method && method.methodName().equalsString("post")) {
                    MethodTypeDesc type = method.methodTypeSymbol();
                    String renamed = type.equals(POST) ? "horizon$post" : type.equals(PHASE_POST) ? "horizon$phasePost" : null;
                    if (renamed != null) {
                        builder.withMethod(builder.constantPool().utf8Entry(renamed), method.methodType(), method.flags().flagsMask(), copy -> method.forEach(copy::with));
                        return;
                    }
                }
                builder.with(element);
            }

            @Override
            public void atEnd(ClassBuilder builder) {
                builder.withMethodBody("post", POST, ClassFile.ACC_PUBLIC, code -> {
                    code.aload(0).aload(1);
                    code.invokedynamic(site(self, "post", MethodTypeDesc.of(EVENT, self, EVENT)));
                    code.areturn();
                });
                builder.withMethodBody("post", PHASE_POST, ClassFile.ACC_PUBLIC, code -> {
                    code.aload(0).aload(1).aload(2);
                    code.invokedynamic(site(self, "phasePost", MethodTypeDesc.of(EVENT, self, PRIORITY, EVENT)));
                    code.areturn();
                });
                builder.withMethodBody("horizon$link", LINK, ClassFile.ACC_PRIVATE | ClassFile.ACC_STATIC, BusAgent::linkBody);
            }
        });
    }

    /** `forceRebuild()` (on a list and, through it, on its children) tells the `rebuilt` site after it ran. */
    static byte[] patchList(byte[] bytes) {
        var file = ClassFile.of();
        var model = file.parse(bytes);
        ClassDesc self = model.thisClass().asSymbol();
        return file.transformClass(model, new ClassTransform() {
            private int flags = ClassFile.ACC_PROTECTED;

            @Override
            public void accept(ClassBuilder builder, ClassElement element) {
                if (element instanceof MethodModel method && method.methodName().equalsString("forceRebuild") && method.methodTypeSymbol().equals(REBUILD)) {
                    flags = method.flags().flagsMask();
                    builder.withMethod(builder.constantPool().utf8Entry("horizon$forceRebuild"), method.methodType(), flags, copy -> method.forEach(copy::with));
                    return;
                }
                builder.with(element);
            }

            @Override
            public void atEnd(ClassBuilder builder) {
                builder.withMethodBody("forceRebuild", REBUILD, flags, code -> {
                    code.aload(0).invokevirtual(self, "horizon$forceRebuild", REBUILD);
                    code.aload(0).invokedynamic(site(self, "rebuilt", MethodTypeDesc.of(ConstantDescs.CD_void, self)));
                    code.return_();
                });
                builder.withMethodBody("horizon$link", LINK, ClassFile.ACC_PRIVATE | ClassFile.ACC_STATIC, BusAgent::linkBody);
            }
        });
    }

    private static DynamicCallSiteDesc site(ClassDesc owner, String name, MethodTypeDesc type) {
        DirectMethodHandleDesc bootstrap = MethodHandleDesc.ofMethod(DirectMethodHandleDesc.Kind.STATIC, owner, "horizon$link", LINK);
        return DynamicCallSiteDesc.of(bootstrap, name, type);
    }

    // The bus's loader may not see the app class path, so its bootstrap finds this class through the system loader.
    private static void linkBody(CodeBuilder code) {
        ClassDesc lookup = ConstantDescs.CD_MethodHandles_Lookup;
        ClassDesc handle = ConstantDescs.CD_MethodHandle;
        code.invokestatic(ConstantDescs.CD_MethodHandles, "publicLookup", MethodTypeDesc.of(lookup));
        code.ldc(BusAgent.class.getName());
        code.iconst_1();
        code.invokestatic(ClassDesc.of("java.lang.ClassLoader"), "getSystemClassLoader", MethodTypeDesc.of(ClassDesc.of("java.lang.ClassLoader")));
        code.invokestatic(ConstantDescs.CD_Class, "forName", MethodTypeDesc.of(ConstantDescs.CD_Class, ConstantDescs.CD_String, ConstantDescs.CD_boolean, ClassDesc.of("java.lang.ClassLoader")));
        code.ldc("link");
        code.ldc(LINK);
        code.invokevirtual(lookup, "findStatic", MethodTypeDesc.of(handle, ConstantDescs.CD_Class, ConstantDescs.CD_String, ConstantDescs.CD_MethodType));
        code.aload(0).aload(1).aload(2);
        code.invokevirtual(handle, "invokeExact", LINK);
        code.areturn();
    }
}
