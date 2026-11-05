package dev.horizonlab.javamods.boot;

import java.util.List;

/** A mixin the analysis predicted but Mixin left out turns its route off instead of failing the boot. */
public final class RoutingCheck {
    public static void main(String[] args) {
        var merged = List.<String[]>of(new String[] {"h$abc", "m.Mixin"});
        if (!Routing.applied(merged, "m.Mixin")) throw new AssertionError("an applied mixin counts");
        if (Routing.applied(merged, "other.Mixin")) throw new AssertionError("a mixin Mixin left out is off");
        if (Routing.applied(List.of(), "m.Mixin")) throw new AssertionError("no merged handler is off");
        System.out.println("RoutingCheck ok");
    }
}
