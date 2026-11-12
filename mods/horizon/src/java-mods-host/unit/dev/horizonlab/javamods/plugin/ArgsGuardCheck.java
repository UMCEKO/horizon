package dev.horizonlab.javamods.plugin;

import java.io.PrintWriter;
import java.io.StringWriter;
import java.util.Arrays;
import org.objectweb.asm.ClassReader;
import org.objectweb.asm.ClassWriter;
import org.objectweb.asm.Opcodes;
import org.objectweb.asm.tree.ClassNode;
import org.objectweb.asm.tree.InsnNode;
import org.objectweb.asm.tree.MethodInsnNode;
import org.objectweb.asm.tree.MethodNode;
import org.objectweb.asm.tree.VarInsnNode;
import org.objectweb.asm.util.CheckClassAdapter;

/** The `@Class.method+` guard (`BridgeProcessor.nativeCall`): it receives the bridged method's own parameters, wide ones included. */
public final class ArgsGuardCheck {
    public static class G {
        public static boolean flag;
        public static Object[] seen;

        public static boolean guard(Object self, String s, int i, double d) {
            seen = new Object[] {s, i, d};
            return flag;
        }

        public static boolean wide(Object self, double d, int i, String s) {
            seen = new Object[] {d, i, s};
            return flag;
        }

        public static int first(String s, int i, double d) {
            return 2;
        }

        public static int second(double d, int i, String s) {
            return 2;
        }
    }

    private static final String HOLDER = "dev/horizonlab/javamods/plugin/ArgsGuardCheck$G";

    private static final class Loader extends ClassLoader {
        Loader() {
            super(ArgsGuardCheck.class.getClassLoader());
        }

        Class<?> define(String name, byte[] bytes) {
            return defineClass(name, bytes, 0, bytes.length);
        }
    }

    private static MethodNode returnsOne(String name, String desc, int locals) {
        MethodNode method = new MethodNode(Opcodes.ACC_PUBLIC, name, desc, null, null);
        method.instructions.add(new InsnNode(Opcodes.ICONST_1));
        method.instructions.add(new InsnNode(Opcodes.IRETURN));
        method.maxLocals = locals;
        method.maxStack = 1;
        return method;
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    public static void main(String[] args) throws Exception {
        ClassNode node = new ClassNode();
        node.version = Opcodes.V21;
        node.access = Opcodes.ACC_PUBLIC;
        node.name = "Guarded";
        node.superName = "java/lang/Object";
        MethodNode init = new MethodNode(Opcodes.ACC_PUBLIC, "<init>", "()V", null, null);
        init.instructions.add(new VarInsnNode(Opcodes.ALOAD, 0));
        init.instructions.add(new MethodInsnNode(Opcodes.INVOKESPECIAL, "java/lang/Object", "<init>", "()V", false));
        init.instructions.add(new InsnNode(Opcodes.RETURN));
        init.maxLocals = 1;
        init.maxStack = 1;
        node.methods.add(init);
        MethodNode first = returnsOne("first", "(Ljava/lang/String;ID)I", 5);
        MethodNode second = returnsOne("second", "(DILjava/lang/String;)I", 5);
        node.methods.add(first);
        node.methods.add(second);
        BridgeProcessor.bridge(node, first, new BridgeProcessor.Entry("Guarded", "first", "(Ljava/lang/String;ID)I", "@" + HOLDER + ".guard+", "first", new String[] {"Ljava/lang/String;", "I", "D"}, "I", false, ""));
        BridgeProcessor.bridge(node, second, new BridgeProcessor.Entry("Guarded", "second", "(DILjava/lang/String;)I", "@" + HOLDER + ".wide+", "second", new String[] {"D", "I", "Ljava/lang/String;"}, "I", false, ""));
        ClassWriter writer = new ClassWriter(ClassWriter.COMPUTE_FRAMES);
        node.accept(writer);
        byte[] bytes = writer.toByteArray();
        StringWriter problems = new StringWriter();
        CheckClassAdapter.verify(new ClassReader(bytes), ArgsGuardCheck.class.getClassLoader(), false, new PrintWriter(problems));
        check(problems.toString().isEmpty(), "verify: " + problems);
        Object o = new Loader().define("Guarded", bytes).getDeclaredConstructor().newInstance();
        var m1 = o.getClass().getMethod("first", String.class, int.class, double.class);
        var m2 = o.getClass().getMethod("second", double.class, int.class, String.class);
        G.flag = false;
        check((int) m1.invoke(o, "a", 3, 4.5) == 1, "guard false runs the Java body");
        check(Arrays.equals(G.seen, new Object[] {"a", 3, 4.5}), "guard saw the arguments");
        check((int) m2.invoke(o, 4.5, 3, "a") == 1, "wide guard false runs the Java body");
        check(Arrays.equals(G.seen, new Object[] {4.5, 3, "a"}), "wide guard saw the arguments");
        G.flag = true;
        check((int) m1.invoke(o, "a", 3, 4.5) == 2, "guard true calls the native");
        check((int) m2.invoke(o, 4.5, 3, "a") == 2, "wide guard true calls the native");
        System.out.println("ArgsGuardCheck ok");
    }
}
