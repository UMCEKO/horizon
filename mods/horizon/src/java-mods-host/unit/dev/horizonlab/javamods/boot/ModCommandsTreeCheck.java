package dev.horizonlab.javamods.boot;

import com.google.gson.JsonObject;
import com.mojang.brigadier.builder.LiteralArgumentBuilder;
import com.mojang.brigadier.tree.CommandNode;
import java.util.IdentityHashMap;

/** `ModCommands.inspect` over a hand-built tree: `restricted` from the original's requirement, `permissions` dropped. */
public final class ModCommandsTreeCheck {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    private static LiteralArgumentBuilder<Object> literal(String name) {
        return LiteralArgumentBuilder.literal(name);
    }

    public static void main(String[] args) {
        CommandNode<Object> original = literal("mek").then(literal("admin").requires(source -> false)).then(literal("give")).build();
        CommandNode<Object> copy = literal("mek").then(literal("admin")).then(literal("give")).build();
        var originals = new IdentityHashMap<CommandNode<?>, CommandNode<?>>();
        originals.put(copy, original);
        originals.put(copy.getChild("admin"), original.getChild("admin"));
        originals.put(copy.getChild("give"), original.getChild("give"));

        var json = new JsonObject();
        json.addProperty("type", "literal");
        json.add("permissions", new JsonObject());
        var children = new JsonObject();
        for (String name : new String[] {"admin", "give"}) {
            var child = new JsonObject();
            child.addProperty("type", "literal");
            child.add("permissions", new JsonObject());
            children.add(name, child);
        }
        json.add("children", children);

        ModCommands.inspect(json, copy, originals, node -> !node.getRequirement().test(new Object()), node -> null);
        check(!json.has("permissions"), "root permissions are gone");
        check(!json.get("restricted").getAsBoolean(), "root is open");
        check(children.getAsJsonObject("admin").get("restricted").getAsBoolean(), "admin is restricted");
        check(!children.getAsJsonObject("give").get("restricted").getAsBoolean(), "give is open");
        check(!children.getAsJsonObject("admin").has("permissions"), "child permissions are gone");
    }
}
