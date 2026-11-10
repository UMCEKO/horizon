package dev.horizonlab.javamods.parity;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import org.objectweb.asm.ClassReader;
import org.objectweb.asm.Type;
import org.objectweb.asm.tree.AnnotationNode;
import org.objectweb.asm.tree.ClassNode;
import org.objectweb.asm.tree.MethodNode;

/** Reads the mixin annotations of class files: `mod config side class file` lines in, one tab-separated handler line out. */
public final class MixinScanner {
    private static final String MIXIN = "Lorg/spongepowered/asm/mixin/Mixin;";
    private static final Map<String, String> KINDS = Map.ofEntries(
        Map.entry("Inject", "inject"), Map.entry("Redirect", "redirect"), Map.entry("ModifyArg", "modify_arg"),
        Map.entry("ModifyArgs", "modify_args"), Map.entry("ModifyVariable", "modify_variable"),
        Map.entry("ModifyConstant", "modify_constant"), Map.entry("ModifyExpressionValue", "modify_expression_value"),
        Map.entry("ModifyReceiver", "modify_receiver"), Map.entry("ModifyReturnValue", "modify_return_value"),
        Map.entry("WrapOperation", "wrap_operation"), Map.entry("WrapWithCondition", "wrap_with_condition"),
        Map.entry("WrapMethod", "wrap_method"), Map.entry("Overwrite", "overwrite"),
        Map.entry("Accessor", "accessor"), Map.entry("Invoker", "invoker"));

    private MixinScanner() {}

    public static void main(String[] args) throws IOException {
        var out = new StringBuilder();
        for (String line : Files.readAllLines(Path.of(args[0]))) {
            String[] f = line.split("\t", -1);
            out.append(scan(f[0], f[1], f[2], Files.readAllBytes(Path.of(f[4]))));
        }
        System.out.print(out);
    }

    public static String scan(String mod, String config, String side, byte[] bytes) {
        var node = new ClassNode();
        new ClassReader(bytes).accept(node, ClassReader.SKIP_CODE | ClassReader.SKIP_FRAMES);
        List<String> targets = new ArrayList<>();
        for (AnnotationNode a : annotations(node.invisibleAnnotations, node.visibleAnnotations)) {
            if (!MIXIN.equals(a.desc)) continue;
            for (Object v : list(a, "value")) targets.add(((Type) v).getClassName());
            for (Object v : list(a, "targets")) targets.add(((String) v).replace('/', '.'));
        }
        var out = new StringBuilder();
        for (MethodNode m : node.methods) {
            for (AnnotationNode a : annotations(m.invisibleAnnotations, m.visibleAnnotations)) {
                String kind = KINDS.get(a.desc.substring(a.desc.lastIndexOf('/') + 1, a.desc.length() - 1));
                if (kind == null) continue;
                List<String> methods = new ArrayList<>();
                for (Object v : list(a, "method")) methods.add((String) v);
                if (kind.equals("overwrite")) methods.add(m.name);
                for (Object v : list(a, "value")) if (v instanceof String s) methods.add(s);
                List<String> at = new ArrayList<>();
                for (Object v : list(a, "at")) if (v instanceof AnnotationNode n) at.add(at(n));
                for (Object v : list(a, "constant")) if (v instanceof AnnotationNode n) at.add("CONSTANT:" + render(n.values));
                out.append(String.join("\t", mod, config, side, node.name.replace('/', '.'), m.name, kind,
                    String.join(",", targets), String.join(",", methods), String.join("|", at))).append('\n');
            }
        }
        return out.toString();
    }

    @SafeVarargs
    private static List<AnnotationNode> annotations(List<AnnotationNode>... lists) {
        var all = new ArrayList<AnnotationNode>();
        for (var l : lists) if (l != null) all.addAll(l);
        return all;
    }

    private static String render(List<Object> values) {
        var parts = new ArrayList<String>();
        if (values != null) for (int i = 0; i + 1 < values.size(); i += 2) parts.add(values.get(i) + "=" + one(values.get(i + 1)));
        return String.join(",", parts);
    }

    private static String one(Object v) {
        if (v instanceof Type t) return t.getDescriptor();
        if (v instanceof String[] e) return e[0] + "." + e[1];
        if (v instanceof AnnotationNode n) return "@" + n.desc;
        if (v instanceof List<?> l) return "[" + String.join(",", l.stream().map(MixinScanner::one).toList()) + "]";
        return String.valueOf(v);
    }

    private static String at(AnnotationNode n) {
        Object value = get(n, "value"), target = get(n, "target");
        return value + (target == null ? "" : ":" + target);
    }

    private static Object get(AnnotationNode a, String key) {
        if (a.values != null) for (int i = 0; i < a.values.size(); i += 2) if (a.values.get(i).equals(key)) return a.values.get(i + 1);
        return null;
    }

    private static List<?> list(AnnotationNode a, String key) {
        Object v = get(a, key);
        return v == null ? List.of() : v instanceof List<?> l ? l : List.of(v);
    }
}
