package dev.horizonlab.javamods.boot;

import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Map;
import net.minecraft.client.Minecraft;
import net.minecraft.client.particle.ParticleEngine;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.neoforged.neoforge.client.extensions.common.IClientBlockExtensions;

/** Which block states' client extensions make their own destroy or hit particles, and running them for the window. */
final class BlockEffects {
    private BlockEffects() {}

    static final int DESTROY = 1, HIT = 2;

    private static final Map<Class<?>, Integer> BITS = new HashMap<>();

    /** DESTROY if the extension's class declares `addDestroyEffects` and HIT if `addHitEffects`, in a mod's class. */
    static int bits(Class<?> extension) {
        int bits = 0;
        if (declares(extension, "addDestroyEffects", BlockState.class, Level.class, BlockPos.class, ParticleEngine.class)) bits |= DESTROY;
        if (declares(extension, "addHitEffects", BlockState.class, Level.class, BlockPos.class, Direction.class, ParticleEngine.class)) bits |= HIT;
        return bits;
    }

    /** Whether the type or a superclass a mod owns declares the method. */
    static boolean declares(Class<?> type, String name, Class<?>... params) {
        for (var c = type; c != null; c = c.getSuperclass()) {
            if (!Shells.isMods(c)) continue;
            try {
                c.getDeclaredMethod(name, params);
                return true;
            } catch (NoSuchMethodException none) {
                // keep walking up
            }
        }
        return false;
    }

    /** Per state of every block whose extension makes effects: the count, then each state's id and bits. */
    static void export(DataOutputStream out) throws IOException {
        var found = new ArrayList<int[]>();
        for (Block block : BuiltInRegistries.BLOCK) {
            int bits = BITS.computeIfAbsent(IClientBlockExtensions.of(block).getClass(), BlockEffects::bits);
            if (bits == 0) continue;
            for (var state : block.getStateDefinition().getPossibleStates()) found.add(new int[] {Block.BLOCK_STATE_REGISTRY.getId(state), bits});
        }
        out.writeInt(found.size());
        for (var entry : found) {
            out.writeInt(entry[0]);
            out.writeByte(entry[1]);
        }
        if (!found.isEmpty()) System.err.println("Horizon: " + found.size() + " block states make their own destroy or hit particles in Java");
    }

    /** One `BlockEffect` input: vanilla's own `ClientLevel` method, so the mod's code decides and the fallback is Java's. */
    static void run(DataInputStream in) throws IOException {
        var level = Minecraft.getInstance().level;
        int kind = in.readByte();
        var pos = new BlockPos(in.readInt(), in.readInt(), in.readInt());
        int state = in.readInt();
        int face = in.readByte();
        if (level == null) return;
        ClientParticles.enter();
        try {
            if (kind == 0) level.addDestroyBlockEffect(pos, Block.stateById(state));
            else level.addBreakingBlockEffects(pos, Direction.from3DDataValue(face), false);
        } finally {
            ClientParticles.exit();
        }
    }
}
