package dev.horizonlab.javamods.plugin;

import dev.horizonlab.javamods.host.Host;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.List;
import java.util.Map;
import java.util.Set;
import net.neoforged.neoforgespi.transformation.ClassProcessor;
import net.neoforged.neoforgespi.transformation.ClassProcessorIds;
import net.neoforged.neoforgespi.transformation.ProcessorName;
import org.objectweb.asm.Opcodes;
import org.objectweb.asm.Type;
import org.objectweb.asm.tree.AbstractInsnNode;
import org.objectweb.asm.tree.ClassNode;
import org.objectweb.asm.tree.FieldInsnNode;
import org.objectweb.asm.tree.FrameNode;
import org.objectweb.asm.tree.InsnList;
import org.objectweb.asm.tree.InsnNode;
import org.objectweb.asm.tree.JumpInsnNode;
import org.objectweb.asm.tree.LabelNode;
import org.objectweb.asm.tree.LdcInsnNode;
import org.objectweb.asm.tree.MethodInsnNode;
import org.objectweb.asm.tree.MethodNode;
import org.objectweb.asm.tree.TypeInsnNode;
import org.objectweb.asm.tree.VarInsnNode;

/** Gives each method in Rust's table a first step that jumps to its native; runs before Mixin, so mods' HEAD and RETURN injections still apply. */
final class BridgeProcessor implements ClassProcessor {
    private static final String FLAT = "dev/horizonlab/javamods/boot/Flat";
    /** A backed view's bridge settles the groups its native wrote here, after the native returns. */
    private static final String VIEWS = "dev/horizonlab/javamods/boot/EntityViews";

    private static final String ROUTING = "dev/horizonlab/javamods/boot/Routing";
    /** W5's class; the unit check points it at a stand-in. */
    static String guiRoutes = "dev/horizonlab/javamods/boot/GuiRoutes";

    /** `routing.tsv`'s row for one method: `tail` and `returns` decide whether a stub checks for an early return. */
    record Routed(boolean body, boolean tail, int returns) {}

    record Entry(String owner, String method, String desc, String guard, String nativeName, String[] args, String ret, boolean route, String seam) {
        boolean java() {
            return !seam.isEmpty();
        }

        /** Where the native is declared: interfaces can't hold natives, so a guarded method's goes on the guard's class. */
        String holder() {
            if (guard.startsWith("@")) return guard.substring(1, guard.lastIndexOf('.'));
            return guard.isEmpty() ? owner : guard;
        }

        String rawDesc() {
            var desc = new StringBuilder("(");
            for (String arg : args) desc.append(arg.equals("-") ? "B" : rawOf(arg));
            return desc.append(')').append(ret).toString();
        }
    }

    private final Map<String, List<Entry>> byClass = new HashMap<>();
    private final Map<String, List<Entry>> byHolder = new HashMap<>();
    private final Map<String, Routed> routed = readRouting();
    private final Set<String> told = new HashSet<>();

    BridgeProcessor() {
        for (String line : Host.bridges) {
            String[] f = line.split("\t", -1);
            String[] args = f[5].isEmpty() ? new String[0] : f[5].split(",");
            var entry = new Entry(f[0], f[1], f[2], f[3], f[4], args, f[6], f.length > 7 && f[7].equals("route"), "");
            byClass.computeIfAbsent(entry.owner, c -> new ArrayList<>()).add(entry);
            byHolder.computeIfAbsent(entry.holder(), c -> new ArrayList<>()).add(entry);
        }
        readGuiBridges();
    }

    /** The install's routes (`routing.tsv` beside the analysis), keyed `owner.name desc`; none without an analysis. */
    private static Map<String, Routed> readRouting() {
        var out = new HashMap<String, Routed>();
        String analysis = System.getProperty("horizon.java.analysis");
        if (analysis == null || analysis.isEmpty()) return out;
        Path file = Path.of(analysis).resolveSibling("routing.tsv");
        if (!Files.isRegularFile(file)) return out;
        try {
            for (String line : Files.readAllLines(file)) {
                String[] f = line.split("\t", -1);
                if (f.length < 6) continue;
                out.put(f[0] + "." + f[1] + f[2], new Routed(f[3].equals("body"), f[4].equals("1"), Integer.parseInt(f[5])));
            }
        } catch (java.io.IOException e) {
            throw new IllegalStateException(file + ": " + e, e);
        }
        return out;
    }

    /** `gui_bridges.tsv` (key `owner#name` + descriptor, seam): methods whose body is Java code, in the plugin jar. */
    private void readGuiBridges() {
        try (var in = BridgeProcessor.class.getClassLoader().getResourceAsStream("gui_bridges.tsv")) {
            if (in == null) return;
            for (String line : new String(in.readAllBytes(), java.nio.charset.StandardCharsets.UTF_8).split("\n")) {
                String[] f = line.split("\t", -1);
                if (f.length < 2 || f[0].isEmpty()) continue;
                int hash = f[0].indexOf('#'), paren = f[0].indexOf('(');
                String owner = f[0].substring(0, hash), name = f[0].substring(hash + 1, paren), desc = f[0].substring(paren);
                var entry = new Entry(owner, name, desc, "", "", new String[0], "", true, f[1]);
                byClass.computeIfAbsent(owner, c -> new ArrayList<>()).add(entry);
            }
        } catch (java.io.IOException e) {
            throw new IllegalStateException("gui_bridges.tsv: " + e, e);
        }
    }

    @Override
    public ProcessorName name() {
        return new ProcessorName("horizon_java", "bridge");
    }

    @Override
    public Set<ProcessorName> runsBefore() {
        return Set.of(ClassProcessorIds.MIXIN);
    }

    @Override
    public boolean handlesClass(SelectionContext context) {
        String name = context.type().getInternalName();
        return !context.empty() && (byClass.containsKey(name) || byHolder.containsKey(name));
    }

    @Override
    public ComputeFlags processClass(TransformationContext context) {
        ClassNode node = context.node();
        boolean stubbed = false;
        for (Entry entry : byClass.getOrDefault(node.name, List.of())) {
            MethodNode method = node.methods.stream().filter(m -> m.name.equals(entry.method) && m.desc.equals(entry.desc)).findFirst().orElse(null);
            if (method == null) {
                System.err.println("Horizon: no method to bridge: " + node.name + "." + entry.method + entry.desc);
                continue;
            }
            Routed route = routed.get(entry.owner + "." + entry.method + entry.desc);
            if (route != null && route.body) {
                if (told.add(entry.owner + "." + entry.method)) System.out.println("Horizon: " + entry.owner + "." + entry.method + " runs in Java: a mod patches inside its body");
            } else if (entry.java() && (method.access & Opcodes.ACC_STATIC) != 0) {
                System.err.println("Horizon: " + entry.owner + "." + entry.method + " is static: no GUI bridge");
            } else if (entry.route && !(entry.java() && route == null)) {
                stubbed |= routeStub(node, method, entry, route != null && route.tail && route.returns > 1);
            } else {
                bridge(node, method, entry);
            }
        }
        List<Entry> natives = byHolder.getOrDefault(node.name, List.of());
        for (Entry entry : natives) {
            node.methods.add(new MethodNode(Opcodes.ACC_PUBLIC | Opcodes.ACC_STATIC | Opcodes.ACC_NATIVE | Opcodes.ACC_SYNTHETIC, entry.nativeName, entry.rawDesc(), null, null));
        }
        if (!natives.isEmpty()) link(node);
        return stubbed ? ComputeFlags.COMPUTE_FRAMES : ComputeFlags.COMPUTE_MAXS;
    }

    /** The native call with its arguments, behind the guard test that jumps to `body` when the receiver isn't backed; leaves the result on the stack. */
    private static InsnList nativeCall(ClassNode node, MethodNode method, Entry entry, LabelNode body) {
        boolean isStatic = (method.access & Opcodes.ACC_STATIC) != 0;
        Type[] params = Type.getArgumentTypes(method.desc);
        Type result = Type.getReturnType(method.desc);
        InsnList head = new InsnList();
        if (entry.java()) {
            head.add(new VarInsnNode(Opcodes.ALOAD, 0));
            head.add(new MethodInsnNode(Opcodes.INVOKESTATIC, guiRoutes, "backed", "(Ljava/lang/Object;)Z", false));
        } else if (!isStatic && entry.guard.startsWith("@")) {
            int dot = entry.guard.lastIndexOf('.');
            head.add(new VarInsnNode(Opcodes.ALOAD, 0));
            head.add(new MethodInsnNode(Opcodes.INVOKESTATIC, entry.guard.substring(1, dot), entry.guard.substring(dot + 1), "(Ljava/lang/Object;)Z", false));
        } else if (!isStatic && !entry.guard.isEmpty()) {
            head.add(new VarInsnNode(Opcodes.ALOAD, 0));
            head.add(new TypeInsnNode(Opcodes.INSTANCEOF, entry.guard));
        } else {
            head.add(new FieldInsnNode(Opcodes.GETSTATIC, FLAT, "ON", "Z"));
        }
        head.add(new JumpInsnNode(Opcodes.IFEQ, body));
        if (entry.java()) {
            javaBody(head, method, entry);
            return head;
        }
        int local = isStatic ? 0 : 1;
        int arg = 0;
        if (entry.args.length == params.length + 1) {
            head.add(new VarInsnNode(Opcodes.ALOAD, 0));
            flatten(head, Type.getObjectType(entry.guard.startsWith("@") ? "net/minecraft/world/entity/Entity" : node.name), entry.args[arg++]);
        }
        for (Type param : params) {
            String flat = entry.args[arg++];
            String raw = rawOf(flat);
            if (raw.equals("-")) {
                head.add(new InsnNode(Opcodes.ICONST_0));
            } else {
                head.add(new VarInsnNode(param.getOpcode(Opcodes.ILOAD), local));
                if (param.getSort() >= Type.ARRAY && !raw.equals(param.getDescriptor())) flatten(head, param, flat);
            }
            local += param.getSize();
        }
        head.add(new MethodInsnNode(Opcodes.INVOKESTATIC, entry.holder(), entry.nativeName, entry.rawDesc(), false));
        if (entry.guard.equals("@" + VIEWS + ".backed")) head.add(new MethodInsnNode(Opcodes.INVOKESTATIC, VIEWS, "settle", "()V", false));
        if (result.getSort() >= Type.ARRAY && !entry.ret.equals(result.getDescriptor())) {
            String simple = result.getClassName().substring(Math.max(result.getClassName().lastIndexOf('.'), result.getClassName().lastIndexOf('$')) + 1);
            head.add(new MethodInsnNode(Opcodes.INVOKESTATIC, FLAT, "to" + simple, "(" + entry.ret + ")" + result.getDescriptor(), false));
        }
        return head;
    }

    /** `GuiRoutes.body(seam, this, boxed arguments)`, its result cast or unboxed to the method's return type. */
    private static void javaBody(InsnList head, MethodNode method, Entry entry) {
        Type[] params = Type.getArgumentTypes(method.desc);
        Type result = Type.getReturnType(method.desc);
        head.add(new LdcInsnNode(entry.seam));
        head.add(new VarInsnNode(Opcodes.ALOAD, 0));
        head.add(new LdcInsnNode(params.length));
        head.add(new TypeInsnNode(Opcodes.ANEWARRAY, "java/lang/Object"));
        int local = 1;
        for (int i = 0; i < params.length; i++) {
            head.add(new InsnNode(Opcodes.DUP));
            head.add(new LdcInsnNode(i));
            head.add(new VarInsnNode(params[i].getOpcode(Opcodes.ILOAD), local));
            if (params[i].getSort() < Type.ARRAY) {
                String box = BOXES[params[i].getSort()];
                head.add(new MethodInsnNode(Opcodes.INVOKESTATIC, box, "valueOf", "(" + params[i].getDescriptor() + ")L" + box + ";", false));
            }
            head.add(new InsnNode(Opcodes.AASTORE));
            local += params[i].getSize();
        }
        head.add(new MethodInsnNode(Opcodes.INVOKESTATIC, guiRoutes, "body", "(Ljava/lang/String;Ljava/lang/Object;[Ljava/lang/Object;)Ljava/lang/Object;", false));
        switch (result.getSort()) {
            case Type.VOID -> head.add(new InsnNode(Opcodes.POP));
            case Type.ARRAY, Type.OBJECT -> {
                if (!result.getInternalName().equals("java/lang/Object")) head.add(new TypeInsnNode(Opcodes.CHECKCAST, result.getInternalName()));
            }
            default -> {
                String box = BOXES[result.getSort()];
                head.add(new TypeInsnNode(Opcodes.CHECKCAST, box));
                head.add(new MethodInsnNode(Opcodes.INVOKEVIRTUAL, box, UNBOX[result.getSort()], "()" + result.getDescriptor(), false));
            }
        }
    }

    private static final String[] BOXES = {null, "java/lang/Boolean", "java/lang/Character", "java/lang/Byte", "java/lang/Short", "java/lang/Integer", "java/lang/Float", "java/lang/Long", "java/lang/Double"};
    private static final String[] UNBOX = {null, "booleanValue", "charValue", "byteValue", "shortValue", "intValue", "floatValue", "longValue", "doubleValue"};

    /** Makes the method's native its body, for Mixin's handlers to surround; see docs/java-mods-map.md. False when the method has no return to share, so the plain bridge went in. */
    static boolean routeStub(ClassNode node, MethodNode method, Entry entry, boolean earlyCheck) {
        return routeStub(node, method, entry, earlyCheck, ROUTING, entry.java() ? "earlyJava" : "early");
    }

    static boolean routeStub(ClassNode node, MethodNode method, Entry entry, boolean earlyCheck, String earlyOwner, String earlyName) {
        AbstractInsnNode last = null;
        for (var insn = method.instructions.getFirst(); insn != null; insn = insn.getNext()) {
            if (insn.getOpcode() >= Opcodes.IRETURN && insn.getOpcode() <= Opcodes.RETURN) last = insn;
        }
        if (last == null) {
            System.err.println("Horizon: " + node.name + "." + method.name + " never returns: bridged without a route stub");
            bridge(node, method, entry);
            return false;
        }
        Type result = Type.getReturnType(method.desc);
        boolean value = result.getSort() != Type.VOID;
        int slot = method.maxLocals;
        method.maxLocals += result.getSize();
        LabelNode body = new LabelNode();
        LabelNode join = new LabelNode();
        LabelNode tail = new LabelNode();
        InsnList head = nativeCall(node, method, entry, body);
        if (value) head.add(new VarInsnNode(result.getOpcode(Opcodes.ISTORE), slot));
        if (earlyCheck) {
            head.add(new MethodInsnNode(Opcodes.INVOKESTATIC, earlyOwner, earlyName, "()Z", false));
            head.add(new JumpInsnNode(Opcodes.IFEQ, join));
            if (value) head.add(new VarInsnNode(result.getOpcode(Opcodes.ILOAD), slot));
            head.add(new InsnNode(result.getOpcode(Opcodes.IRETURN)));
        }
        head.add(join);
        if (value) head.add(new VarInsnNode(result.getOpcode(Opcodes.ILOAD), slot));
        head.add(new JumpInsnNode(Opcodes.GOTO, tail));
        head.add(body);
        method.instructions.set(last, new JumpInsnNode(Opcodes.GOTO, tail));
        method.instructions.insert(head);
        method.instructions.add(tail);
        method.instructions.add(new InsnNode(result.getOpcode(Opcodes.IRETURN)));
        return true;
    }

    private static void bridge(ClassNode node, MethodNode method, Entry entry) {
        boolean isStatic = (method.access & Opcodes.ACC_STATIC) != 0;
        Type[] params = Type.getArgumentTypes(method.desc);
        LabelNode body = new LabelNode();
        InsnList head = nativeCall(node, method, entry, body);
        head.add(new InsnNode(Type.getReturnType(method.desc).getOpcode(Opcodes.IRETURN)));
        head.add(body);
        if (!startsWithFrame(method)) head.add(frame(node, isStatic, params));
        method.instructions.insert(head);
    }

    private static boolean startsWithFrame(MethodNode method) {
        for (var insn = method.instructions.getFirst(); insn != null; insn = insn.getNext()) {
            if (insn.getOpcode() >= 0) return false;
            if (insn instanceof FrameNode) return true;
        }
        return false;
    }

    /** An argument's JNI type: `name:type` names the `Flat` method that flattens it, else it is `raw`. */
    static String rawOf(String arg) {
        return arg.substring(arg.indexOf(':') + 1);
    }

    private static void flatten(InsnList head, Type type, String arg) {
        int split = arg.indexOf(':');
        String name = split < 0 ? "raw" : arg.substring(0, split);
        head.add(new MethodInsnNode(Opcodes.INVOKESTATIC, FLAT, name, "(" + type.getDescriptor() + ")" + rawOf(arg), false));
    }

    /** The method's arguments as they stand at its first instruction. */
    private static FrameNode frame(ClassNode node, boolean isStatic, Type[] params) {
        List<Object> locals = new ArrayList<>();
        if (!isStatic) locals.add(node.name);
        for (Type param : params) {
            locals.add(switch (param.getSort()) {
                case Type.BOOLEAN, Type.BYTE, Type.CHAR, Type.SHORT, Type.INT -> Opcodes.INTEGER;
                case Type.FLOAT -> Opcodes.FLOAT;
                case Type.LONG -> Opcodes.LONG;
                case Type.DOUBLE -> Opcodes.DOUBLE;
                default -> param.getInternalName();
            });
        }
        return new FrameNode(Opcodes.F_NEW, locals.size(), locals.toArray(), 0, new Object[0]);
    }

    private static void link(ClassNode node) {
        MethodNode init = node.methods.stream().filter(m -> m.name.equals("<clinit>")).findFirst().orElse(null);
        if (init == null) {
            init = new MethodNode(Opcodes.ACC_STATIC, "<clinit>", "()V", null, null);
            init.instructions.add(new InsnNode(Opcodes.RETURN));
            node.methods.add(init);
        }
        InsnList first = new InsnList();
        first.add(new LdcInsnNode(Type.getObjectType(node.name)));
        first.add(new MethodInsnNode(Opcodes.INVOKESTATIC, FLAT, "link", "(Ljava/lang/Class;)V", false));
        init.instructions.insert(first);
    }
}
