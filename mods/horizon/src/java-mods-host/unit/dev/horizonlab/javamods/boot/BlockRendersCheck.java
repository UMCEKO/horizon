package dev.horizonlab.javamods.boot;

import java.util.List;
import net.minecraft.world.phys.Vec3;

/** A renderer that chooses faces by the camera records differently from opposite sides; one that doesn't, the same. */
public final class BlockRendersCheck {
    private static BlockRenders.Draw draw(int quads) {
        var d = new BlockRenders.Draw();
        for (int i = 0; i < quads; i++) d.quads.add(new BlockRenders.Quad("a:b", new float[20], new int[4], new int[4], null, false));
        return d;
    }

    public static void main(String[] args) {
        var center = new Vec3(0.5, 64.5, 0.5);
        var camera = new Vec3(5, 70, 0.5);
        boolean facing = BlockRenders.dependsOnCamera(from -> List.of(draw(from.x > center.x ? 2 : 4)), center, camera);
        boolean fixed = BlockRenders.dependsOnCamera(from -> List.of(draw(3)), center, camera);
        if (!facing || fixed) throw new AssertionError("facing=" + facing + " fixed=" + fixed);
        System.out.println("BlockRendersCheck ok");
    }
}
