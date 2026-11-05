package dev.horizonlab.javamods.boot;

import java.io.ByteArrayInputStream;
import java.util.List;

/** The boot report's lines and the gate list they read. */
public final class RoutingReportCheck {
    public static void main(String[] args) throws Exception {
        var gates = Routing.gates(new ByteArrayInputStream("# generated\nk/A#a(I)V\tf.rs:1\ta\tgame\t0\nk/A#b()I\tf.rs:2\tb\tgame\t2\nk/A#c()V\tf.rs:3\tc\tgui\n".getBytes()));
        if (!gates.keySet().equals(java.util.Set.of("k/A#a(I)V", "k/A#b()I", "k/A#c()V"))) throw new AssertionError("keys: " + gates);
        if (!Routing.line("k/A#a(I)V", false, true, gates.get("k/A#a(I)V"), List.of("carryon")).equals("Horizon: route k/A#a(I)V edge on gated (carryon)")) throw new AssertionError("gated");
        if (!Routing.line("k/A#b()I", true, true, gates.get("k/A#b()I"), List.of("carryon")).equals("Horizon: route k/A#b()I body on gated (partial: 2 sites) (carryon)")) throw new AssertionError("partial");
        if (!Routing.line("k/A#d()V", false, false, gates.get("k/A#d()V"), List.of("a", "b")).equals("Horizon: route k/A#d()V edge off UNGATED (a, b)")) throw new AssertionError("ungated");
        if (!Routing.summary(List.of("jade: Entity.tick", "lootr: LootTable.fill")).equals("Horizon: 2 behaviours mods change won't take effect (native runs them without the mods' patches): jade: Entity.tick, lootr: LootTable.fill")) throw new AssertionError("summary");
        if (!Routing.gates(null).isEmpty()) throw new AssertionError("no resource lists no gates");
        System.out.println("RoutingReportCheck ok");
    }
}
