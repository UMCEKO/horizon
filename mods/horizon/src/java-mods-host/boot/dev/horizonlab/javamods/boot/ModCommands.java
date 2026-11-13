package dev.horizonlab.javamods.boot;

import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.tree.ArgumentCommandNode;
import com.mojang.brigadier.tree.CommandNode;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Collections;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.function.Predicate;
import net.minecraft.commands.CommandSourceStack;

/** Which root commands of the server's dispatcher run mod code, told by the classes that own each node's code. */
final class ModCommands {
    private ModCommands() {}

    static final byte NATIVE = 0, JAVA = 1, MIXED = 2;

    /** The class that declares this code: a lambda's hidden class up to `$$Lambda`, else its own class. */
    static String owner(Object code) {
        String name = code.getClass().getName();
        int lambda = name.indexOf("$$Lambda");
        return lambda < 0 ? name : name.substring(0, lambda);
    }

    static boolean vanilla(String owner) {
        return owner.startsWith("net.minecraft.") || owner.startsWith("com.mojang.brigadier.");
    }

    private static boolean java(Object code, Predicate<String> vanilla) {
        return code != null && !vanilla.test(owner(code));
    }

    /** One node's code is Java's when any of its command, requirement, redirect modifier, custom suggestions or argument type is. */
    static boolean javaNode(CommandNode<?> node, Predicate<String> vanilla) {
        if (java(node.getCommand(), vanilla) || java(node.getRequirement(), vanilla) || java(node.getRedirectModifier(), vanilla)) return true;
        if (node instanceof ArgumentCommandNode<?, ?> argument) {
            return java(argument.getCustomSuggestions(), vanilla) || java(argument.getType(), vanilla);
        }
        return false;
    }

    /** JAVA if the root's own node is Java's, MIXED if any node below it is, else NATIVE; a redirect target is not part of the root pointing at it. */
    static byte kind(CommandNode<?> root, Predicate<String> vanilla) {
        if (javaNode(root, vanilla)) return JAVA;
        Set<CommandNode<?>> seen = Collections.newSetFromMap(new IdentityHashMap<>());
        seen.add(root);
        var pending = new ArrayDeque<CommandNode<?>>();
        pending.add(root);
        while (!pending.isEmpty()) {
            CommandNode<?> node = pending.poll();
            if (node.getRedirect() != null) seen.add(node.getRedirect());
            for (CommandNode<?> child : node.getChildren()) {
                if (!seen.add(child)) continue;
                if (javaNode(child, vanilla)) return MIXED;
                pending.add(child);
            }
        }
        return NATIVE;
    }

    /** Every root child of the server's dispatcher whose kind isn't NATIVE, in the dispatcher's order. */
    static List<Map.Entry<String, Byte>> roots(CommandDispatcher<CommandSourceStack> dispatcher) {
        var found = new ArrayList<Map.Entry<String, Byte>>();
        for (CommandNode<CommandSourceStack> root : dispatcher.getRoot().getChildren()) {
            byte kind = kind(root, ModCommands::vanilla);
            if (kind != NATIVE) found.add(Map.entry(root.getName(), kind));
        }
        return found;
    }
}
