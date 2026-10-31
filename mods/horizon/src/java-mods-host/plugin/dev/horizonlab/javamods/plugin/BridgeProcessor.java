package dev.horizonlab.javamods.plugin;

import dev.horizonlab.javamods.host.Host;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import net.neoforged.neoforgespi.transformation.ClassProcessor;
import net.neoforged.neoforgespi.transformation.ClassProcessorIds;
import net.neoforged.neoforgespi.transformation.ProcessorName;
import org.objectweb.asm.Opcodes;
import org.objectweb.asm.Type;
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

    private record Entry(String owner, String method, String desc, String guard, String nativeName, String[] args, String ret) {
        /** Where the native is declared: interfaces can't hold natives, so a guarded method's goes on the guard's class. */
        String holder() {
            return guard.isEmpty() ? owner : guard;
        }

        String rawDesc() {
            var desc = new StringBuilder("(");
            for (String arg : args) desc.append(arg.equals("-") ? "B" : arg);
            return desc.append(')').append(ret).toString();
        }
    }

    private final Map<String, List<Entry>> byClass = new HashMap<>();
    private final Map<String, List<Entry>> byHolder = new HashMap<>();

    BridgeProcessor() {
        for (String line : Host.bridges) {
            String[] f = line.split("\t", -1);
            String[] args = f[5].isEmpty() ? new String[0] : f[5].split(",");
            var entry = new Entry(f[0], f[1], f[2], f[3], f[4], args, f[6]);
            byClass.computeIfAbsent(entry.owner, c -> new ArrayList<>()).add(entry);
            byHolder.computeIfAbsent(entry.holder(), c -> new ArrayList<>()).add(entry);
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
        for (Entry entry : byClass.getOrDefault(node.name, List.of())) {
            MethodNode method = node.methods.stream().filter(m -> m.name.equals(entry.method) && m.desc.equals(entry.desc)).findFirst().orElse(null);
            if (method == null) {
                System.err.println("Horizon: no method to bridge: " + node.name + "." + entry.method + entry.desc);
                continue;
            }
            bridge(node, method, entry);
        }
        List<Entry> natives = byHolder.getOrDefault(node.name, List.of());
        for (Entry entry : natives) {
            node.methods.add(new MethodNode(Opcodes.ACC_PUBLIC | Opcodes.ACC_STATIC | Opcodes.ACC_NATIVE | Opcodes.ACC_SYNTHETIC, entry.nativeName, entry.rawDesc(), null, null));
        }
        if (!natives.isEmpty()) link(node);
        return ComputeFlags.COMPUTE_MAXS;
    }

    private static void bridge(ClassNode node, MethodNode method, Entry entry) {
        boolean isStatic = (method.access & Opcodes.ACC_STATIC) != 0;
        Type[] params = Type.getArgumentTypes(method.desc);
        Type result = Type.getReturnType(method.desc);
        InsnList head = new InsnList();
        LabelNode body = new LabelNode();
        if (!isStatic && !entry.guard.isEmpty()) {
            head.add(new VarInsnNode(Opcodes.ALOAD, 0));
            head.add(new TypeInsnNode(Opcodes.INSTANCEOF, entry.guard));
        } else {
            head.add(new FieldInsnNode(Opcodes.GETSTATIC, FLAT, "ON", "Z"));
        }
        head.add(new JumpInsnNode(Opcodes.IFEQ, body));
        int local = isStatic ? 0 : 1;
        int arg = 0;
        if (entry.args.length == params.length + 1) {
            head.add(new VarInsnNode(Opcodes.ALOAD, 0));
            flatten(head, Type.getObjectType(node.name), entry.args[arg++]);
        }
        for (Type param : params) {
            String raw = entry.args[arg++];
            if (raw.equals("-")) {
                head.add(new InsnNode(Opcodes.ICONST_0));
            } else {
                head.add(new VarInsnNode(param.getOpcode(Opcodes.ILOAD), local));
                if (param.getSort() >= Type.ARRAY && !raw.equals(param.getDescriptor())) flatten(head, param, raw);
            }
            local += param.getSize();
        }
        head.add(new MethodInsnNode(Opcodes.INVOKESTATIC, entry.holder(), entry.nativeName, entry.rawDesc(), false));
        if (result.getSort() >= Type.ARRAY && !entry.ret.equals(result.getDescriptor())) {
            String simple = result.getClassName().substring(Math.max(result.getClassName().lastIndexOf('.'), result.getClassName().lastIndexOf('$')) + 1);
            head.add(new MethodInsnNode(Opcodes.INVOKESTATIC, FLAT, "to" + simple, "(" + entry.ret + ")" + result.getDescriptor(), false));
        }
        head.add(new InsnNode(result.getOpcode(Opcodes.IRETURN)));
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

    private static void flatten(InsnList head, Type type, String raw) {
        head.add(new MethodInsnNode(Opcodes.INVOKESTATIC, FLAT, "raw", "(" + type.getDescriptor() + ")" + raw, false));
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
