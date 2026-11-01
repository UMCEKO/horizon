package dev.horizonlab.javamods.plugin;

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
import org.objectweb.asm.tree.AnnotationNode;
import org.objectweb.asm.tree.ClassNode;
import org.objectweb.asm.tree.FieldInsnNode;
import org.objectweb.asm.tree.InsnNode;
import org.objectweb.asm.tree.JumpInsnNode;
import org.objectweb.asm.tree.LabelNode;
import org.objectweb.asm.tree.LdcInsnNode;
import org.objectweb.asm.tree.LineNumberNode;
import org.objectweb.asm.tree.LookupSwitchInsnNode;
import org.objectweb.asm.tree.MethodInsnNode;
import org.objectweb.asm.tree.MethodNode;
import org.objectweb.asm.tree.FrameNode;
import org.objectweb.asm.tree.TableSwitchInsnNode;
import org.objectweb.asm.tree.TypeInsnNode;
import org.objectweb.asm.tree.VarInsnNode;
import org.objectweb.asm.tree.analysis.Analyzer;
import org.objectweb.asm.tree.analysis.AnalyzerException;
import org.objectweb.asm.tree.analysis.BasicInterpreter;
import org.objectweb.asm.tree.analysis.BasicValue;
import org.objectweb.asm.tree.analysis.Frame;

/** Views are built without their constructors, so this keeps what mods added to those as `horizon$init$N` methods for `Inits` to call. */
final class ViewInits implements ClassProcessor {
    private static final Set<String> VIEWS = Set.of(
            "net/minecraft/client/Minecraft",
            "net/minecraft/client/gui/Gui",
            "net/minecraft/client/server/IntegratedServer",
            "net/minecraft/client/server/IntegratedPlayerList",
            "net/minecraft/server/MinecraftServer",
            "net/minecraft/server/dedicated/DedicatedServer",
            "net/minecraft/server/dedicated/DedicatedPlayerList",
            "net/minecraft/server/level/ChunkMap",
            "net/minecraft/server/level/ServerChunkCache",
            "net/minecraft/server/level/ServerLevel",
            "net/minecraft/server/level/ServerPlayer",
            "net/minecraft/server/network/ServerGamePacketListenerImpl",
            "net/minecraft/server/players/PlayerList");
    private static final String CALLBACK = "org/spongepowered/asm/mixin/injection/callback/CallbackInfo";
    private static final Map<String, Set<String>> ORIGINAL_FIELDS = new HashMap<>();

    private final boolean before;

    ViewInits(boolean before) {
        this.before = before;
    }

    @Override
    public ProcessorName name() {
        return new ProcessorName("horizon_java", before ? "view_fields" : "view_inits");
    }

    @Override
    public Set<ProcessorName> runsBefore() {
        return before ? Set.of(ClassProcessorIds.MIXIN) : Set.of();
    }

    @Override
    public Set<ProcessorName> runsAfter() {
        return before ? Set.of() : Set.of(ClassProcessorIds.MIXIN);
    }

    @Override
    public boolean handlesClass(SelectionContext context) {
        return !context.empty() && VIEWS.contains(context.type().getInternalName());
    }

    @Override
    public ComputeFlags processClass(TransformationContext context) {
        ClassNode node = context.node();
        if (before) {
            var fields = new HashSet<String>();
            node.fields.forEach(field -> fields.add(field.name));
            ORIGINAL_FIELDS.put(node.name, fields);
            return ComputeFlags.NO_REWRITE;
        }
        var added = new ArrayList<MethodNode>();
        for (MethodNode constructor : node.methods) {
            if (constructor.name.equals("<init>")) keep(node, constructor, added);
        }
        node.methods.addAll(added);
        return added.isEmpty() ? ComputeFlags.NO_REWRITE : ComputeFlags.COMPUTE_MAXS;
    }

    private static boolean merged(MethodNode method) {
        for (var list : List.of(nonNull(method.visibleAnnotations), nonNull(method.invisibleAnnotations))) {
            for (AnnotationNode annotation : list) {
                if (annotation.desc.endsWith("/MixinMerged;")) return true;
            }
        }
        return false;
    }

    private static <T> List<T> nonNull(List<T> list) {
        return list == null ? List.of() : list;
    }

    private void keep(ClassNode node, MethodNode constructor, List<MethodNode> out) {
        Frame<BasicValue>[] frames;
        try {
            frames = new Analyzer<>(new BasicInterpreter()).analyze(node.name, constructor);
        } catch (AnalyzerException e) {
            System.err.println("Horizon: can't read " + node.name + "'s constructor: " + e.getMessage());
            return;
        }
        Set<String> original = ORIGINAL_FIELDS.getOrDefault(node.name, Set.of());
        AbstractInsnNode[] code = constructor.instructions.toArray();
        for (int i = 0; i < code.length; i++) {
            MethodNode kept = null;
            if (code[i] instanceof MethodInsnNode call && call.owner.equals(node.name)) {
                MethodNode target = node.methods.stream().filter(m -> m.name.equals(call.name) && m.desc.equals(call.desc)).findFirst().orElse(null);
                if (target != null && merged(target) && Type.getReturnType(call.desc) == Type.VOID_TYPE) {
                    kept = handler(node, constructor, call, out.size());
                }
            } else if (code[i] instanceof FieldInsnNode put && put.getOpcode() == Opcodes.PUTFIELD && put.owner.equals(node.name) && !original.contains(put.name)) {
                kept = initializer(node, constructor, code, frames, i, out.size());
            }
            if (kept != null) out.add(kept);
        }
    }

    private static MethodNode method(ClassNode node, MethodNode constructor, int index) {
        return new MethodNode(Opcodes.ACC_PUBLIC | Opcodes.ACC_SYNTHETIC, "horizon$init$" + index, constructor.desc, null, null);
    }

    /** A new call to a mod's injection, with the constructor's arguments where they line up and defaults for captured locals. */
    private static MethodNode handler(ClassNode node, MethodNode constructor, MethodInsnNode call, int index) {
        MethodNode method = method(node, constructor, index);
        Type[] params = Type.getArgumentTypes(constructor.desc);
        if (call.getOpcode() != Opcodes.INVOKESTATIC) method.instructions.add(new VarInsnNode(Opcodes.ALOAD, 0));
        int slot = 1;
        Type[] wanted = Type.getArgumentTypes(call.desc);
        for (int i = 0; i < wanted.length; i++) {
            Type type = wanted[i];
            if (i < params.length && params[i].equals(type)) {
                method.instructions.add(new VarInsnNode(type.getOpcode(Opcodes.ILOAD), slot));
            } else if (type.getDescriptor().equals("L" + CALLBACK + ";")) {
                method.instructions.add(new TypeInsnNode(Opcodes.NEW, CALLBACK));
                method.instructions.add(new InsnNode(Opcodes.DUP));
                method.instructions.add(new LdcInsnNode("<init>"));
                method.instructions.add(new InsnNode(Opcodes.ICONST_0));
                method.instructions.add(new MethodInsnNode(Opcodes.INVOKESPECIAL, CALLBACK, "<init>", "(Ljava/lang/String;Z)V"));
            } else {
                method.instructions.add(new InsnNode(switch (type.getSort()) {
                    case Type.LONG -> Opcodes.LCONST_0;
                    case Type.FLOAT -> Opcodes.FCONST_0;
                    case Type.DOUBLE -> Opcodes.DCONST_0;
                    case Type.OBJECT, Type.ARRAY -> Opcodes.ACONST_NULL;
                    default -> Opcodes.ICONST_0;
                }));
            }
            if (i < params.length) slot += params[i].getSize();
        }
        method.instructions.add(new MethodInsnNode(call.getOpcode(), call.owner, call.name, call.desc, call.itf));
        method.instructions.add(new InsnNode(Opcodes.RETURN));
        return method;
    }

    /** The statement setting a field a mod added, copied when it needs nothing but `this`, the arguments and constants. */
    private static MethodNode initializer(ClassNode node, MethodNode constructor, AbstractInsnNode[] code, Frame<BasicValue>[] frames, int end, int index) {
        int start = end;
        while (start > 0 && (frames[start] == null || frames[start].getStackSize() != 0)) start--;
        int locals = 1;
        for (Type param : Type.getArgumentTypes(constructor.desc)) locals += param.getSize();
        MethodNode method = method(node, constructor, index);
        var labels = new HashMap<LabelNode, LabelNode>();
        for (int i = start; i <= end; i++) {
            if (code[i] instanceof LabelNode label) labels.put(label, new LabelNode());
        }
        for (int i = start; i <= end; i++) {
            AbstractInsnNode insn = code[i];
            if (insn instanceof LabelNode || insn instanceof LineNumberNode || insn instanceof FrameNode) continue;
            boolean branches = insn instanceof JumpInsnNode || insn instanceof TableSwitchInsnNode || insn instanceof LookupSwitchInsnNode;
            if (branches || insn instanceof VarInsnNode local && local.var >= locals) {
                System.err.println("Horizon: can't keep the initializer of " + node.name + "." + ((FieldInsnNode) code[end]).name);
                return null;
            }
            method.instructions.add(insn.clone(labels));
        }
        method.instructions.add(new InsnNode(Opcodes.RETURN));
        return method;
    }
}
