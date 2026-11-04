package dev.horizonlab.javamods.boot;

import com.mojang.renderpearl.api.pipeline.CompareOp;
import com.mojang.renderpearl.api.pipeline.DepthStencilState;
import net.minecraft.client.renderer.RenderPipelines;

/** A piece's flags come from its pipeline's state: bit 1 for no cull, bit 2 for depth writes. */
public final class WorldFramePieceFlagsCheck {
    private static void expect(int actual, int want, String what) {
        if (actual != want) throw new AssertionError(what + ": " + actual);
    }

    public static void main(String[] args) {
        expect(WorldFrame.flags(RenderPipelines.TRANSLUCENT_BLOCK), 2, "translucent block");
        var write = new DepthStencilState(CompareOp.LESS_THAN_OR_EQUAL, true, 0, 0);
        var noWrite = new DepthStencilState(CompareOp.LESS_THAN_OR_EQUAL, false, 0, 0);
        expect(WorldFrame.flags(false, write), 3, "no cull, writes depth");
        expect(WorldFrame.flags(true, noWrite), 0, "culled, no depth write");
        expect(WorldFrame.flags(true, null), 0, "no depth state");
        System.out.println("WorldFramePieceFlagsCheck ok");
    }
}
