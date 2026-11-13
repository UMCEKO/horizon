package dev.horizonlab.javamods.boot;

import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.builder.LiteralArgumentBuilder;

/** `ModCommands.javaSuggests` on hand-built dispatchers: true only when the parent of the completed word has a Java child. */
public final class ModCommandsSuggestCheck {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    private static LiteralArgumentBuilder<Object> literal(String name) {
        return LiteralArgumentBuilder.literal(name);
    }

    public static void main(String[] args) {
        var dispatcher = new CommandDispatcher<Object>();
        dispatcher.register(literal("c").then(literal("j").executes(context -> 1)));
        dispatcher.register(literal("a").then(literal("b")));
        var source = new Object();
        check(ModCommands.javaSuggests(dispatcher.parse("c ", source), 2, ModCommands::vanilla), "c has a Java literal child");
        check(!ModCommands.javaSuggests(dispatcher.parse("a ", source), 2, ModCommands::vanilla), "a has vanilla children only");
    }
}
