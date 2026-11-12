package net.minecraft.world.rt; import net.minecraft.world.item.*; import net.minecraft.core.*; import net.minecraft.world.level.LevelReader;
        public class Guarded {
          public static int level(Holder<Object> h, ItemInstance s) { return 0; }
          public void logged(ItemStack s) {}
          public void twice(ItemStack s) {}
          public boolean chestAt(LevelReader l, BlockPos p) { return false; }
          public static boolean join(net.minecraft.world.phys.shapes.VoxelShape a, net.minecraft.world.phys.shapes.VoxelShape b) { return false; }
          public static boolean joinLike(net.minecraft.world.phys.shapes.VoxelShape a, net.minecraft.world.phys.shapes.VoxelShape b) { return false; }
        }