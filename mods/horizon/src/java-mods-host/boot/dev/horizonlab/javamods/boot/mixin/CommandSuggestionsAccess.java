package dev.horizonlab.javamods.boot.mixin;

import com.mojang.brigadier.ParseResults;
import com.mojang.brigadier.suggestion.Suggestions;
import java.util.List;
import java.util.concurrent.CompletableFuture;
import net.minecraft.client.gui.components.CommandSuggestions;
import net.minecraft.client.multiplayer.ClientSuggestionProvider;
import net.minecraft.util.FormattedCharSequence;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;
import org.spongepowered.asm.mixin.gen.Invoker;

/** The chat box's parse, completions and usage lines, which `CommandSuggestions` keeps private. */
@Mixin(CommandSuggestions.class)
public interface CommandSuggestionsAccess {
    @Accessor("currentParse")
    ParseResults<ClientSuggestionProvider> horizon$currentParse();

    @Accessor("pendingSuggestions")
    CompletableFuture<Suggestions> horizon$pendingSuggestions();

    @Accessor("commandUsage")
    List<FormattedCharSequence> horizon$commandUsage();

    @Accessor("commandUsagePosition")
    int horizon$commandUsagePosition();

    @Accessor("commandUsageWidth")
    int horizon$commandUsageWidth();

    @Invoker("formatText")
    static FormattedCharSequence horizon$formatText(ParseResults<ClientSuggestionProvider> parse, String text, int offset) {
        throw new AssertionError();
    }
}
