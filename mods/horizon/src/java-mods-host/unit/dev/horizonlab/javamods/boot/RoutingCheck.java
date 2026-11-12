package dev.horizonlab.javamods.boot;

import com.google.gson.JsonArray;
import com.google.gson.JsonParser;
import java.util.List;
import java.util.Map;
import org.objectweb.asm.ClassWriter;
import org.objectweb.asm.Opcodes;
import org.objectweb.asm.Type;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.ModifyArg;
import org.spongepowered.asm.mixin.transformer.meta.MixinMerged;

/** A mixin the analysis predicted but Mixin left out turns its route off instead of failing the boot. */
public final class RoutingCheck {
    static final String PAUSE = "Lnet/minecraft/client/sounds/SoundManager;pauseAllExcept([Lnet/minecraft/sounds/SoundSource;)V";

    /** A target class as Mixin leaves it after merging the test mod's `MinecraftLoopMixin.keep`; `@MixinMerged` can't be written in source, so ASM writes it. */
    static Class<?> merged() {
        var writer = new ClassWriter(0);
        writer.visit(Opcodes.V21, Opcodes.ACC_PUBLIC, "dev/horizonlab/javamods/boot/MergedTarget", null, "java/lang/Object", null);
        var method = writer.visitMethod(Opcodes.ACC_PRIVATE, "modify$zza000$horizon_java_test$keep", "([Ljava/lang/Object;)[Ljava/lang/Object;", null, null);
        var from = method.visitAnnotation(Type.getDescriptor(MixinMerged.class), true);
        from.visit("mixin", "dev.horizonlab.javamods.testmod.mixin.MinecraftLoopMixin");
        from.visit("priority", 1000);
        from.visitEnd();
        var modify = method.visitAnnotation(Type.getDescriptor(ModifyArg.class), true);
        var names = modify.visitArray("method");
        names.visit(null, "runTick");
        names.visitEnd();
        var ats = modify.visitArray("at");
        var at = ats.visitAnnotation(null, Type.getDescriptor(At.class));
        at.visit("value", "INVOKE");
        at.visit("target", PAUSE);
        at.visitEnd();
        ats.visitEnd();
        modify.visitEnd();
        method.visitCode();
        method.visitVarInsn(Opcodes.ALOAD, 1);
        method.visitInsn(Opcodes.ARETURN);
        method.visitMaxs(1, 2);
        method.visitEnd();
        writer.visitEnd();
        byte[] bytes = writer.toByteArray();
        return new ClassLoader(RoutingCheck.class.getClassLoader()) {
            Class<?> define() {
                return defineClass("dev.horizonlab.javamods.boot.MergedTarget", bytes, 0, bytes.length);
            }
        }.define();
    }

    public static void main(String[] args) {
        var merged = List.<String[]>of(new String[] {"h$abc", "m.Mixin"});
        if (!Routing.applied(merged, "m.Mixin")) throw new AssertionError("an applied mixin counts");
        if (Routing.applied(merged, "other.Mixin")) throw new AssertionError("a mixin Mixin left out is off");
        if (Routing.applied(List.of(), "m.Mixin")) throw new AssertionError("no merged handler is off");
        contradictions();
        atTargets();
        loopOrder();
        System.out.println("RoutingCheck ok");
    }

    private static void atTargets() {
        var shapes = Routing.shapes(merged());
        if (shapes.size() != 1) throw new AssertionError("one merged handler: " + shapes);
        var keep = shapes.get(0);
        if (!keep.kind().equals("modify_arg") || !keep.at().equals(List.of("INVOKE")) || !keep.targets().equals(List.of("runTick"))) throw new AssertionError("the ModifyArg's shape: " + keep);
        if (!keep.atTargets().equals(List.of(PAUSE))) throw new AssertionError("the @At's target is kept: " + keep.atTargets());
    }

    private static void loopOrder() {
        if (LoopEdges.compare(900, "handler$zzb009$a$x", 1000, "handler$zza000$b$y") >= 0) throw new AssertionError("a lower priority runs first");
        if (LoopEdges.compare(1000, "handler$zza010$a$x", 1000, "handler$zza009$b$y") <= 0) throw new AssertionError("the merge counter orders equal priorities");
        if (LoopEdges.compare(1000, "x", 1000, "handler$zza000$b$y") != null) throw new AssertionError("a name without a counter can't be ordered");
    }

    private static void contradictions() {
        String key = "net/minecraft/A#m(I)V";
        JsonArray changes = JsonParser.parseString("[{\"mod\":\"x\",\"mixin\":\"m.Mixin\",\"handler\":\"head\",\"kind\":\"inject\",\"at\":[\"HEAD\"],\"locals\":false}]").getAsJsonArray();
        var mods = Map.of("m.Mixin", "x");
        var head = new Routing.Applied("handler$z$x$head", "m.Mixin", "inject", List.of("HEAD"), false, 2, List.of("m"), List.of(""));
        if (Routing.contradiction(key, "m", 1, false, changes, List.of(head), mods) != null) throw new AssertionError("a predicted edge that applied as an edge agrees");
        var stray = new Routing.Applied("handler$z$x$other", "m.Mixin", "inject", List.of("HEAD"), false, 2, List.of("m"), List.of(""));
        var unpredicted = Routing.contradiction(key, "m", 1, false, changes, List.of(head, stray), mods);
        if (unpredicted == null || !unpredicted.contains("x") || !unpredicted.contains("other")) throw new AssertionError("an applied mixin nobody predicted is an error naming mod and handler: " + unpredicted);
        var invoke = new Routing.Applied("handler$z$x$head", "m.Mixin", "inject", List.of("INVOKE"), false, 2, List.of("m(I)V"), List.of(""));
        var kind = Routing.contradiction(key, "m", 1, false, changes, List.of(invoke), mods);
        if (kind == null || !kind.contains("x") || !kind.contains("head") || !kind.contains("predicted edge")) throw new AssertionError("an edge predicted but applied as a body is an error: " + kind);
        var elsewhere = new Routing.Applied("handler$z$x$far", "m.Mixin", "inject", List.of("HEAD"), false, 2, List.of("other"), List.of(""));
        if (Routing.contradiction(key, "m", 1, false, changes, List.of(head, elsewhere), mods) != null) throw new AssertionError("a handler for another method is not this route's");
    }
}
