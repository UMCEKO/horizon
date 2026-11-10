package org.spongepowered.asm.mixin.injection;

import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;

@Retention(RetentionPolicy.CLASS)
public @interface Constant {
    int intValue() default 0;

    double doubleValue() default 0;

    String stringValue() default "";

    Class<?> classValue() default void.class;

    Condition[] expandZeroConditions() default {};

    enum Condition {
        LESS_THAN_ZERO,
        GREATER_THAN_ZERO
    }
}
