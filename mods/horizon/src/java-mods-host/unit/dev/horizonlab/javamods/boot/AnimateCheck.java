package dev.horizonlab.javamods.boot;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.TorchBlock;
import net.minecraft.world.level.material.Fluid;
import net.minecraft.world.level.material.WaterFluid;

/** Which `animateTick` code Java runs: overrides, groups of picks in order, mixin targets against the closure, and the particles Java forwards. */
public final class AnimateCheck {
    static class Plain extends Block {
        Plain() {
            super(null);
        }
    }

    static class Torchlike extends TorchBlock {
        Torchlike() {
            super(null, null);
        }
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    private static byte[] group(int... xyz) throws Exception {
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(xyz.length / 3);
        for (int v : xyz) out.writeInt(v);
        return bytes.toByteArray();
    }

    public static void main(String[] args) throws Exception {
        check(Animate.overridesBlock(TorchBlock.class), "TorchBlock overrides");
        check(!Animate.overridesBlock(Block.class), "Block does not");
        check(!Animate.overridesBlock(Plain.class), "a plain subclass does not");
        check(Animate.overridesBlock(Torchlike.class), "a subclass of TorchBlock runs its parent's");
        check(Animate.overridesFluid(WaterFluid.class), "WaterFluid overrides");
        check(!Animate.overridesFluid(Fluid.class), "Fluid does not");

        Animate.group(new DataInputStream(new ByteArrayInputStream(group(1, 2, 3, 4, 5, 6))));
        Animate.group(new DataInputStream(new ByteArrayInputStream(group(7, 8, 9))));
        Animate.next();
        check(Animate.current().length == 2, "the first group first");
        Animate.next();
        check(Animate.current().length == 1 && Animate.current()[0] == net.minecraft.core.BlockPos.asLong(7, 8, 9), "the second next");
        Animate.next();
        check(Animate.current().length == 0, "a third next is empty");
        for (int i = 0; i < 14; i++) Animate.group(new DataInputStream(new ByteArrayInputStream(group(i, 0, 0))));
        Animate.next();
        check(Animate.current()[0] == net.minecraft.core.BlockPos.asLong(4, 0, 0), "at most ten groups wait");
        Animate.forget();

        var rows = Animate.parse("loop\tnet/minecraft/client/multiplayer/ClientLevel\tdoAnimateTick\t(IIIILnet/minecraft/util/RandomSource;)V\n"
                + "block\tnet/minecraft/world/level/block/BaseFireBlock\tanimateTick\t(Lx;)V\tnet/minecraft/world/level/block/BaseFireBlock\n"
                + "block\tnet/minecraft/world/level/block/AbstractCandleBlock\taddParticlesAndSound\t()V\tnet/minecraft/world/level/block/CandleBlock,net/minecraft/world/level/block/CandleCakeBlock\n");
        String loop = "mod\tcfg\tclient\tmod.Mixin\twrap\twrap_operation\tnet.minecraft.client.multiplayer.ClientLevel\tdoAnimateTick\tINVOKE:Block.animateTick";
        var hit = Animate.matches(loop, "net/minecraft/client/multiplayer/ClientLevel", rows);
        check(hit.size() == 1 && hit.get(0).role().equals("loop"), "a doAnimateTick target is the loop");
        String fire = "mod\tcfg\tclient\tmod.Fire\tredirect\tredirect\tnet.minecraft.world.level.block.BaseFireBlock\tLnet/minecraft/world/level/block/BaseFireBlock;animateTick(Lx;)V\tINVOKE";
        hit = Animate.matches(fire, "net/minecraft/world/level/block/BaseFireBlock", rows);
        check(hit.size() == 1 && hit.get(0).role().equals("block"), "a BaseFireBlock target is its block row");
        check(Animate.matches(fire.replace("(Lx;)V", "(Ly;)V"), "net/minecraft/world/level/block/BaseFireBlock", rows).isEmpty(), "another descriptor is another method");
        check(Animate.matches(loop, "net/minecraft/world/level/block/BaseFireBlock", rows).isEmpty(), "a mixin of another class matches nothing");
        check(Animate.parse(rows.get(2).served().toString().replace("[", "block\ta\tb\tc\t").replace("]", "")).get(0).served().size() == 2, "served classes split");

        var sent = new ByteArrayOutputStream();
        var out = new DataOutputStream(sent);
        ClientParticles.add(options -> new byte[] {4}, null, 1, 2, 3, 0, 0.5, 0);
        ClientParticles.add(options -> new byte[] {9, 9}, null, 4, 5, 6, 0, 0, 0);
        ClientParticles.take(out);
        var in = new DataInputStream(new ByteArrayInputStream(sent.toByteArray()));
        check(in.readInt() == 2, "two particles");
        check(in.readDouble() == 1 && in.readDouble() == 2 && in.readDouble() == 3, "the first place");
        in.readDouble();
        check(in.readDouble() == 0.5, "the first speed");
        in.readDouble();
        check(in.readInt() == 1 && in.readByte() == 4, "the first type");
        in.readNBytes(48);
        check(in.readInt() == 2 && in.readByte() == 9, "the second type");
        sent.reset();
        ClientParticles.take(out);
        check(new DataInputStream(new ByteArrayInputStream(sent.toByteArray())).readInt() == 0, "taken once");
        System.out.println("AnimateCheck ok");
    }
}
