package fixture.mixin; import org.spongepowered.asm.mixin.*; import org.spongepowered.asm.mixin.injection.*; import org.spongepowered.asm.mixin.injection.callback.*;
        import com.llamalad7.mixinextras.injector.wrapoperation.*;
        @Mixin(net.minecraft.world.rt.Target.class) public class TMixin {
          @Inject(method = "head", at = @At("HEAD"), cancellable = true) void headInject(int a, CallbackInfo ci) {}
          @Inject(method = "headLocal", at = @At("HEAD")) void headLocal(int a, CallbackInfo ci, int local) {}
          @Inject(method = "tail", at = @At("TAIL")) void tailInject(int a, CallbackInfoReturnable<Integer> ci) {}
          @WrapOperation(method = "wrapped", at = @At("INVOKE")) void wrap(Operation<Object> op) {}
          @Inject(method = "mixed", at = @At("HEAD")) void mixHead(CallbackInfo ci) {}
          @WrapOperation(method = "mixed", at = @At("INVOKE")) void mixWrap(Operation<Object> op) {}
          @ModifyVariable(method = "varArg", at = @At("HEAD"), argsOnly = true) int va(int a) { return a; }
          @ModifyVariable(method = "varNo", at = @At("HEAD")) int vn(int a) { return a; }
          @Inject(method = "lambda$run$0", at = @At("HEAD")) void inLambda(CallbackInfo ci) {}
          @Inject(method = "hook", at = @At("HEAD")) void hookHead(CallbackInfo ci) {}
          @WrapOperation(method = "body", at = @At("INVOKE")) void bodyWrap(Operation<Object> op) {}
          @Inject(method = "weak", at = @At("HEAD")) void weakHead(CallbackInfo ci) {}
        }