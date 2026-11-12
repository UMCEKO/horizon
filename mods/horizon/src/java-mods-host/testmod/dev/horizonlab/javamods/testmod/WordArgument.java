package dev.horizonlab.javamods.testmod;

import com.mojang.brigadier.StringReader;
import com.mojang.brigadier.arguments.ArgumentType;
import com.mojang.brigadier.arguments.StringArgumentType;
import com.mojang.brigadier.context.CommandContext;
import com.mojang.brigadier.suggestion.Suggestions;
import com.mojang.brigadier.suggestion.SuggestionsBuilder;
import java.util.concurrent.CompletableFuture;
import net.minecraft.commands.Commands;
import net.minecraft.commands.synchronization.ArgumentTypeInfo;
import net.minecraft.commands.synchronization.ArgumentTypeInfos;
import net.minecraft.commands.synchronization.SingletonArgumentInfo;
import net.minecraft.core.registries.Registries;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.registries.DeferredRegister;

/** A mod's own argument type, for the command tree's network form: a word that suggests `alpha` and `beta`. */
public final class WordArgument implements ArgumentType<String> {
    private static final DeferredRegister<ArgumentTypeInfo<?, ?>> TYPES = DeferredRegister.create(Registries.COMMAND_ARGUMENT_TYPE, "horizon_java_test");

    static void register(IEventBus bus) {
        TYPES.register("word", () -> ArgumentTypeInfos.registerByClass(WordArgument.class, SingletonArgumentInfo.contextFree(WordArgument::new)));
        TYPES.register(bus);
    }

    /** `custom <word>` and `named <name>` under the test mod's root. */
    static com.mojang.brigadier.builder.LiteralArgumentBuilder<net.minecraft.commands.CommandSourceStack> commands() {
        return Commands.literal("horizon_java_test").executes(context -> 1)
                .then(Commands.literal("custom").then(Commands.argument("word", new WordArgument()).executes(context -> {
                    context.getSource().sendSuccess(() -> net.minecraft.network.chat.Component.literal("custom " + context.getArgument("word", String.class)), false);
                    return 1;
                })))
                .then(Commands.literal("named").then(Commands.argument("name", StringArgumentType.word())
                        .suggests((CommandContext<net.minecraft.commands.CommandSourceStack> context, SuggestionsBuilder builder) -> builder.suggest("gamma").buildFuture())
                        .executes(context -> {
                            context.getSource().sendSuccess(() -> net.minecraft.network.chat.Component.literal("named " + context.getArgument("name", String.class)), false);
                            return 1;
                        })));
    }

    @Override
    public String parse(StringReader reader) {
        return reader.readUnquotedString();
    }

    @Override
    public <S> CompletableFuture<Suggestions> listSuggestions(CommandContext<S> context, SuggestionsBuilder builder) {
        return builder.suggest("alpha").suggest("beta").buildFuture();
    }
}
