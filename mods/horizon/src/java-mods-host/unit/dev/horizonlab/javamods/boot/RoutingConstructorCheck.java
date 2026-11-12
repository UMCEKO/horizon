package dev.horizonlab.javamods.boot;

import com.google.gson.JsonParser;
import java.util.List;

/** A constructor route is never bound to a method handle, and its boot line says who runs the mods' patches. */
public final class RoutingConstructorCheck {
    public static void main(String[] args) throws Exception {
        if (!Routing.constructor("<init>") || !Routing.constructor("<clinit>")) throw new AssertionError("constructors and static initialisers are constructors");
        if (Routing.constructor("init") || Routing.constructor("tick")) throw new AssertionError("a method named like one is not");
        for (String name : List.of("<init>", "<clinit>")) {
            try {
                Routing.handle(Routing.class, "dev/horizonlab/javamods/boot/Routing#" + name + "()V", name, "()V", name.equals("<clinit>"), RoutingConstructorCheck.class.getClassLoader());
                throw new AssertionError(name + " was bound");
            } catch (IllegalStateException refused) {
                if (!refused.getMessage().contains("never bound")) throw new AssertionError("wrong refusal: " + refused.getMessage());
            }
        }
        if (Routing.handle(Routing.class, "dev/horizonlab/javamods/boot/Routing#noop()V", "noop", "()V", true, RoutingConstructorCheck.class.getClassLoader()) == null) throw new AssertionError("a method still binds");
        var seam = JsonParser.parseString("{\"by\":\"seam\",\"reason\":\"r\"}").getAsJsonObject();
        if (!Routing.constructorLine("k/A#<init>()V", seam, true, List.of("lithostitched")).equals("Horizon: route k/A#<init>()V constructor applied, seam (r) (lithostitched)")) throw new AssertionError("seam line");
        if (!Routing.constructorLine("k/A#<init>()V", null, false, List.of("a")).equals("Horizon: route k/A#<init>()V constructor not applied, deferred (no decision in the analysis) (a)")) throw new AssertionError("older analysis line");
        System.out.println("RoutingConstructorCheck ok");
    }
}
