package dev.horizonlab.javamods.boot;

import java.io.ByteArrayInputStream;
import java.util.List;

/** The boot report's lines and the gate list they read. */
public final class RoutingReportCheck {
    public static void main(String[] args) throws Exception {
        var gates = Routing.gates(new ByteArrayInputStream("# generated\nk/A#a(I)V\tf.rs:1\ta\tgame\t0\nk/A#b()I\tf.rs:2\tb\tgame\t2\nk/A#c()V\tf.rs:3\tc\tgui\n".getBytes()));
        if (!gates.keySet().equals(java.util.Set.of("k/A#a(I)V", "k/A#b()I", "k/A#c()V"))) throw new AssertionError("keys: " + gates);
        if (!Routing.line("k/A#a(I)V", false, true, false, gates.get("k/A#a(I)V"), null, List.of("carryon")).equals("Horizon: route k/A#a(I)V edge on gated (carryon)")) throw new AssertionError("gated");
        if (!Routing.line("k/A#b()I", true, true, false, gates.get("k/A#b()I"), null, List.of("carryon")).equals("Horizon: route k/A#b()I body on gated (partial: 2 sites) (carryon)")) throw new AssertionError("partial");
        if (!Routing.line("k/A#d()V", false, false, false, gates.get("k/A#d()V"), null, List.of("a", "b")).equals("Horizon: route k/A#d()V edge off UNGATED (a, b)")) throw new AssertionError("ungated");
        if (!Routing.line("k/A#e()V", false, true, true, null, null, List.of("a")).equals("Horizon: route k/A#e()V edge on inert (guard) (a)")) throw new AssertionError("inert");
        if (!Routing.line("k/A#f()V", false, true, false, null, "owner: W9 3-2", List.of("a")).equals("Horizon: route k/A#f()V edge on deferred (owner: W9 3-2) (a)")) throw new AssertionError("deferred");
        if (!Routing.line("k/A#g()V", false, true, false, null, "verdict: java-client", List.of("a")).equals("Horizon: route k/A#g()V edge on java-client (a)")) throw new AssertionError("verdict");
        if (!Routing.line("k/A#a(I)V", false, true, false, 0, "owner: W5", List.of("a")).equals("Horizon: route k/A#a(I)V edge on gated (a)")) throw new AssertionError("a gate wins over a status");
        if (!Routing.waiting(List.of("spec: w12-coverage SPEC-6", "owner: W10", "owner: W5", "owner: W9 3-2", "owner: W10")).equals("Horizon: 5 routes wait for their owner: W5 1, W9 1, W10 2, w12-coverage 1")) throw new AssertionError("waiting");
        var statuses = Routing.statuses(new ByteArrayInputStream("# generated\nk/A#f()V\towner: W9 3-2\nk/A#g()V\tverdict: java-client\n".getBytes()));
        if (!statuses.equals(java.util.Map.of("k/A#f()V", "owner: W9 3-2", "k/A#g()V", "verdict: java-client"))) throw new AssertionError("statuses: " + statuses);
        if (!Routing.statuses(null).isEmpty()) throw new AssertionError("no resource lists no statuses");
        if (!Routing.summary(List.of("jade: Entity.tick", "lootr: LootTable.fill")).equals("Horizon: 2 behaviours mods change won't take effect (native runs them without the mods' patches): jade: Entity.tick, lootr: LootTable.fill")) throw new AssertionError("summary");
        if (!Routing.gates(null).isEmpty()) throw new AssertionError("no resource lists no gates");
        System.out.println("RoutingReportCheck ok");
    }
}
