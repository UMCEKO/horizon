package org.spongepowered.asm.mixin.gen;

import java.lang.annotation.Retention;
import java.lang.annotation.RetentionPolicy;

@Retention(RetentionPolicy.CLASS)
public @interface Accessor {
    String value() default "";
}
