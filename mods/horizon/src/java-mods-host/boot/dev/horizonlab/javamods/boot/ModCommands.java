package dev.horizonlab.javamods.boot;

import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.ParseResults;
import com.mojang.brigadier.tree.ArgumentCommandNode;
import com.mojang.brigadier.tree.CommandNode;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.mojang.brigadier.arguments.ArgumentType;
import com.mojang.brigadier.tree.RootCommandNode;
import dev.horizonlab.javamods.boot.mixin.CommandTreeCleanerAccess;
import java.util.HashMap;
import java.util.function.Function;
import net.minecraft.commands.Commands;
import net.minecraft.commands.synchronization.ArgumentTypeInfos;
import net.minecraft.commands.synchronization.ArgumentUtils;
import net.minecraft.commands.synchronization.SuggestionProviders;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.permissions.PermissionSet;
import net.neoforged.neoforge.server.command.CommandHelper;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Collections;
import java.util.IdentityHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.function.Predicate;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.commands.Commands;
import net.minecraft.network.chat.Component;
import net.minecraft.util.Util;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.CommandEvent;

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

    /** `commands_for`: the use request's header, then `bool send`; replies `text json` ("" when the player may use none), then effects. */
    static void commandsFor(Crossing.In in, Crossing.Out out) throws Exception {
        var call = GameBridge.fill(in, true);
        boolean send = in.readBoolean();
        var server = GameBridge.server();
        if (send) server.getPlayerList().sendPlayerPermissionLevel(call.player());
        var dispatcher = server.getCommands().getDispatcher();
        var names = new ArrayList<String>();
        for (var root : roots(dispatcher)) names.add(root.getKey());
        GameBridge.writeText(out, names.isEmpty() ? "" : tree(dispatcher, call.player().createCommandSourceStack(), names).toString());
        GameBridge.writeEffects(out);
    }

    private static boolean modOwned(ArgumentType<?> type) {
        var registry = BuiltInRegistries.COMMAND_ARGUMENT_TYPE;
        return BootFacts.owns(registry, registry.getKey(ArgumentTypeInfos.byClass(type)));
    }

    /** NeoForge's per-player copy of the roots in `names`, cleaned of mods' argument types, as the report's JSON with the inspector's answers. */
    @SuppressWarnings("unchecked")
    static JsonObject tree(CommandDispatcher<CommandSourceStack> dispatcher, CommandSourceStack source, List<String> names) {
        Map<CommandNode<CommandSourceStack>, CommandNode<CommandSourceStack>> map = new HashMap<>();
        var copy = new RootCommandNode<CommandSourceStack>();
        map.put(dispatcher.getRoot(), copy);
        CommandHelper.mergeCommandNode(dispatcher.getRoot(), copy, map, source, context -> 0, provider -> provider);
        var clean = CommandTreeCleanerAccess.horizon$clean(copy, type -> !modOwned(type));
        var originals = new IdentityHashMap<CommandNode<?>, CommandNode<?>>();
        for (var entry : map.entrySet()) originals.put(entry.getValue(), entry.getKey());
        var byClean = new IdentityHashMap<CommandNode<?>, CommandNode<?>>();
        pair(clean, copy, originals, byClean);
        var cleaned = new CommandDispatcher<CommandSourceStack>(clean);
        var noPermission = Commands.createCompilationContext(PermissionSet.NO_PERMISSIONS);
        var children = new JsonObject();
        for (String name : names) {
            var child = clean.getChild(name);
            if (child == null) continue;
            var json = ArgumentUtils.serializeNodeToJson(cleaned, child);
            inspect(json, child, byClean, node -> !((CommandNode<CommandSourceStack>) node).getRequirement().test(noPermission), ModCommands::suggestionId);
            children.add(name, json);
        }
        var root = new JsonObject();
        root.addProperty("type", "root");
        root.add("children", children);
        return root;
    }

    private static String suggestionId(CommandNode<?> node) {
        if (!(node instanceof ArgumentCommandNode<?, ?> argument) || argument.getCustomSuggestions() == null) return null;
        return SuggestionProviders.getName(argument.getCustomSuggestions()).toString();
    }

    /** Maps each cleaned node to its original, walking the cleaned tree and the merged copy by child name. */
    private static void pair(CommandNode<?> clean, CommandNode<?> copy, Map<CommandNode<?>, CommandNode<?>> originals, Map<CommandNode<?>, CommandNode<?>> out) {
        var original = originals.get(copy);
        if (original != null) out.put(clean, original);
        for (CommandNode<?> child : clean.getChildren()) {
            var twin = copy.getChild(child.getName());
            if (twin != null) pair(child, twin, originals, out);
        }
    }

    /** Adds `restricted` and `suggestions` to `json` for `copy`'s nodes, reading each answer off the original node; drops `permissions`. */
    static void inspect(JsonObject json, CommandNode<?> copy, Map<CommandNode<?>, CommandNode<?>> originals, Predicate<CommandNode<?>> restricted, Function<CommandNode<?>, String> suggestions) {
        json.remove("permissions");
        var original = originals.get(copy);
        if (original != null) {
            json.addProperty("restricted", restricted.test(original));
            String suggested = suggestions.apply(original);
            if (suggested != null) json.addProperty("suggestions", suggested);
        }
        if (!(json.get("children") instanceof JsonObject children)) return;
        for (CommandNode<?> child : copy.getChildren()) {
            JsonElement element = children.get(child.getName());
            if (element instanceof JsonObject object) inspect(object, child, originals, restricted, suggestions);
        }
    }

    private static final org.slf4j.Logger LOGGER = com.mojang.logging.LogUtils.getLogger();

    /** Whether a parse read its whole line without error and ends at a node whose code is Java's. */
    static boolean javaRuns(ParseResults<?> parse, Predicate<String> vanilla) {
        if (!parse.getExceptions().isEmpty() || parse.getReader().canRead()) return false;
        var last = parse.getContext().getLastChild();
        var nodes = last.getNodes();
        if (nodes.isEmpty() || last.getCommand() == null) return false;
        return javaNode(nodes.get(nodes.size() - 1).getNode(), vanilla) || java(last.getCommand(), vanilla);
    }

    /** `command`: the acting player's view as `use` fills it, then `text command`, `byte root`, `bool event`, `bool quiet`. */
    static void command(Crossing.In in, Crossing.Out out) throws Exception {
        var call = GameBridge.fill(in, false);
        String command = in.text();
        byte root = in.readByte();
        boolean event = in.readBoolean();
        boolean quiet = in.readBoolean();
        var source = call.player().createCommandSourceStack();
        String[] replaced = {""};
        byte code = perform(quiet ? source.withSuppressedOutput() : source, command, root, event, replaced);
        out.writeByte(code);
        GameBridge.writeText(out, replaced[0]);
        PlayerSync.writeChanges(call.player(), out);
        GameBridge.writeEffects(out);
    }

    /** `console_command`: a level's key and game time, then `text command`, `byte root`, `bool event`; the server is the source. */
    static void console(Crossing.In in, Crossing.Out out) throws Exception {
        GameBridge.levelOf(in);
        String command = in.text();
        byte root = in.readByte();
        boolean event = in.readBoolean();
        String[] replaced = {""};
        byte code = perform(GameBridge.server().createCommandSourceStack(), command, root, event, replaced);
        out.writeByte(code);
        GameBridge.writeText(out, replaced[0]);
        GameBridge.writeEffects(out);
    }

    /** 1 when Java ran the line, 2 when a `CommandEvent` listener stopped it, else 0, with `replaced[0]` set when a listener replaced its parse. */
    static byte perform(CommandSourceStack source, String command, byte root, boolean event, String[] replaced) {
        try {
            var commands = GameBridge.server().getCommands();
            String text = Commands.trimOptionalPrefix(command);
            if (root == JAVA) {
                commands.performPrefixedCommand(source, command);
                return 1;
            }
            var parse = commands.getDispatcher().parse(text, source);
            if (root == MIXED && javaRuns(parse, ModCommands::vanilla)) {
                commands.performCommand(parse, text);
                return 1;
            }
            if (!event) return 0;
            var posted = NeoForge.EVENT_BUS.post(new CommandEvent(parse));
            if (posted.isCanceled()) {
                // `Commands.performCommand`'s own report of a cancelled event.
                if (posted.getException() != null) {
                    source.sendFailure(Component.literal(Util.describeError(posted.getException())));
                    LOGGER.error("'/{}' threw an exception", text, posted.getException());
                }
                return 2;
            }
            if (posted.getParseResults() != parse) replaced[0] = posted.getParseResults().getReader().getString();
            return 0;
        } catch (Throwable t) {
            System.err.println("Horizon: a Java mod threw during the command /" + command);
            t.printStackTrace();
            return 0;
        }
    }
}
