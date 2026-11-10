package net.neoforged.fml.common; import java.lang.annotation.*; @Retention(RetentionPolicy.RUNTIME) public @interface EventBusSubscriber { Bus bus() default Bus.GAME; enum Bus { GAME, MOD } }
