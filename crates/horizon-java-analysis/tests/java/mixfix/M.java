package mixfix;

import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.Overwrite;
import org.spongepowered.asm.mixin.gen.Accessor;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Constant;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyConstant;

@Mixin(targets = "net/minecraft/world/Base")
abstract class M {
    @Inject(method = "tick", at = @At(value = "INVOKE", target = "La;b()V"))
    void h() {}

    @Overwrite
    void must() {}

    @Accessor("x")
    abstract int getX();

    @ModifyConstant(method = "tick", constant = @Constant(intValue = 5))
    int c(int v) {
        return v;
    }
}
