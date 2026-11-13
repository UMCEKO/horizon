import java.lang.classfile.*;
import java.lang.classfile.constantpool.StringEntry;
import java.lang.classfile.instruction.ConstantInstruction;
import java.lang.classfile.instruction.FieldInstruction;
import java.lang.classfile.instruction.InvokeDynamicInstruction;
import java.lang.classfile.instruction.InvokeInstruction;
import java.lang.classfile.instruction.NewObjectInstruction;
import java.lang.constant.DirectMethodHandleDesc;
import java.util.*;
import java.util.stream.Stream;
import java.util.zip.*;

/** Reads the vanilla server jar's class files and prints, per worldgen seam, the vanilla methods a patch of which changes that seam (tools/worldgen_closure.py). */
public class WorldgenClosure {
    record M(String owner, String name, String desc) implements Comparable<M> {
        public int compareTo(M o) { return Comparator.comparing(M::owner).thenComparing(M::name).thenComparing(M::desc).compare(this, o); }
    }

    /** One call site: its named method, whether the JVM picks the body by the receiver's class, and whether it is a lambda or method reference. */
    record Call(M target, boolean virtual, boolean handle) {}

    static final String LG = "net/minecraft/world/level/levelgen/";
    static final String GENERATOR = "net/minecraft/world/level/chunk/ChunkGenerator";
    static final String NOISE = LG + "NoiseBasedChunkGenerator";
    static final String BIOME_SOURCE = "net/minecraft/world/level/biome/BiomeSource";
    static final String FEATURE = LG + "feature/Feature";
    static final String PLACED = LG + "placement/PlacedFeature";
    static final String CARVER = LG + "carver/WorldCarver";
    static final String STRUCTURE = LG + "structure/Structure";
    static final String PIECE = LG + "structure/StructurePiece";
    static final String START = LG + "structure/StructureStart";
    /** Where each typed seam's types are registered, by `Registry.register(registry, "name", Type.CODEC)`. */
    static final Map<String, String> REGISTRARS = Map.of("feature", LG + "feature/FeatureTypes", "carver", LG + "carver/WorldCarverTypes", "structure", LG + "structure/StructureType");
    /** What a generating level and chunk are: calls on the types above these run only their bodies (an `ImposterProtoChunk`'s full chunk is generated already). */
    static final List<String> GENERATING = List.of("net/minecraft/server/level/WorldGenRegion", "net/minecraft/world/level/chunk/ProtoChunk");

    static Map<String, String> sup = new HashMap<>();
    static Map<String, List<String>> ifaces = new HashMap<>();
    static Map<String, List<String>> subs = new HashMap<>();
    static Map<String, Set<String>> declared = new HashMap<>();
    static Set<M> bodies = new HashSet<>();
    static Map<M, List<Call>> calls = new HashMap<>();
    static Map<M, Set<String>> made = new HashMap<>();
    static Map<String, Map<String, String>> types = new HashMap<>();
    /** The whole-stage seams' methods, which the other seams' closures leave to them. */
    static Set<M> stages = new HashSet<>();

    public static void main(String[] a) throws Exception {
        try (var zip = new ZipFile(a[0])) {
            for (var en : Collections.list(zip.entries())) {
                if (!en.getName().endsWith(".class")) continue;
                read(ClassFile.of().parse(zip.getInputStream(en).readAllBytes()));
            }
        }
        var rows = new TreeSet<String>();
        var biomes = closure(biomeRoots(), Map.of());
        var fill = closure(roots(NOISE, "doFill", "net/minecraft/world/level/levelgen/RandomState", "create"), Map.of());
        var surface = closure(roots(NOISE, "buildSurface", LG + "NoiseGeneratorSettings", "materialRule"), Map.of());
        emit(rows, "biomes", "", biomes);
        emit(rows, "fill", "", fill);
        emit(rows, "surface", "", surface);
        emit(rows, "terrain", "", terrain(biomes, fill, surface));
        // A structure's biome check or a feature's height query runs the biome, fill or surface seam, which routes on its own.
        Stream.of(biomes, fill, surface).forEach(stages::addAll);
        emit(rows, "beard", "", closure(roots(LG + "Beardifier", "forStructuresInChunk"), Map.of()));
        emit(rows, "spawn", "", closure(roots(NOISE, "spawnOriginalMobs"), Map.of()));
        for (var t : types.get("carver").entrySet()) emit(rows, "carver", t.getKey(), closure(own(t.getValue(), CARVER, "carve"), Map.of(CARVER, Set.of(t.getValue()))));
        for (var t : types.get("feature").entrySet()) {
            var roots = own(t.getValue(), FEATURE, "place");
            roots.addAll(roots(PLACED, "place"));
            roots.addAll(roots(PLACED, "placeWithBiomeCheck"));
            emit(rows, "feature", t.getKey(), closure(roots, Map.of(FEATURE, Set.of(t.getValue()))));
        }
        for (var t : types.get("structure").entrySet()) structure(rows, t.getKey(), t.getValue());
        rows.forEach(System.out::println);
    }

    static void read(ClassModel cm) {
        String name = cm.thisClass().asInternalName();
        cm.superclass().ifPresent(s -> { sup.put(name, s.asInternalName()); subs.computeIfAbsent(s.asInternalName(), k -> new ArrayList<>()).add(name); });
        var is = new ArrayList<String>();
        for (var i : cm.interfaces()) { is.add(i.asInternalName()); subs.computeIfAbsent(i.asInternalName(), k -> new ArrayList<>()).add(name); }
        ifaces.put(name, is);
        Set<String> d = new HashSet<>();
        String kind = REGISTRARS.entrySet().stream().filter(e -> e.getValue().equals(name)).map(Map.Entry::getKey).findFirst().orElse(null);
        for (MethodModel mm : cm.methods()) {
            d.add(mm.methodName().stringValue() + mm.methodType().stringValue());
            M self = new M(name, mm.methodName().stringValue(), mm.methodType().stringValue());
            List<Call> out = new ArrayList<>();
            Set<String> news = new HashSet<>();
            String[] text = {null};
            mm.code().ifPresent(code -> {
                bodies.add(self);
                for (var el : code) {
                    if (el instanceof InvokeInstruction ii) {
                        boolean virtual = ii.opcode() == Opcode.INVOKEVIRTUAL || ii.opcode() == Opcode.INVOKEINTERFACE;
                        out.add(new Call(new M(ii.owner().asInternalName(), ii.name().stringValue(), ii.type().stringValue()), virtual, false));
                    } else if (el instanceof InvokeDynamicInstruction id) {
                        for (var arg : id.bootstrapArgs()) {
                            if (!(arg instanceof DirectMethodHandleDesc h)) continue;
                            String owner = h.owner().descriptorString().substring(1, h.owner().descriptorString().length() - 1);
                            if (h.kind() == DirectMethodHandleDesc.Kind.CONSTRUCTOR) news.add(owner);
                            boolean virtual = h.kind() == DirectMethodHandleDesc.Kind.VIRTUAL || h.kind() == DirectMethodHandleDesc.Kind.INTERFACE_VIRTUAL;
                            out.add(new Call(new M(owner, h.methodName(), h.lookupDescriptor()), virtual, true));
                        }
                    } else if (el instanceof NewObjectInstruction n) {
                        news.add(n.className().asInternalName());
                    } else if (kind != null && el instanceof ConstantInstruction.LoadConstantInstruction ldc && ldc.constantEntry() instanceof StringEntry s) {
                        text[0] = s.stringValue();
                    } else if (kind != null && text[0] != null && el instanceof FieldInstruction f && f.opcode() == Opcode.GETSTATIC) {
                        types.computeIfAbsent(kind, k -> new TreeMap<>()).put("minecraft:" + text[0], f.owner().asInternalName());
                        text[0] = null;
                    }
                }
            });
            calls.put(self, out);
            if (!news.isEmpty()) made.put(self, news);
        }
        declared.put(name, d);
    }

    static boolean vanilla(String owner) { return owner.startsWith("net/minecraft/"); }

    /** Worldgen's own code; blocks, entities, NBT and the rest are W12's per-call routes, so a mod patching them is routed there. */
    static final List<String> WORLDGEN = List.of(LG, "net/minecraft/world/level/biome/", "net/minecraft/server/level/WorldGenRegion", "net/minecraft/world/level/StructureManager",
        "net/minecraft/world/level/chunk/ChunkGenerator", "net/minecraft/world/level/chunk/ProtoChunk", "net/minecraft/world/level/chunk/CarvingMask");

    /** Noise primitives: a mod patching them is routed by its own seam. */
    static boolean descends(String owner) { return WORLDGEN.stream().anyMatch(owner::startsWith) && !owner.startsWith(LG + "synth/"); }

    static boolean below(String k, String root) {
        if (k == null) return false;
        if (k.equals(root)) return true;
        if (below(sup.get(k), root)) return true;
        for (String i : ifaces.getOrDefault(k, List.of())) if (below(i, root)) return true;
        return false;
    }

    /** The body a call on a receiver of class `cls` runs: the nearest declaration up the superclasses, then the interfaces. */
    static M up(String cls, M c) {
        for (String k = cls; k != null; k = sup.get(k)) if (declared.getOrDefault(k, Set.of()).contains(c.name + c.desc)) return new M(k, c.name, c.desc);
        Deque<String> q = new ArrayDeque<>(List.of(cls));
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

    /** The bodies a call can run: the named one or its nearest ancestor's, and every override below it, unless `narrow` fixes the receiver classes for the call's owner. */
    static Set<M> resolve(Call call, Map<String, Set<String>> narrow) {
        M c = call.target;
        Set<M> out = new LinkedHashSet<>();
        if (!call.virtual) {
            M m = up(c.owner, c);
            if (m != null) out.add(m);
            return out;
        }
        for (var e : narrow.entrySet()) {
            if (!below(e.getKey(), c.owner)) continue;
            for (String k : e.getValue()) if (below(k, c.owner)) { M m = up(k, c); if (m != null) out.add(m); }
            return out;
        }
        for (String g : GENERATING) {
            if (!below(g, c.owner)) continue;
            M m = up(g, c);
            if (m != null) out.add(m);
        }
        if (!out.isEmpty()) return out;
        M base = up(c.owner, c);
        if (base != null) out.add(base);
        overrides(c.owner, c, out);
        return out;
    }

    static void overrides(String cls, M c, Set<M> out) {
        Deque<String> q = new ArrayDeque<>(subs.getOrDefault(cls, List.of()));
        Set<String> seen = new HashSet<>();
        while (!q.isEmpty()) {
            String s = q.poll();
            if (!seen.add(s)) continue;
            if (declared.getOrDefault(s, Set.of()).contains(c.name + c.desc)) out.add(new M(s, c.name, c.desc));
            q.addAll(subs.getOrDefault(s, List.of()));
        }
    }

    /** The vanilla methods with a body reachable from `roots`. */
    static Set<M> closure(Collection<M> roots, Map<String, Set<String>> narrow) {
        Set<M> seen = new TreeSet<>();
        Deque<M> work = new ArrayDeque<>();
        for (M r : roots) if (descends(r.owner) && seen.add(r)) work.add(r);
        while (!work.isEmpty()) {
            M m = work.poll();
            for (Call c : calls.getOrDefault(m, List.of())) {
                if (!vanilla(c.target.owner)) continue;
                for (M t : resolve(c, narrow)) if (descends(t.owner) && !stages.contains(t) && seen.add(t)) work.add(t);
            }
        }
        seen.retainAll(bodies);
        return seen;
    }

    /** Every method named one of `pairs`' names declared in its class or a class below it. */
    static List<M> roots(String... pairs) {
        var out = new ArrayList<M>();
        for (int i = 0; i < pairs.length; i += 2)
            for (var e : declared.entrySet())
                if (below(e.getKey(), pairs[i]))
                    for (String d : e.getValue()) if (d.startsWith(pairs[i + 1] + "(")) out.add(new M(e.getKey(), pairs[i + 1], d.substring(pairs[i + 1].length())));
        return out;
    }

    /** The bodies `cls` runs for `root`'s methods whose name starts with `prefix`. */
    static List<M> own(String cls, String root, String prefix) {
        var out = new ArrayList<M>();
        for (String d : declared.getOrDefault(root, Set.of())) {
            if (!d.startsWith(prefix)) continue;
            int p = d.indexOf('(');
            M m = up(cls, new M(root, d.substring(0, p), d.substring(p)));
            if (m != null) out.add(m);
        }
        return out;
    }

    static List<M> biomeRoots() {
        var out = roots(GENERATOR, "createBiomes", GENERATOR, "doCreateBiomes", BIOME_SOURCE, "getNoiseBiome");
        for (var e : declared.entrySet())
            if (below(e.getKey(), BIOME_SOURCE))
                for (String d : e.getValue()) if (d.startsWith("createResolver")) out.add(new M(e.getKey(), d.substring(0, d.indexOf('(')), d.substring(d.indexOf('('))));
        return out;
    }

    /** `buildTerrain` and its lambdas, and what they call directly that no part of the stage reaches. */
    static Set<M> terrain(Set<M> biomes, Set<M> fill, Set<M> surface) {
        Set<M> self = new TreeSet<>();
        for (M m : bodies) if (m.owner.equals(NOISE) && (m.name.equals("buildTerrain") || m.name.startsWith("lambda$buildTerrain$"))) self.add(m);
        Set<M> out = new TreeSet<>(self);
        for (M m : self)
            for (Call c : calls.getOrDefault(m, List.of()))
                if (vanilla(c.target.owner))
                    for (M t : resolve(c, Map.of())) if (descends(t.owner) && bodies.contains(t) && !biomes.contains(t) && !fill.contains(t) && !surface.contains(t)) out.add(t);
        return out;
    }

    /** A structure type's starts, and the placement of the pieces those starts make. */
    static void structure(Set<String> rows, String type, String cls) {
        var startRoots = own(cls, STRUCTURE, "findGenerationPoint");
        // A single-piece structure's constructor hands its piece's constructor to the base class's start.
        for (var e : calls.entrySet())
            if (e.getKey().owner.equals(cls) && e.getKey().name.equals("<init>"))
                for (Call c : e.getValue()) if (c.handle) startRoots.add(c.target);
        startRoots.addAll(roots(STRUCTURE, "generate").stream().filter(m -> m.owner.equals(STRUCTURE)).toList());
        startRoots.addAll(roots(GENERATOR, "tryGenerateStructure"));
        Set<String> pieces = new TreeSet<>();
        Set<M> start;
        while (true) {
            start = closure(startRoots, Map.of(STRUCTURE, Set.of(cls), PIECE, Set.copyOf(pieces)));
            Set<String> found = new TreeSet<>();
            for (M m : startRoots) if (m.name.equals("<init>") && below(m.owner, PIECE)) found.add(m.owner);
            for (M m : start) for (String k : made.getOrDefault(m, Set.of())) if (below(k, PIECE)) found.add(k);
            if (found.equals(pieces)) break;
            pieces = found;
        }
        emit(rows, "start", type, start);
        var placeRoots = roots(START, "placeInChunk").stream().filter(m -> m.owner.equals(START)).collect(java.util.stream.Collectors.toCollection(ArrayList::new));
        placeRoots.addAll(own(cls, STRUCTURE, "afterPlace"));
        emit(rows, "place", type, closure(placeRoots, Map.of(STRUCTURE, Set.of(cls), PIECE, pieces)));
    }

    static void emit(Set<String> rows, String seam, String type, Set<M> methods) {
        for (M m : methods) rows.add(seam + "\t" + type + "\t" + m.owner + "\t" + m.name + "\t" + m.desc);
    }
}
