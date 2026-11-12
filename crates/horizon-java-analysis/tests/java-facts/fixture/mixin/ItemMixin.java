package fixture.mixin; import org.spongepowered.asm.mixin.*; import org.spongepowered.asm.mixin.injection.*; import org.spongepowered.asm.mixin.injection.callback.*;
        @Mixin(net.minecraft.world.entity.item.ItemEntity.class) public class ItemMixin {
          @Inject(method = "tick", at = @At("HEAD"), cancellable = true) void head(CallbackInfo ci) {}
          @Inject(method = "tick", at = @At("RETURN")) void ret(CallbackInfo ci) {}
          @Inject(method = "tick", at = @At("HEAD")) void captured(CallbackInfo ci, int local) {}
          @Inject(method = "hurt", at = @At("INVOKE")) void middle(float a, CallbackInfoReturnable<Boolean> ci) {}
          @Inject(method = "tryPick", at = @At("HEAD"), cancellable = true) void pick(int a, CallbackInfoReturnable<Boolean> ci) {}
        }
