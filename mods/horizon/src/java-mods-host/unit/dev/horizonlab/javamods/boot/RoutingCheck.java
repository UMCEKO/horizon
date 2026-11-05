package dev.horizonlab.javamods.boot;

import java.util.List;
import java.util.Set;

/** A mixin the analysis predicted but Mixin left out turns its route off instead of failing the boot. */
public final class RoutingCheck {
    public static void main(String[] args) {
        var merged = List.<String[]>of(new String[] {"h$abc", "m.Mixin"});
        if (!Routing.applied(Set.of("m.Mixin"), merged, "m.Mixin")) throw new AssertionError("an applied mixin counts");
        if (Routing.applied(Set.of(), merged, "m.Mixin")) throw new AssertionError("a mixin Mixin left out is off");
        if (Routing.applied(Set.of("m.Mixin"), List.of(), "m.Mixin")) throw new AssertionError("no merged handler is off");
        System.out.println("RoutingCheck ok");
    }
}
