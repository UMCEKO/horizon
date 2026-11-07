# SPEC-7 (W4): mods' mixins into `FogRenderer#setupFog` run around native's fog

Design: `DESIGN.md` §1.3 (Jade row), §2.4, D3. Size: ~260 changed lines (Java ~150, Python ~50, Rust ~60). Depends on: SPEC-6;
W5 SPEC-7 §1 (GUI bridges: `RES/gui_bridges.tsv`, `GuiRoutes.backed`/`body`, `gui_route!`) and W12 SPEC-2 (route stubs,
`routes.bin`, `Routing.route/kind/on`). If W5 SPEC-7 §1 has not landed, this spec waits.

Read first: W12 `DESIGN.md` §2.1–2.2 and `SPEC-1.md` (classifier), `SPEC-2.md` (stubs, `RouteStubCheck`), W5 `SPEC-7.md` §1,
`B/GuiRoutes.java`, the plugin's `BridgeProcessor`, `T/mod_routing.py` / `T/mod_replaces.py` (where W12 classifies changes),
`T/java_routing/deferred.tsv` (the `FogRenderer#setupFog` row, reason `client: phase C`), `B/ViewFrame.java` (SPEC-6),
`G/render_tick.rs` `frame_fog`. Vanilla: `FogRenderer.setupFog` (its `FogData fog` local is the value returned at the method's only
`areturn`). Jade's handler (javap): `jade$setupFog(Camera, int, DeltaTracker, float, ClientLevel, CallbackInfoReturnable, FogData)`,
`@At("TAIL")`, `locals = true`.

Code comments: one line at most.

## Goal

A mod's `inject` at RETURN or TAIL of `setupFog` that captures only the arguments and the returned `FogData` runs, in Java, around
native's fog: Mixin calls it with the `FogData` native computed (through the two NeoForge events, SPEC-6), and what it changes in
that object is the frame's fog. The vanilla body of `setupFog` never runs in Java. M2: the Jade key leaves `deferred.tsv`.

## 1. Classifier rule (W12's analysis; `T/mod_routing.py` or wherever SPEC-1 of W12 decides `edge`/`body`, ~50 lines)
An `inject` at `RETURN` or `TAIL` with captured locals is **edge (return-local)** when, in the target method's bytecode (vanilla jar):
every `*return` instruction is immediately preceded by a load of the same local slot N, and the handler's captured locals are exactly
the method's argument slots plus N (by the handler's descriptor after the `CallbackInfo` parameter: one extra parameter whose type
is the local's type). Record `return_local: N` in the route row. Anything else with locals stays `body`.
Test in `tests/test_mod_routing.py` (or W12's test file): a javac-built target `Object f(int a) { Object r = new Object(); return r; }`
with a TAIL handler `(int, CallbackInfoReturnable, Object)` → edge, `return_local 2`; a handler capturing another local → body; a
method with two returns of different locals → body.

## 2. The stub (plugin `BridgeProcessor`, ~60 lines)
For an edge route row with `return_local N` on a GUI-bridge key: the stub stores the body call's result into slot N (`astore N`)
before `goto tail`, and the method's `LocalVariableTable` entry for N is extended to cover the tail. So at `tail`, where Mixin put the
TAIL handlers, slot N holds the returned value on both paths, and Mixin's local capture finds it.
Extend `J/unit/dev/horizonlab/javamods/plugin/RouteStubCheck.java`: a test class shaped like `setupFog` with a TAIL handler capturing
the returned local gets, through the stub, the value the stub's body returned (the handler records it); the body's original code
doesn't run (it would throw).

## 3. The seam (`RES/gui_bridges.tsv`, `B/GuiRoutes.java`, `B/ViewFrame.java`; ~90 lines)
- Row: `net/minecraft/client/renderer/fog/FogRenderer#setupFog(Lnet/minecraft/client/Camera;ILnet/minecraft/client/DeltaTracker;FLnet/minecraft/client/multiplayer/ClientLevel;)Lnet/minecraft/client/renderer/fog/FogData;	fog`.
- `GuiRoutes.backed(this)` is true for the client's `FogRenderer` while `ViewFrame.fogStep` runs (a flag), so no other call reaches the
  seam; `GuiRoutes.body("fog", this, args)` returns `ViewFrame.nativeFog(args)`: SPEC-6's two steps (`getFogColor` into a fresh
  `FogData`'s colour, the six distances from the inputs, `onSetupFog`) and returns that `FogData`.
- `ViewFrame.take`'s fog step: when `Routing.on(Routing.route(KEY))`, call the real method
  `((GameRendererAccess) minecraft.gameRenderer).horizon$fogRenderer().setupFog(camera, rd, ClientBridge.timer(minecraft), darken,
  minecraft.level)` (new accessor `B/mixin/GameRendererAccess.java` for the field), with `fogStep = true` around it, and read the
  outputs from the returned `FogData`; otherwise SPEC-6's direct path. The deltaTracker's residual is set to the inputs' partial
  first (`DeltaTrackerAccess.horizon$residual`), as `ClientBridge.advance` does.
- `ViewFrame.fogWanted()` ORs "the route is on", so Jade alone turns the view job on.

## 4. Rust (~60 lines)
- `G/render_tick.rs` `frame_fog`: `gui_route!("net/minecraft/client/renderer/fog/FogRenderer#setupFog(...)")` on the
  `java_hook_view_fog` call (the full key as in the row), so `java_routing.py gates` lists it.
- `T/java_routing/deferred.tsv`: remove the row. `java_routing.py lint` passes.
- `java_routing.py report` on both installs: the key reads `gated`, no longer `deferred (client: phase C)`; the boot line reads
  `Horizon: route …setupFog… edge on (jade) served by view fog`.

## Test mod
Mixin `testmod/.../mixin/FogTailMixin.java` (in `testmod/res/horizon_java_test.mixins.json`): `@Inject(method = "setupFog", at =
@At("TAIL"), locals = LocalCapture.CAPTURE_FAILHARD)` capturing the `FogData` like Jade's; while the player holds
`horizon_java_test:fog_lens`, sets `fog.cloudEnd = 3f`; always records that it ran and the `fog.renderDistanceEnd` it saw.
Checks `fog_tail_sees_native_fog`: the recorded `renderDistanceEnd` equals `R × 16` (SPEC-6's independent value) and the handler ran
once per view firing; `fog_tail_changes_reach_native`: with the lens held, the next status's fog output has `cloudEnd == 3`.

## Tests
- Python classifier cases (§1); `RouteStubCheck` (§2); `java_mods.py build` (private dir), `unit`, `check --analysis`, `bridges`.
- `python3 T/java_routing.py lint`, `report` on install-rs and the showcase.
- `cargo test -p horizon-client --features java-mods --lib java_view`, clippy with and without the feature.

## Done bar
All pass; the report shows the `setupFog` key gated on both installs; the boot log line above appears with Jade.

## Live test (lead; play install with Jade)
1. Look at a block with Jade's overlay at night, in water and in lava: fog is identical to before this spec (Jade's TAIL reads it);
   Jade behaves as on Forge where it depends on fog (compare its overlay under water with a Forge client).
2. Boot log: no `UNGATED` line for `FogRenderer#setupFog`.
