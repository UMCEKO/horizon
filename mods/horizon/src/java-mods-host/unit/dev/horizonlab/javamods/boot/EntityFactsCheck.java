package dev.horizonlab.javamods.boot;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/** The analysis against what the booted mods say: refused packs, handlers found by merged name, listener counts, and the bytes Rust reads. */
public final class EntityFactsCheck {
    static final String ITEM = "net/minecraft/world/entity/item/ItemEntity";
    static final String FACTS = """
        {"opaque": [], "reflective_entity_access": {}, "event_root": "r/EntityEvent", "event_prefixes": ["r/tick/"],
         "event_supers": {"r/Child": ["r/Parent"]},
         "reads": {"r/Entity": ["position", "motion"]},
         "routes": {"%s": {"edge": {"%s#tick()V": [
            {"mixin": "a.M", "handler": "ret", "at": "RETURN", "cancellable": false, "conditional": false},
            {"mixin": "a.M", "handler": "head", "at": "HEAD", "cancellable": true, "conditional": false},
            {"mixin": "b.M", "handler": "gone", "at": "HEAD", "cancellable": false, "conditional": true}]},
          "body": ["%s#hurt(F)Z", "%s#other()V"], "body_conditional": {"%s#other()V": [{"mixin": "b.M", "handler": "gone", "conditional": true}]}}},
         "overrides": {"mod/Robit": ["%s#tick()V"]},
         "events": {
           "r/tick/Post": {"listeners": [{}], "subjects": [{"side": "server", "require": ["r/Living"], "exclude": ["r/Player"], "types": null}]},
           "r/Leave": {"listeners": [{}, {}], "subjects": "all"},
           "r/Missing": {"listeners": [{}], "subjects": "all"},
           "r/Grown": {"listeners": [{}], "subjects": [{"side": "both", "require": [], "exclude": [], "types": ["minecraft:cow"]}]}},
         "caps": {"minecraft:cow": ["c/Cap.X"]},
         "hooks": {"living_breathe": {"reads": ["air", "flags"], "writes": [], "events": ["r/Leave", "r/Missing", "r/Grown"]}}}
        """.formatted(ITEM, ITEM, ITEM, ITEM, ITEM, ITEM);

    public static void main(String[] args) throws Exception {
        var log = new ArrayList<String>();
        var world = new EntityFacts.World() {
            public int listeners(String event) {
                return switch (event) {
                    case "r/tick/Post", "r/Leave", "r/Unknown", "r/Parent", "r/Child" -> 1 + (event.equals("r/Leave") ? 1 : 0);
                    case "r/Grown" -> 3;
                    default -> 0;
                };
            }
            public List<String> busEvents(String root, List<String> prefixes) {
                if (!root.equals("r/EntityEvent") || !prefixes.equals(List.of("r/tick/"))) throw new AssertionError("the root and prefixes come from the file");
                return List.of("r/Parent", "r/Leave");
            }
            public List<String[]> merged(String target, String handler) {
                if (!target.equals(ITEM)) throw new AssertionError(target);
                return switch (handler) {
                    case "ret" -> List.of(new String[] {"handler$zzz000$ret", "a.M"}, new String[] {"handler$zzz001$ret", "c.M"});
                    case "head" -> List.<String[]>of(new String[] {"handler$zzz000$head", "a.M"});
                    default -> List.of();
                };
            }
        };
        var bytes = EntityFacts.build(JsonParser.parseString(FACTS).getAsJsonObject(), world, log::add, List.of("a:att"));
        var in = new DataInputStream(new ByteArrayInputStream(bytes));
        expect(in.readInt() == 2, "version");
        var methods = new ArrayList<String>();
        for (int n = in.readInt(), i = 0; i < n; i++) methods.add(text(in));
        expect(methods.equals(List.of(ITEM + "#tick()V", ITEM + "#hurt(F)Z")), "methods " + methods);
        expect(in.readInt() == 2, "two classes");
        expect(text(in).equals(ITEM) && in.readInt() == 0, "the item class reads nothing");
        expect(in.readInt() == 1 && in.readInt() == 0 && in.readInt() == 2, "one edge on method 0 with two handlers (the conditional one left out)");
        expect(text(in).equals("handler$zzz000$ret") && in.readByte() == 1 && !in.readBoolean(), "the handler told apart by its mixin, as an exit");
        expect(text(in).equals("handler$zzz000$head") && in.readByte() == 0 && in.readBoolean(), "a cancellable head");
        expect(in.readInt() == 1 && in.readInt() == 1, "one body route: the conditional one is gone");
        expect(text(in).equals("r/Entity") && in.readInt() == (2 | 4), "position and motion bits");
        in.readInt();
        in.readInt();
        expect(in.readInt() == 1 && text(in).equals("mod/Robit") && in.readInt() == 1 && in.readInt() == 0, "an override of method 0");
        var events = new java.util.TreeMap<String, String>();
        for (int n = in.readInt(), i = 0; i < n; i++) {
            var name = text(in);
            boolean all = in.readBoolean();
            int filters = in.readInt();
            var sb = new StringBuilder(all ? "all" : "");
            for (int f = 0; f < filters; f++) {
                sb.append("side").append(in.readByte());
                for (int k = 0; k < 3; k++) {
                    sb.append('[');
                    for (int m = in.readInt(), j = 0; j < m; j++) sb.append(text(in)).append(',');
                    sb.append(']');
                }
            }
            events.put(name, sb.toString());
        }
        expect(events.equals(Map.of(
                "r/tick/Post", "side1[r/Living,][r/Player,][]",
                "r/Leave", "all",
                "r/Grown", "all",
                "r/Parent", "all",
                "r/Child", "all")), "events " + events);
        expect(in.readInt() == 1 && text(in).equals("minecraft:cow") && in.readInt() == 1 && text(in).equals("c/Cap.X"), "caps");
        expect(in.readInt() == 1 && text(in).equals("a:att"), "attachments");
        expect(in.readInt() == 1 && text(in).equals("living_breathe") && in.readInt() == (128 | 256), "a hook and its reads");
        expect(in.readInt() == 2 && in.readInt() == 2 && in.readInt() == 1, "its events by position, the unlistened one skipped");
        expect(in.available() == 0, "nothing left");
        expect(log.stream().anyMatch(l -> l.equals("r/Missing not listened at runtime")), "a dropped event is logged: " + log);
        expect(log.stream().anyMatch(l -> l.equals("r/Parent listened at runtime only")), "a runtime-only event is logged: " + log);
        expect(log.stream().anyMatch(l -> l.startsWith("r/Child listened at runtime only")), "its subclass too: " + log);
        expect(log.stream().anyMatch(l -> l.contains("r/Grown has 3 listeners at runtime, 1 analysed")), "extra listeners: " + log);

        refused("{\"opaque\": [\"mek: service ClassProcessorProvider: x.Provider\"]}", "mek: service ClassProcessorProvider");
        var ambiguous = FACTS.replace("\"handler\": \"head\"", "\"handler\": \"ret\", \"x\": 1").replace("\"mixin\": \"a.M\", \"handler\": \"ret\", \"at\": \"RETURN\"", "\"mixin\": \"z.M\", \"handler\": \"ret\", \"at\": \"RETURN\"");
        refusedBuild(ambiguous, world, "z.M.ret");
        System.out.println("EntityFactsCheck ok");
    }

    private static void refused(String json, String named) throws Exception {
        var facts = JsonParser.parseString(json).getAsJsonObject();
        try {
            EntityFacts.build(facts, null, line -> {}, List.of());
        } catch (IllegalStateException e) {
            if (!e.getMessage().contains(named) || !e.getMessage().contains("can't start")) throw new AssertionError(e.getMessage());
            return;
        }
        throw new AssertionError("an opaque pack was accepted");
    }

    private static void refusedBuild(String json, EntityFacts.World world, String named) throws Exception {
        try {
            EntityFacts.build(JsonParser.parseString(json).getAsJsonObject(), world, line -> {}, List.of());
        } catch (IllegalStateException e) {
            if (!e.getMessage().contains(named)) throw new AssertionError(e.getMessage());
            return;
        }
        throw new AssertionError("a handler that isn't merged was accepted");
    }

    private static String text(DataInputStream in) throws Exception {
        return new String(in.readNBytes(in.readInt()), java.nio.charset.StandardCharsets.UTF_8);
    }

    private static void expect(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }
}
