import java.lang.classfile.*;
import java.lang.classfile.instruction.InvokeDynamicInstruction;
import java.lang.classfile.instruction.InvokeInstruction;
import java.nio.file.*;
import java.util.*;
import java.util.zip.*;

/** Reads the vanilla client jar's class files and prints the methods whose patch changes `ClientLevel.animateTick`'s effects (tools/animate_closure.py). */
public class Closure {
    record M(String owner, String name, String desc) implements Comparable<M> {
        public int compareTo(M o) { return Comparator.comparing(M::owner).thenComparing(M::name).thenComparing(M::desc).compare(this, o); }
    }

    static final String LEVEL = "net/minecraft/client/multiplayer/ClientLevel";
    static final String BLOCK = "net/minecraft/world/level/block/Block";
    static final String FLUID = "net/minecraft/world/level/material/Fluid";
    static final String BLOCK_TICK = "(Lnet/minecraft/world/level/block/state/BlockState;Lnet/minecraft/world/level/Level;Lnet/minecraft/core/BlockPos;Lnet/minecraft/util/RandomSource;)V";
    static final String FLUID_TICK = "(Lnet/minecraft/world/level/Level;Lnet/minecraft/core/BlockPos;Lnet/minecraft/world/level/material/FluidState;Lnet/minecraft/util/RandomSource;)V";
    /** Effect sinks of an `animateTick` implementation, by name, on `ClientLevel`; their own-class callees follow. */
    static final Set<String> SINKS = Set.of("addParticle", "addAlwaysVisibleParticle", "playLocalSound");
    /** Read by the loop, but a patch there is not a patch of the animate code. */
    static final Set<String> LEVEL_PLUMBING = Set.of("getBlockState", "getFluidState", "environmentAttributes");
    static final Set<String> CALLEE_SOURCES = Set.of("doAnimateTick", "trySpawnDripParticles");
    static final Set<String> NOT_LOOP_OWNERS = Set.of(
        "net/minecraft/core/BlockPos", "net/minecraft/core/BlockPos$MutableBlockPos", "net/minecraft/util/RandomSource", "net/minecraft/util/Mth",
        "net/minecraft/core/particles/BlockParticleOption", "net/minecraft/world/phys/shapes/VoxelShape");

    static Map<String, String> sup = new HashMap<>();
    static Map<String, List<String>> ifaces = new HashMap<>();
    static Map<String, Set<String>> declared = new HashMap<>();
    static Map<M, Set<M>> calls = new HashMap<>();

    public static void main(String[] a) throws Exception {
        try (var zip = new ZipFile(a[0])) {
            for (var en : Collections.list(zip.entries())) {
                if (!en.getName().endsWith(".class")) continue;
                ClassModel cm = ClassFile.of().parse(zip.getInputStream(en).readAllBytes());
                String name = cm.thisClass().asInternalName();
                cm.superclass().ifPresent(s -> sup.put(name, s.asInternalName()));
                ifaces.put(name, cm.interfaces().stream().map(i -> i.asInternalName()).toList());
                Set<String> d = new HashSet<>();
                for (MethodModel mm : cm.methods()) {
                    d.add(mm.methodName().stringValue() + mm.methodType().stringValue());
                    Set<M> out = new LinkedHashSet<>();
                    mm.code().ifPresent(code -> {
                        for (var el : code) {
                            if (el instanceof InvokeInstruction ii) out.add(new M(ii.owner().asInternalName(), ii.name().stringValue(), ii.type().stringValue()));
                            if (el instanceof InvokeDynamicInstruction id)
                                for (var arg : id.bootstrapArgs())
                                    if (arg instanceof java.lang.constant.DirectMethodHandleDesc h)
                                        out.add(new M(h.owner().descriptorString().substring(1, h.owner().descriptorString().length() - 1), h.methodName(), h.lookupDescriptor()));
                        }
                    });
                    calls.put(new M(name, mm.methodName().stringValue(), mm.methodType().stringValue()), out);
                }
                declared.put(name, d);
            }
        }
        var rows = new TreeMap<String, String>();
        loop(rows);
        family("block", BLOCK, "animateTick", BLOCK_TICK, rows);
        family("fluid", FLUID, "animateTick", FLUID_TICK, rows);
        rows.values().forEach(System.out::println);
    }

    static boolean vanilla(String owner) { return owner.startsWith("net/minecraft/") || owner.startsWith("com/mojang/"); }

    /** The method a call to `c` runs when its owner is exact: the nearest declaration up the superclasses, then the interfaces. */
    static M resolve(M c) {
        for (String k = c.owner; k != null; k = sup.get(k)) if (declared.getOrDefault(k, Set.of()).contains(c.name + c.desc)) return new M(k, c.name, c.desc);
        Deque<String> q = new ArrayDeque<>(List.of(c.owner));
        Set<String> seen = new HashSet<>();
        while (!q.isEmpty()) {
            String k = q.poll();
            if (k == null || !seen.add(k)) continue;
            if (declared.getOrDefault(k, Set.of()).contains(c.name + c.desc)) return new M(k, c.name, c.desc);
            if (sup.get(k) != null) q.add(sup.get(k));
            q.addAll(ifaces.getOrDefault(k, List.of()));
        }
        return null;
    }

    static void row(Map<String, String> rows, String role, M m, String block) {
        rows.put(role + "\t" + m.owner + "\t" + m.name + "\t" + m.desc, role + "\t" + m.owner + "\t" + m.name + "\t" + m.desc + (role.equals("loop") ? "" : "\t" + block));
    }

    static void loop(Map<String, String> rows) {
        var set = new TreeSet<M>();
        var work = new ArrayDeque<M>();
        for (M m : calls.keySet()) if (m.owner.equals(LEVEL) && m.name.equals("animateTick") && m.desc.equals("(III)V")) { set.add(m); work.add(m); }
        // The loop's own methods: what it calls on `ClientLevel` itself.
        while (!work.isEmpty()) {
            M m = work.poll();
            for (M c : calls.getOrDefault(m, Set.of())) {
                if (!c.owner.equals(LEVEL) || LEVEL_PLUMBING.contains(c.name) || SINKS.contains(c.name)) continue;
                M r = resolve(c);
                if (r != null && r.owner.equals(LEVEL) && set.add(r)) work.add(r);
            }
        }
        var all = new TreeSet<>(set);
        // The sinks: every overload of them on `ClientLevel`, and what those call on it.
        for (M m : calls.keySet()) if (m.owner.equals(LEVEL) && SINKS.contains(m.name)) { all.add(m); work.add(m); }
        while (!work.isEmpty()) {
            M m = work.poll();
            for (M c : calls.getOrDefault(m, Set.of())) {
                if (!c.owner.equals(LEVEL) || LEVEL_PLUMBING.contains(c.name)) continue;
                M r = resolve(c);
                if (r != null && r.owner.equals(LEVEL) && all.add(r)) work.add(r);
            }
        }
        // The vanilla methods the iteration calls, where they are declared.
        for (M m : set) {
            if (!CALLEE_SOURCES.contains(m.name)) continue;
            for (M c : calls.getOrDefault(m, Set.of())) {
                if (c.owner.equals(LEVEL) || !vanilla(c.owner) || NOT_LOOP_OWNERS.contains(c.owner) || c.name.equals("getBlock") || c.name.startsWith("<")) continue;
                M r = resolve(c);
                if (r != null && vanilla(r.owner) && !NOT_LOOP_OWNERS.contains(r.owner) && !r.owner.startsWith("java/")) all.add(r);
            }
        }
        all.add(new M(FLUID, "animateTick", FLUID_TICK));
        for (M m : all) row(rows, "loop", m, "");
    }

    static boolean below(String k, String root) { for (; k != null; k = sup.get(k)) if (k.equals(root)) return true; return false; }

    /** Every class below `root` that declares its `animateTick`, and the methods only those call, each with the classes it serves. */
    static void family(String role, String root, String name, String desc, Map<String, String> rows) {
        Map<M, Set<String>> served = new TreeMap<>();
        for (String k : declared.keySet()) {
            if (k.equals(root) || !below(k, root) || !declared.get(k).contains(name + desc)) continue;
            served.put(new M(k, name, desc), new TreeSet<>(Set.of(k)));
        }
        Map<M, Set<M>> callers = new HashMap<>();
        for (var e : calls.entrySet()) for (M c : e.getValue()) {
            M r = resolve(c);
            if (r != null) callers.computeIfAbsent(r, x -> new HashSet<>()).add(e.getKey());
        }
        // A method also runs when a call names a method it overrides.
        for (String k : declared.keySet()) for (String d : declared.get(k)) {
            int p = d.indexOf('(');
            M self = new M(k, d.substring(0, p), d.substring(p));
            Deque<String> up = new ArrayDeque<>();
            if (sup.get(k) != null) up.add(sup.get(k));
            up.addAll(ifaces.getOrDefault(k, List.of()));
            Set<String> seen = new HashSet<>();
            while (!up.isEmpty()) {
                String u = up.poll();
                if (!seen.add(u)) continue;
                if (declared.getOrDefault(u, Set.of()).contains(d)) callers.computeIfAbsent(self, x -> new HashSet<>()).addAll(callers.getOrDefault(new M(u, self.name, self.desc), Set.of()));
                if (sup.get(u) != null) up.add(sup.get(u));
                up.addAll(ifaces.getOrDefault(u, List.of()));
            }
        }
        boolean grew = true;
        while (grew) {
            grew = false;
            for (M m : new ArrayList<>(served.keySet())) {
                for (M c : calls.getOrDefault(m, Set.of())) {
                    M r = resolve(c);
                    if (r == null || !vanilla(r.owner) || r.name.startsWith("<") || r.owner.startsWith("java/")) continue;
                    if (served.containsKey(r) && served.get(r).containsAll(served.get(m))) continue;
                    Set<M> who = callers.getOrDefault(r, Set.of());
                    if (who.isEmpty() || !served.keySet().containsAll(who) || r.owner.equals(root) || r.owner.equals(LEVEL) || r.owner.equals("net/minecraft/world/level/Level")) continue;
                    var into = served.computeIfAbsent(r, x -> new TreeSet<>());
                    if (into.addAll(served.get(m))) grew = true;
                }
            }
        }
        for (var e : served.entrySet()) row(rows, role, e.getKey(), String.join(",", e.getValue()));
    }
}
