package dev.horizonlab.javamods.boot;

import com.mojang.brigadier.Command;
import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.builder.LiteralArgumentBuilder;
import java.util.Map;
import java.util.function.Predicate;

/** Which roots of a hand-built tree are Java's, mixed or vanilla's, with this class's nested `Vanilla` standing for vanilla. */
public final class ModCommandsCheck {
    static final class Vanilla {
        static final Command<Object> RUN = context -> 1;
    }

    private static final Predicate<String> VANILLA = owner -> owner.endsWith("$Vanilla") || ModCommands.vanilla(owner);

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    private static LiteralArgumentBuilder<Object> literal(String name) {
        return LiteralArgumentBuilder.literal(name);
    }

    public static void main(String[] args) {
        var dispatcher = new CommandDispatcher<Object>();
        dispatcher.register(literal("a").executes(Vanilla.RUN));
        dispatcher.register(literal("b").executes(context -> 2));
        dispatcher.register(literal("c").executes(Vanilla.RUN).then(literal("sub").executes(context -> 3)));
        dispatcher.register(literal("d").executes(Vanilla.RUN).redirect(dispatcher.getRoot().getChild("b")));

        var roots = dispatcher.getRoot().getChildren();
        var kinds = new java.util.LinkedHashMap<String, Byte>();
        for (var root : roots) {
            byte kind = ModCommands.kind(root, VANILLA);
            if (kind != ModCommands.NATIVE) kinds.put(root.getName(), kind);
        }
        check(kinds.equals(Map.of("b", ModCommands.JAVA, "c", ModCommands.MIXED)), "roots " + kinds);
        check(ModCommands.owner((Command<Object>) context -> 4).equals(ModCommandsCheck.class.getName()), "lambda owner");
        check(ModCommands.owner(Vanilla.RUN).endsWith("$Vanilla"), "vanilla lambda owner");
        check(ModCommands.owner("x").equals("java.lang.String"), "plain class owner");
        System.out.println("ModCommands ok");

        var source = new Object();
        check(ModCommands.javaRuns(dispatcher.parse("c sub", source), VANILLA), "a parse ending in a Java node runs in Java");
        check(!ModCommands.javaRuns(dispatcher.parse("c", source), VANILLA), "a parse ending in a vanilla node runs natively");
        check(!ModCommands.javaRuns(dispatcher.parse("c nope", source), VANILLA), "a parse with an error runs natively");
        check(ModCommands.javaRuns(dispatcher.parse("b", source), VANILLA), "a Java root's parse runs in Java");
        System.out.println("ModCommandsPerform ok");
    }
}
