package dev.horizonlab.javamods.boot.mixin;

import com.mojang.brigadier.arguments.ArgumentType;
import com.mojang.brigadier.tree.RootCommandNode;
import java.util.function.Predicate;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Invoker;

/** NeoForge's cleaner of mods' argument types, which is package-private. */
@Mixin(targets = "net.neoforged.neoforge.network.filters.CommandTreeCleaner")
public interface CommandTreeCleanerAccess {
    @Invoker("cleanArgumentTypes")
    static <S> RootCommandNode<S> horizon$clean(RootCommandNode<S> root, Predicate<ArgumentType<?>> keep) {
        throw new AssertionError();
    }
}
