package fixture.mixin; import org.spongepowered.asm.mixin.*; import org.spongepowered.asm.mixin.injection.*; import org.spongepowered.asm.mixin.injection.callback.*;
        @Mixin(net.minecraft.client.rt.Panel.class) public class CtorMixin {
          @Inject(method = "<init>", at = @At("TAIL")) void panelInit(int a, CallbackInfo ci) {}
          @Inject(method = "<init>", at = @At("TAIL")) void mapInit(int a, CallbackInfo ci) {}
          @Inject(method = "<init>", at = @At("TAIL")) void genInit(CallbackInfo ci) {}
        }