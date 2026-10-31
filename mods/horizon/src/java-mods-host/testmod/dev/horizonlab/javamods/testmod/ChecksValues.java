package dev.horizonlab.javamods.testmod;

import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.phys.Vec3;
import net.minecraft.world.phys.shapes.Shapes;
import net.minecraft.world.phys.shapes.VoxelShape;

final class ChecksValues {
    private ChecksValues() {}

    static void run(UseOnContext context, BlockPos area) {
        Checks.check("VoxelShape.clip", () -> {
            Vec3 at = Vec3.atLowerCornerOf(area);
            BlockHitResult side = Shapes.block().clip(at.add(-1, 0.5, 0.5), at.add(2, 0.5, 0.5), area);
            BlockHitResult inside = Shapes.block().clip(at.add(0.5, 0.5, 0.5), at.add(3, 0.5, 0.5), area);
            VoxelShape slab = Shapes.box(0, 0, 0, 1, 0.5, 1);
            BlockHitResult top = slab.clip(at.add(0.5, 1.5, 0.5), at.add(0.5, 0.25, 0.5), area);
            boolean missed = slab.clip(at.add(0.5, 1.5, 0.5), at.add(0.5, 0.75, 0.5), area) == null && Shapes.empty().clip(at, at.add(1, 1, 1), area) == null;
            VoxelShape stairs = Shapes.or(Shapes.box(0, 0, 0, 1, 0.5, 1), Shapes.box(0, 0.5, 0, 1, 1, 0.5));
            boolean agrees = true;
            for (int i = 0; i < 64; i++) {
                Vec3 from = at.add(i % 4 * 0.9 - 1, i / 4 % 4 * 0.9 - 1, i / 16 * 0.9 - 1);
                Vec3 to = at.add(2 - i % 3 * 0.8, 2 - i % 5 * 0.7, 1.7 - i % 7 * 0.6);
                BlockHitResult mine = stairs.clip(from, to, area);
                BlockHitResult plain = AABB.clip(stairs.toAabbs(), from, to, area);
                if (mine != null && mine.isInside()) continue;
                if (mine == null || plain == null) {
                    agrees &= mine == plain;
                } else {
                    agrees &= mine.getLocation().equals(plain.getLocation()) && mine.getDirection() == plain.getDirection() && mine.getBlockPos().equals(plain.getBlockPos());
                }
            }
            return side != null && side.getDirection() == Direction.WEST && side.getLocation().x == at.x && !side.isInside() && side.getBlockPos().equals(area)
                && inside != null && inside.isInside()
                && top != null && top.getDirection() == Direction.UP && top.getLocation().y == at.y + 0.5
                && missed && agrees;
        });
    }
}
