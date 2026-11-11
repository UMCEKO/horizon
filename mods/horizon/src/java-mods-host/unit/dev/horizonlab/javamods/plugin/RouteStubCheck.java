package dev.horizonlab.javamods.plugin;

import java.io.PrintWriter;
import java.io.StringWriter;
import org.objectweb.asm.ClassReader;
import org.objectweb.asm.ClassWriter;
import org.objectweb.asm.Opcodes;
import org.objectweb.asm.tree.ClassNode;
import org.objectweb.asm.tree.InsnNode;
import org.objectweb.asm.tree.JumpInsnNode;
import org.objectweb.asm.tree.LabelNode;
import org.objectweb.asm.tree.MethodNode;
import org.objectweb.asm.tree.VarInsnNode;
import org.objectweb.asm.util.CheckClassAdapter;

/** The route stub (`BridgeProcessor.routeStub`) on classes built here: both paths, the shared tail, the extra early return. */
public final class RouteStubCheck {
    public static class Base {
        public static int nativeValue = 7;
        public static boolean earlyFlag;
        public static int earlyCalls;
        public static int voidCalls;
        public static int flushes;

        public static void beforeNativeBody() {
            flushes++;
        }

        public static int nat(int x) {
            return nativeValue;
        }

        public static void natVoid() {
            voidCalls++;
        }

        public static boolean early() {
            earlyCalls++;
            return earlyFlag;
        }
    }

    private static final String BASE = "dev/horizonlab/javamods/plugin/RouteStubCheck$Base";

    private static final class Loader extends ClassLoader {
        Loader() {
            super(RouteStubCheck.class.getClassLoader());
        }

        Class<?> define(String name, byte[] bytes) {
            return defineClass(name, bytes, 0, bytes.length);
        }
    }

    private static ClassNode build(String name, String superName) {
        ClassNode node = new ClassNode();
        node.version = Opcodes.V21;
        node.access = Opcodes.ACC_PUBLIC;
        node.name = name;
        node.superName = superName;
        MethodNode init = new MethodNode(Opcodes.ACC_PUBLIC, "<init>", "()V", null, null);
        init.instructions.add(new VarInsnNode(Opcodes.ALOAD, 0));
        init.instructions.add(new org.objectweb.asm.tree.MethodInsnNode(Opcodes.INVOKESPECIAL, superName, "<init>", "()V", false));
        init.instructions.add(new InsnNode(Opcodes.RETURN));
        node.methods.add(init);
        MethodNode two = new MethodNode(Opcodes.ACC_PUBLIC, "twoReturns", "(I)I", null, null);
        LabelNode positive = new LabelNode();
        two.instructions.add(new VarInsnNode(Opcodes.ILOAD, 1));
        two.instructions.add(new JumpInsnNode(Opcodes.IFGE, positive));
        two.instructions.add(new InsnNode(Opcodes.ICONST_M1));
        two.instructions.add(new InsnNode(Opcodes.IRETURN));
        two.instructions.add(positive);
        two.instructions.add(new VarInsnNode(Opcodes.ILOAD, 1));
        two.instructions.add(new InsnNode(Opcodes.ICONST_2));
        two.instructions.add(new InsnNode(Opcodes.IMUL));
        two.instructions.add(new InsnNode(Opcodes.IRETURN));
        two.maxLocals = 2;
        two.maxStack = 2;
        node.methods.add(two);
        MethodNode one = new MethodNode(Opcodes.ACC_PUBLIC, "oneVoid", "()V", null, null);
        one.instructions.add(new InsnNode(Opcodes.RETURN));
        one.maxLocals = 1;
        node.methods.add(one);
        return node;
    }

    private static BridgeProcessor.Entry entry(String owner, String method, String desc, String nativeName, String[] args, String ret) {
        return new BridgeProcessor.Entry(owner, method, desc, BASE, nativeName, args, ret, true, "");
    }

    private static int returns(MethodNode method) {
        int n = 0;
        for (var insn = method.instructions.getFirst(); insn != null; insn = insn.getNext()) {
            if (insn.getOpcode() >= Opcodes.IRETURN && insn.getOpcode() <= Opcodes.RETURN) n++;
        }
        return n;
    }

    /** The class with the stubs in both methods, as bytes; `early` asks `Base.early` after the native. */
    private static ClassNode stubbed(String name, String superName, boolean early) {
        ClassNode node = build(name, superName);
        for (MethodNode m : node.methods) {
            switch (m.name) {
                case "twoReturns" -> BridgeProcessor.routeStub(node, m, entry(name, "twoReturns", "(I)I", "nat", new String[] {"I"}, "I"), early, BASE, "early");
                case "oneVoid" -> BridgeProcessor.routeStub(node, m, entry(name, "oneVoid", "()V", "natVoid", new String[0], "V"), early, BASE, "early");
                default -> {}
            }
        }
        return node;
    }

    private static byte[] write(ClassNode node) {
        ClassWriter writer = new ClassWriter(ClassWriter.COMPUTE_FRAMES);
        node.accept(writer);
        return writer.toByteArray();
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    public static void main(String[] args) throws Exception {
        BridgeProcessor.playerSync = BASE;
        MethodNode plain = build("P", "java/lang/Object").methods.stream().filter(m -> m.name.equals("twoReturns")).findFirst().orElseThrow();
        int base = returns(plain);
        for (boolean early : new boolean[] {false, true}) {
            ClassNode node = stubbed("Off" + early, "java/lang/Object", early);
            MethodNode two = node.methods.stream().filter(m -> m.name.equals("twoReturns")).findFirst().orElseThrow();
            check(two.instructions.getLast().getOpcode() == Opcodes.IRETURN, "the last instruction returns");
            check(returns(two) == base + (early ? 1 : 0), "return count, early=" + early);
            var tail = two.instructions.getLast().getPrevious();
            check(tail instanceof LabelNode, "the tail is a label");
            int toTail = 0, firstReturnsKept = 0;
            for (var insn = two.instructions.getFirst(); insn != null; insn = insn.getNext()) {
                if (insn instanceof JumpInsnNode jump && jump.getOpcode() == Opcodes.GOTO && jump.label == tail) toTail++;
            }
            check(toTail == 2, "the join and the original's last return both go to the tail, got " + toTail);
            for (var insn = two.instructions.getFirst(); insn != null && firstReturnsKept == 0; insn = insn.getNext()) {
                if (insn.getOpcode() == Opcodes.ICONST_M1) firstReturnsKept = insn.getNext().getOpcode() == Opcodes.IRETURN ? 1 : -1;
            }
            check(firstReturnsKept == 1, "the original's first return is still a return");
            byte[] bytes = write(node);
            StringWriter problems = new StringWriter();
            CheckClassAdapter.verify(new ClassReader(bytes), RouteStubCheck.class.getClassLoader(), false, new PrintWriter(problems));
            check(problems.toString().isEmpty(), "verify: " + problems);
            var off = new Loader().define(node.name, bytes).getDeclaredConstructor().newInstance();
            var twoOff = off.getClass().getMethod("twoReturns", int.class);
            check((int) twoOff.invoke(off, -3) == -1 && (int) twoOff.invoke(off, 4) == 8, "guard off runs the original");

            ClassNode onNode = stubbed("On" + early, BASE, early);
            Object on = new Loader().define(onNode.name, write(onNode)).getDeclaredConstructor().newInstance();
            var twoOn = on.getClass().getMethod("twoReturns", int.class);
            Base.earlyCalls = 0;
            Base.flushes = 0;
            twoOff.invoke(off, 4);
            check(Base.flushes == 0, "the original body flushes nothing");
            for (boolean flag : new boolean[] {false, true}) {
                Base.earlyFlag = flag;
                check((int) twoOn.invoke(on, -3) == 7 && (int) twoOn.invoke(on, 4) == 7, "guard on answers with the native, early=" + early + " flag=" + flag);
            }
            check(Base.earlyCalls == (early ? 4 : 0), "early is asked only with earlyCheck, got " + Base.earlyCalls);
            check(Base.flushes == 4, "Java's player changes reach native before each native body, got " + Base.flushes);
            Base.voidCalls = 0;
            on.getClass().getMethod("oneVoid").invoke(on);
            off.getClass().getMethod("oneVoid").invoke(off);
            check(Base.voidCalls == 1, "the void stub calls the native once, only when the guard is on");
        }
        javaBodied();
        System.out.println("RouteStubCheck ok");
    }

    /** Stand-in for W5's `GuiRoutes`. */
    public static class Gui {
        public static boolean backed;
        public static boolean mark;
        public static String lastSeam;
        public static Object[] lastArgs;

        public static boolean backed(Object self) {
            return backed;
        }

        public static Object body(String seam, Object self, Object[] args) {
            lastSeam = seam;
            lastArgs = args;
            if (mark) dev.horizonlab.javamods.boot.Routing.markEarly();
            return 42;
        }
    }

    private static void javaBodied() throws Exception {
        BridgeProcessor.guiRoutes = "dev/horizonlab/javamods/plugin/RouteStubCheck$Gui";
        for (boolean early : new boolean[] {false, true}) {
            ClassNode node = build("J" + early, "java/lang/Object");
            MethodNode two = node.methods.stream().filter(m -> m.name.equals("twoReturns")).findFirst().orElseThrow();
            var java = new BridgeProcessor.Entry(node.name, "twoReturns", "(I)I", "", "", new String[0], "", true, "seam");
            check(BridgeProcessor.routeStub(node, two, java, early, "dev/horizonlab/javamods/boot/Routing", "earlyJava"), "stubbed");
            byte[] bytes = write(node);
            StringWriter problems = new StringWriter();
            CheckClassAdapter.verify(new ClassReader(bytes), RouteStubCheck.class.getClassLoader(), false, new PrintWriter(problems));
            check(problems.toString().isEmpty(), "verify: " + problems);
            Object o = new Loader().define(node.name, bytes).getDeclaredConstructor().newInstance();
            var method = o.getClass().getMethod("twoReturns", int.class);
            Gui.backed = false;
            check((int) method.invoke(o, -3) == -1 && (int) method.invoke(o, 4) == 8, "not backed runs the original");
            Gui.backed = true;
            Gui.mark = early;
            check((int) method.invoke(o, 4) == 42, "backed answers with the unboxed body value");
            check("seam".equals(Gui.lastSeam) && Gui.lastArgs.length == 1 && Gui.lastArgs[0].equals(4), "seam and boxed arguments");
            check(!dev.horizonlab.javamods.boot.Routing.earlyJava(), "the stub consumed the early flag");
        }
    }
}
