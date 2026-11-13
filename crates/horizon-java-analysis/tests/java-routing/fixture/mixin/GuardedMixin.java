package fixture.mixin; import org.spongepowered.asm.mixin.*; import org.spongepowered.asm.mixin.injection.*; import org.spongepowered.asm.mixin.injection.callback.*;
        import net.minecraft.world.item.*; import net.minecraft.core.*; import net.minecraft.world.level.LevelReader; import net.minecraft.world.phys.shapes.VoxelShape;
        @Mixin(net.minecraft.world.rt.Guarded.class) public class GuardedMixin {
          @Inject(method = "level", at = @At("HEAD")) private static void tool(Holder<Object> h, ItemInstance s, CallbackInfoReturnable<Integer> c) {
            if (!(s.getItem() instanceof fixture.FakeTool)) return; System.out.println("tool"); }
          @Inject(method = "logged", at = @At("HEAD")) void logged(ItemStack s, CallbackInfo c) { System.out.println("first"); if (!(s.getItem() instanceof fixture.FakeTool)) return; }
          @Inject(method = "twice", at = @At("HEAD")) void guardedOnce(ItemStack s, CallbackInfo c) { if (!(s.getItem() instanceof fixture.FakeTool)) return; System.out.println("x"); }
          @Inject(method = "twice", at = @At("HEAD")) void openOnce(ItemStack s, CallbackInfo c) { System.out.println("y"); }
          @Inject(method = "chestAt", at = @At("HEAD")) void chest(LevelReader l, BlockPos p, CallbackInfoReturnable<Boolean> c) {
            if (!(l.getBlockState(p).getBlock() instanceof fixture.FakeChestBlock)) return; System.out.println("chest"); }
          @Inject(method = "join", at = @At("HEAD")) private static void shape(VoxelShape a, VoxelShape b, CallbackInfoReturnable<Boolean> c) {
            if (!(a instanceof fixture.FakeShape)) return; System.out.println("shape"); }
          @Inject(method = "joinLike", at = @At("HEAD")) private static void shapeLike(VoxelShape a, VoxelShape b, CallbackInfoReturnable<Boolean> c) {
            if (!(b instanceof fixture.ShapeLike)) return; System.out.println("shape"); }
        }