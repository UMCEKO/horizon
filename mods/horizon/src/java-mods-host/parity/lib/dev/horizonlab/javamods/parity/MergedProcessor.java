package dev.horizonlab.javamods.parity;

import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardOpenOption;
import java.util.List;
import java.util.Set;
import net.neoforged.neoforgespi.transformation.ClassProcessor;
import net.neoforged.neoforgespi.transformation.ClassProcessorIds;
import net.neoforged.neoforgespi.transformation.ProcessorName;
import org.objectweb.asm.tree.AnnotationNode;
import org.objectweb.asm.tree.MethodNode;

/** Logs `class method desc mixin` for every method Mixin merged, because a MethodTiming event names the merged method but not its mixin. */
final class MergedProcessor implements ClassProcessor {
    private static final Path FILE = System.getProperty("horizon.parity.merged") == null ? null : Path.of(System.getProperty("horizon.parity.merged"));

    @Override
    public ProcessorName name() {
        return new ProcessorName("horizon_parity", "merged");
    }

    @Override
    public Set<ProcessorName> runsAfter() {
        return Set.of(ClassProcessorIds.MIXIN);
    }

    @Override
    public boolean handlesClass(SelectionContext context) {
        return FILE != null && !context.empty();
    }

    @Override
    public ComputeFlags processClass(TransformationContext context) {
        var node = context.node();
        var out = new StringBuilder();
        for (MethodNode method : node.methods) {
            for (var list : List.of(orEmpty(method.visibleAnnotations), orEmpty(method.invisibleAnnotations))) {
                for (AnnotationNode a : list) {
                    if (!a.desc.endsWith("/MixinMerged;")) continue;
                    String mixin = "";
                    for (int i = 0; a.values != null && i < a.values.size(); i += 2) {
                        if (a.values.get(i).equals("mixin")) mixin = String.valueOf(a.values.get(i + 1));
                    }
                    out.append(node.name.replace('/', '.')).append('\t').append(method.name).append('\t').append(method.desc).append('\t').append(mixin).append('\n');
                }
            }
        }
        if (!out.isEmpty()) append(out.toString());
        return ComputeFlags.NO_REWRITE;
    }

    private static synchronized void append(String text) {
        try {
            Files.writeString(FILE, text, StandardOpenOption.CREATE, StandardOpenOption.APPEND);
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    private static <T> List<T> orEmpty(List<T> list) {
        return list == null ? List.of() : list;
    }
}
