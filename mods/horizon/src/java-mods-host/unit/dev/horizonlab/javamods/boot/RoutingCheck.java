package dev.horizonlab.javamods.boot;

import com.google.gson.JsonArray;
import com.google.gson.JsonParser;
import java.util.List;
import java.util.Map;

/** A mixin the analysis predicted but Mixin left out turns its route off instead of failing the boot. */
public final class RoutingCheck {
    public static void main(String[] args) {
        var merged = List.<String[]>of(new String[] {"h$abc", "m.Mixin"});
        if (!Routing.applied(merged, "m.Mixin")) throw new AssertionError("an applied mixin counts");
        if (Routing.applied(merged, "other.Mixin")) throw new AssertionError("a mixin Mixin left out is off");
        if (Routing.applied(List.of(), "m.Mixin")) throw new AssertionError("no merged handler is off");
        contradictions();
        System.out.println("RoutingCheck ok");
    }

    private static void contradictions() {
        String key = "net/minecraft/A#m(I)V";
        JsonArray changes = JsonParser.parseString("[{\"mod\":\"x\",\"mixin\":\"m.Mixin\",\"handler\":\"head\",\"kind\":\"inject\",\"at\":[\"HEAD\"],\"locals\":false}]").getAsJsonArray();
        var mods = Map.of("m.Mixin", "x");
        var head = new Routing.Applied("handler$z$x$head", "m.Mixin", "inject", List.of("HEAD"), false, 2, List.of("m"));
        if (Routing.contradiction(key, "m", 1, false, changes, List.of(head), mods) != null) throw new AssertionError("a predicted edge that applied as an edge agrees");
        var stray = new Routing.Applied("handler$z$x$other", "m.Mixin", "inject", List.of("HEAD"), false, 2, List.of("m"));
        var unpredicted = Routing.contradiction(key, "m", 1, false, changes, List.of(head, stray), mods);
        if (unpredicted == null || !unpredicted.contains("x") || !unpredicted.contains("other")) throw new AssertionError("an applied mixin nobody predicted is an error naming mod and handler: " + unpredicted);
        var invoke = new Routing.Applied("handler$z$x$head", "m.Mixin", "inject", List.of("INVOKE"), false, 2, List.of("m(I)V"));
        var kind = Routing.contradiction(key, "m", 1, false, changes, List.of(invoke), mods);
        if (kind == null || !kind.contains("x") || !kind.contains("head") || !kind.contains("predicted edge")) throw new AssertionError("an edge predicted but applied as a body is an error: " + kind);
        var elsewhere = new Routing.Applied("handler$z$x$far", "m.Mixin", "inject", List.of("HEAD"), false, 2, List.of("other"));
        if (Routing.contradiction(key, "m", 1, false, changes, List.of(head, elsewhere), mods) != null) throw new AssertionError("a handler for another method is not this route's");
    }
}
