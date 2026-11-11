package dev.horizonlab.javamods.boot;

import java.io.DataInputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.HashSet;
import java.util.List;
import java.util.Set;
import net.minecraft.client.multiplayer.ClientLevel;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.util.RandomSource;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.level.material.FluidState;

/** Which `animateTick` code runs in Java: native picks the positions and runs vanilla's own; Java runs only what a mod owns or patches. */
public final class Animate {
    private Animate() {}

    /** A row of `animate_closure.tsv`: `role` loop, block or fluid; for the last two `served` names the block or fluid classes whose states it belongs to. */
    record Row(String role, String owner, String method, String desc, List<String> served) {}

    private static final long[] NONE = new long[0];
    private static final ArrayDeque<long[]> GROUPS = new ArrayDeque<>();
    private static final Set<String> THREW = new HashSet<>();
    private static final Set<String> SEEN = new HashSet<>();
    private static final boolean DEBUG = System.getenv("HORIZON_JAVA_DEBUG") != null;
    private static final Set<String> PATCHED_BLOCKS = new HashSet<>();
    private static final Set<String> PATCHED_FLUIDS = new HashSet<>();
    private static long[] current = NONE;
    private static boolean wholeLoop;
    private static boolean forwarding;

    private static final ClassValue<Boolean> BLOCK_CODE = new ClassValue<>() {
        @Override
        protected Boolean computeValue(Class<?> type) {
            boolean runs = overridesBlock(type);
            if (runs) System.err.println("Horizon: " + type.getName() + " declares animateTick: its mod's states run it in Java");
            return runs;
        }
    };
    private static final ClassValue<Boolean> FLUID_CODE = new ClassValue<>() {
        @Override
        protected Boolean computeValue(Class<?> type) {
            boolean runs = overridesFluid(type);
            if (runs) System.err.println("Horizon: " + type.getName() + " declares animateTick: its mod's fluid runs it in Java");
            return runs;
        }
    };
    private static final ClassValue<Boolean> BLOCK_PATCHED = new ClassValue<>() {
        @Override
        protected Boolean computeValue(Class<?> type) {
            return PATCHED_BLOCKS.contains(declaring(type, true));
        }
    };
    private static final ClassValue<Boolean> FLUID_PATCHED = new ClassValue<>() {
        @Override
        protected Boolean computeValue(Class<?> type) {
            return PATCHED_FLUIDS.contains(declaring(type, false));
        }
    };

    /** Whether a block class runs its own `animateTick`: the first declaration up its chain is not `Block`'s empty one. */
    static boolean overridesBlock(Class<?> type) {
        return declaring(type, true) != null && !declaring(type, true).equals(Block.class.getName());
    }

    /** `Fluid.animateTick` is protected, so the declaration is looked up class by class. */
    static boolean overridesFluid(Class<?> type) {
        return declaring(type, false) != null && !declaring(type, false).equals(Fluid.class.getName());
    }

    private static String declaring(Class<?> type, boolean block) {
        for (Class<?> c = type; c != null; c = c.getSuperclass()) {
            try {
                if (block) c.getDeclaredMethod("animateTick", BlockState.class, Level.class, BlockPos.class, RandomSource.class);
                else c.getDeclaredMethod("animateTick", Level.class, BlockPos.class, FluidState.class, RandomSource.class);
                return c.getName();
            } catch (NoSuchMethodException e) {
                // Up the chain.
            }
        }
        return null;
    }

    private static boolean ownBlockCode(Block block) {
        return BootFacts.ownsBlock(block) && BLOCK_CODE.get(block.getClass());
    }

    private static boolean ownFluidCode(Fluid fluid) {
        return BootFacts.owns(BuiltInRegistries.FLUID, BuiltInRegistries.FLUID.getKey(fluid)) && FLUID_CODE.get(fluid.getClass());
    }

    /** Bit 1: this state's block runs `animateTick` in Java; bit 2: its fluid does. */
    static int route(BlockState state) {
        Block block = state.getBlock();
        FluidState fluidState = state.getFluidState();
        int bits = 0;
        if (ownBlockCode(block) || BLOCK_PATCHED.get(block.getClass())) bits |= 1;
        if (!fluidState.isEmpty() && (ownFluidCode(fluidState.getType()) || FLUID_PATCHED.get(fluidState.getType().getClass()))) bits |= 2;
        return bits;
    }

    /** Whether a mod patches the animate loop itself, so the Java client's own loop runs and native's does not. */
    public static boolean wholeLoop() {
        return wholeLoop;
    }

    /** True while Java makes the particles and sounds of `animateTick` code, whose copies native does not make. */
    public static boolean forwarding() {
        return forwarding;
    }

    /** Vanilla's own loop runs (a mod patches it): what it makes is Java's to forward. */
    public static void loop(boolean running) {
        forwarding = running;
    }

    /** One tick's picked positions, as the window sent them. */
    static void group(DataInputStream in) throws IOException {
        long[] positions = new long[in.readInt()];
        for (int i = 0; i < positions.length; i++) positions[i] = BlockPos.asLong(in.readInt(), in.readInt(), in.readInt());
        GROUPS.add(positions);
        // The window keeps at most as many as it can owe ticks.
        while (GROUPS.size() > 10) GROUPS.poll();
    }

    /** This tick's picks, sent at its player step in a lockstep tick: the group the tick's `animateTick` runs. */
    static void current(DataInputStream in) throws IOException {
        long[] positions = new long[in.readInt()];
        for (int i = 0; i < positions.length; i++) positions[i] = BlockPos.asLong(in.readInt(), in.readInt(), in.readInt());
        if (positions.length > 0) current = positions;
    }

    /** The group of the tick about to run: the oldest queued, or none. */
    static void next() {
        current = GROUPS.isEmpty() ? NONE : GROUPS.poll();
    }

    static long[] current() {
        return current;
    }

    static void forget() {
        GROUPS.clear();
        current = NONE;
    }

    /** `ClientLevel.animateTick` for the positions native picked: only the code the mods own or patch runs, in pick order, on one random source. */
    public static void run(ClientLevel level) {
        long[] picks = current;
        current = NONE;
        if (picks.length == 0) return;
        RandomSource random = RandomSource.createThreadLocalInstance();
        forwarding = true;
        try {
            for (long packed : picks) {
                BlockPos pos = BlockPos.of(packed);
                BlockState state = level.getBlockState(pos);
                int routed = route(state);
                if ((routed & 1) != 0) call(state.getBlock().getClass(), "block", () -> state.getBlock().animateTick(state, level, pos, random), state);
                if ((routed & 2) != 0) call(level.getFluidState(pos).getType().getClass(), "fluid", () -> level.getFluidState(pos).animateTick(level, pos, random), state);
            }
        } finally {
            forwarding = false;
        }
    }

    private static void call(Class<?> type, String what, Runnable step, BlockState state) {
        if (DEBUG && SEEN.add(what + BuiltInRegistries.BLOCK.getKey(state.getBlock()))) System.err.println("Horizon: Java animateTick for " + BuiltInRegistries.BLOCK.getKey(state.getBlock()));
        try {
            step.run();
        } catch (Throwable t) {
            if (THREW.add(type.getName())) {
                System.err.println("Horizon: a Java mod threw during animateTick");
                t.printStackTrace();
            }
        }
    }

    // Boot: which mixins land in the animate code

    static List<Row> parse(String tsv) {
        var rows = new ArrayList<Row>();
        for (String line : tsv.split("\n")) {
            if (line.isBlank()) continue;
            String[] f = line.split("\t", -1);
            rows.add(new Row(f[0], f[1], f[2], f[3], f.length > 4 && !f[4].isEmpty() ? List.of(f[4].split(",")) : List.of()));
        }
        return rows;
    }

    /** A mixin handler's target (`name`, `name(desc)` or `Lowner;name(desc)`) against a closure row's method. */
    static boolean targetMatches(String target, Row row) {
        String t = target.replaceFirst("^L[^;]+;", "");
        int paren = t.indexOf('(');
        String name = paren < 0 ? t : t.substring(0, paren);
        return name.equals(row.method()) && (paren < 0 || t.substring(paren).equals(row.desc()));
    }

    /** The rows of `owner` a scanner line (`mod config side mixin handler kind targets methods at`) patches. */
    static List<Row> matches(String scanned, String owner, List<Row> rows) {
        String[] f = scanned.split("\t", -1);
        var found = new ArrayList<Row>();
        if (f.length < 9 || !List.of(f[6].split(",")).contains(owner.replace('/', '.'))) return found;
        for (String target : f[7].split(",")) {
            if (target.isEmpty()) continue;
            for (Row row : rows) if (row.owner().equals(owner) && targetMatches(target, row)) found.add(row);
        }
        return found;
    }

    /** Reads the closure and finds the loaded mods' mixins on it, once the mods' registration is sealed. */
    static void route() throws Exception {
        String tsv;
        try (var in = Animate.class.getResourceAsStream("/animate_closure.tsv")) {
            tsv = new String(in.readAllBytes(), StandardCharsets.UTF_8);
        }
        var rows = parse(tsv);
        var classes = new java.util.TreeSet<String>();
        for (Row row : rows) classes.add(row.owner());
        for (String owner : classes) {
            String dotted = owner.replace('/', '.');
            try {
                Class.forName(dotted, false, Animate.class.getClassLoader());
            } catch (ClassNotFoundException | LinkageError e) {
                continue;
            }
            for (var info : org.spongepowered.asm.mixin.Mixins.getMixinsForClass(dotted)) {
                String config = info.getConfig().getName();
                if (config.startsWith("horizon_java")) continue;
                String mod = config.replaceFirst("\\.mixins?\\.json$", "");
                for (String line : dev.horizonlab.javamods.parity.MixinScanner.scan(mod, config, "client", info.getClassBytes()).split("\n")) {
                    if (line.isEmpty()) continue;
                    String[] f = line.split("\t", -1);
                    for (Row row : matches(line, owner, rows)) {
                        switch (row.role()) {
                            case "loop" -> wholeLoop = true;
                            case "block" -> PATCHED_BLOCKS.addAll(row.served().stream().map(s -> s.replace('/', '.')).toList());
                            default -> PATCHED_FLUIDS.addAll(row.served().stream().map(s -> s.replace('/', '.')).toList());
                        }
                        System.err.println("Horizon: " + mod + " patches " + owner.substring(owner.lastIndexOf('/') + 1) + "#" + row.method() + " (" + f[5] + " at " + f[8] + "): " + row.role() + " animateTick runs in Java");
                    }
                }
            }
        }
    }
}
