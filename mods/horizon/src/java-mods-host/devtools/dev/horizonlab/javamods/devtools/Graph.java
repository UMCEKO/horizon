import java.lang.classfile.*;
import java.lang.classfile.instruction.InvokeInstruction;
import java.nio.file.*;
import java.util.*;
import java.util.zip.*;

/** Sound sinks reachable from the Java client's tick in the vanilla client jar (class hierarchy dispatch, entity calls narrowed to LocalPlayer); tools/java_sound_sources.py. */
public class Graph {
    record M(String owner, String name, String desc) { public String toString() { return owner + "#" + name; } }
    static Map<String, String> sup = new HashMap<>();
    static Map<String, List<String>> ifaces = new HashMap<>();
    static Map<String, Set<String>> declared = new HashMap<>();
    static Map<M, List<M>> calls = new HashMap<>();
    static Map<String, List<String>> subs = new HashMap<>();

    public static void main(String[] a) throws Exception {
        try (var zip = new ZipFile(a[0])) {
            for (var en : Collections.list(zip.entries())) {
                if (!en.getName().endsWith(".class")) continue;
                ClassModel cm = ClassFile.of().parse(zip.getInputStream(en).readAllBytes());
                String name = cm.thisClass().asInternalName();
                cm.superclass().ifPresent(s -> { sup.put(name, s.asInternalName()); subs.computeIfAbsent(s.asInternalName(), k -> new ArrayList<>()).add(name); });
                List<String> is = new ArrayList<>();
                for (var i : cm.interfaces()) { is.add(i.asInternalName()); subs.computeIfAbsent(i.asInternalName(), k -> new ArrayList<>()).add(name); }
                ifaces.put(name, is);
                Set<String> d = new HashSet<>();
                for (MethodModel mm : cm.methods()) {
                    String key = mm.methodName().stringValue() + mm.methodType().stringValue();
                    d.add(key);
                    List<M> out = new ArrayList<>();
                    mm.code().ifPresent(code -> { for (var el : code) if (el instanceof InvokeInstruction ii) out.add(new M(ii.owner().asInternalName(), ii.name().stringValue(), ii.type().stringValue())); });
                    // lambdas: invokedynamic bootstrap args pointing at synthetic methods
                    mm.code().ifPresent(code -> { for (var el : code) if (el instanceof java.lang.classfile.instruction.InvokeDynamicInstruction id) for (var arg : id.bootstrapArgs()) if (arg instanceof java.lang.constant.DirectMethodHandleDesc h) out.add(new M(h.owner().descriptorString().substring(1, h.owner().descriptorString().length() - 1), h.methodName(), h.lookupDescriptor())); });
                    calls.put(new M(name, mm.methodName().stringValue(), mm.methodType().stringValue()), out);
                }
                declared.put(name, d);
            }
        }
        Set<String> cut = new HashSet<>(Arrays.asList(a[1].split(",")));
        Set<String> sinks = new HashSet<>(Arrays.asList(a[2].split(",")));
        Deque<M> work = new ArrayDeque<>();
        Map<M, M> from = new HashMap<>();
        for (String e : a[3].split(",")) { String[] p = e.split("#"); for (String k : declared.getOrDefault(p[0], Set.of())) if (k.startsWith(p[1] + "(")) { M m = new M(p[0], p[1], k.substring(p[1].length())); from.put(m, null); work.add(m); } }
        Set<M> hits = new LinkedHashSet<>();
        while (!work.isEmpty()) {
            M m = work.poll();
            for (M c : calls.getOrDefault(m, List.of())) {
                for (M t : resolve(c)) {
                    String id = t.owner + "#" + t.name;
                    if (cut.contains(id) || from.containsKey(t)) continue;
                    from.put(t, m);
                    if (sinks.contains(id)) { hits.add(t); continue; }
                    if (!t.owner.startsWith("net/minecraft/") && !t.owner.startsWith("com/mojang/")) continue;
                    work.add(t);
                }
            }
        }
        // print the caller just before each sink path, grouped by the first "source" frame
        Map<String, Integer> sources = new TreeMap<>();
        for (M m : from.keySet()) {
            for (M c : calls.getOrDefault(m, List.of())) for (M t : resolve(c)) if (hits.contains(t) && from.containsKey(m)) {
                List<String> path = new ArrayList<>();
                for (M p = m; p != null && path.size() < 14; p = from.get(p)) path.add(p.toString());
                sources.merge(t + " <- " + String.join(" <- ", path), 1, Integer::sum);
            }
        }
        sources.keySet().forEach(System.out::println);
    }

    static final String LOCAL = "net/minecraft/client/player/LocalPlayer";
    static List<M> resolve(M c) {
        List<M> out = new ArrayList<>();
        // the declared target or the nearest ancestor that declares it
        M base = up(c.owner, c);
        if (base != null) out.add(base);
        if (isAncestor(c.owner, LOCAL)) { M l = up(LOCAL, c); if (l != null && !out.contains(l)) out.add(l); return out; }
        Deque<String> q = new ArrayDeque<>(subs.getOrDefault(c.owner, List.of()));
        Set<String> seen = new HashSet<>();
        while (!q.isEmpty()) { String s = q.poll(); if (!seen.add(s)) continue; if (declared.getOrDefault(s, Set.of()).contains(c.name + c.desc)) out.add(new M(s, c.name, c.desc)); q.addAll(subs.getOrDefault(s, List.of())); }
        return out;
    }
    static M up(String cls, M c) { for (String k = cls; k != null; k = sup.get(k)) if (declared.getOrDefault(k, Set.of()).contains(c.name + c.desc)) return new M(k, c.name, c.desc); return null; }
    static boolean isAncestor(String a, String b) { for (String k = b; k != null; k = sup.get(k)) if (k.equals(a)) return true; return false; }
}
