package mixfix;

import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.injection.At;
import org.spongepowered.asm.mixin.injection.Constant;
import org.spongepowered.asm.mixin.injection.Inject;
import org.spongepowered.asm.mixin.injection.ModifyConstant;

@Mixin(value = {java.util.List.class, int[].class}, targets = "a/b/Hidden")
abstract class Rich {
    @Inject(at = @At("HEAD"))
    void first() {}

    void unmarked() {}

    @ModifyConstant(constant = @Constant(doubleValue = 1.5e10, stringValue = "s", classValue = String[].class, expandZeroConditions = {Constant.Condition.GREATER_THAN_ZERO, Constant.Condition.LESS_THAN_ZERO}))
    double d(double v) {
        return v;
    }
}
