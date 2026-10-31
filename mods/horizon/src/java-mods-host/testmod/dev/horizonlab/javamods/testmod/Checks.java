package dev.horizonlab.javamods.testmod;

import java.util.function.BooleanSupplier;
import net.minecraft.core.BlockPos;
import net.minecraft.world.item.context.UseOnContext;

/** One named answer per bridge, printed for `java_mods.py bridges`; each family has its own clear area. */
public final class Checks {
    private Checks() {}

    public interface Family {
        void run(UseOnContext context, BlockPos area);
    }

    public static void check(String name, BooleanSupplier answer) {
        String result;
        try {
            result = answer.getAsBoolean() ? "ok" : "FAILED";
        } catch (Throwable t) {
            result = "THREW " + t;
        }
        System.out.println("CHECK " + name + " " + result);
    }

    public static void runAll(UseOnContext context) {
        Family[] families = {ChecksRules::run, ChecksLevelApi::run, ChecksEffects::run, ChecksPlayer::run, ChecksValues::run, ChecksEntities::run, ChecksBlockEntities::run, ChecksServer::run};
        BlockPos base = context.getClickedPos();
        for (int i = 0; i < families.length; i++) families[i].run(context, base.offset(8 + i * 4, 0, 0));
        System.out.println("CHECK done ok");
    }
}
