# SPEC-22: window gates (W12 phase C): input, the frame loop, debug keys, prediction acks

Design: `DESIGN.md` §2.6 row 22, decisions C4 and C6; W12 `DESIGN.md` §2.5 (the Window context). Needs SPEC-1 (its trace decides the final key list: every `java-client` key it found `MISSING` lands here), SPEC-3. Size: ~450 changed lines; if §3 alone passes 250, split it out as `SPEC-22b`.

Paths as SPEC-2. Read first: `B/ClientBridge.java` (`click`, `events`, the tick op that emulates `runTick`, the frame op), `B/mixin/MinecraftTickMixin.java` (it cancels `handleKeybinds`, `renderFrame`, `handleGlobalKeyPress`, `pauseGame`), `B/mixin/DebugKeysMixin.java`, `B/ClientSounds.java`, `B/ClientWorld.java`, `B/ClientLoader.java` (~174: `Minecraft` is built by `GameBridge.construct` without its constructor), `P/ViewInits.java`; `G/java_screens.rs` (the window's calls into the Java client: clicks, keys, size), `G/remote.rs` (the window's prediction acks), `G/game.rs` `window_event`, `key_pressed`, the debug-key handling, the window's main loop start; vanilla client `Minecraft.java` (`run`, `runTick`, `tick`, `handleKeybinds`, `continueAttack`, `resizeGui`), `KeyboardHandler.handleDebugKeys`, `ClientLevel.syncBlockState`, `BlockStatePredictionHandler`.

Comments in code: one line at most.

## 1. Client routes (the mechanism)
- `Recv::Client` (byte 9): the receiver is the Java client's object for the route's owner: `Minecraft.getInstance()`, `minecraft.keyboardHandler`, `minecraft.mouseHandler`, `minecraft.level` (`ClientLevel`), `minecraft.player`. Projection by owner class, as SPEC-3 §4.
- `Game::client_route_call`, twin of `route_call`: op `client_route` on the window thread, which `ClientBridge` runs synchronously on the Java client's "Render thread" (the way `client_click` runs today), with the window's input already delivered. Effects: the client's sound events, and the `ClientActions` list (§2) the window applies after the reply.
- Route stubs on client classes bridge their native body to the window: natives in `G/java_bridges/client_world.rs` (or a new `client_window.rs`) run on the window thread with the window's `Game`, as client bridges already do.
- `route!` accepts `Recv::Client` only in a `Game` method reached from the window's frame or input handling; `java_routing.py gates` marks such gates `window`, and the lint checks a `window` gate isn't on a server path (the fn isn't reachable from `fn tick_world`/`tick_server`: use the gate list's file and fn names; a conservative rule is fine: the fn's file is one of `game.rs` input functions, `java_screens.rs`, `remote.rs`, `screens.rs`).

## 2. `handleKeybinds` (body; Architectury, Carry On) and `continueAttack` (edge HEAD; Architectury)
Vanilla's `Minecraft.tick` calls `handleKeybinds()`, which reads each `KeyMapping`'s clicks and acts: hotbar selection, inventory, drop, swap hands, chat and command, social interactions, advancements, perspective, smooth camera, and the attack, use and pick clicks (`startAttack`, `startUseItem`, `pickBlockOrEntity`, `continueAttack`). The Java client cancels it; the window does all of it natively and tells Java afterwards. Carry On's `wrap_with_condition` keeps the hotbar slot while carrying; Architectury's inject fires its input events.
- When `routes::patched("…Minecraft#handleKeybinds()V")` (decided once at the Java client's start): `MinecraftTickMixin` no longer cancels `handleKeybinds`, and the window stops performing the actions `handleKeybinds` owns (list them from the vanilla method; each native site checks one flag, `self.java_owns_keybinds`, set at join from the same fact).
- Java's actions reach the window as a list, `ClientActions` (`B/ClientActions.java`, new), appended to the reply of the op that ran `Minecraft.tick`, in call order. Each action is a bridge on the Java client's object vanilla's body calls; the bridge records the action and returns what vanilla's call would (no packet is sent from Java: the window sends its own):
  - `Inventory#setSelectedSlot` on the local player → `SELECT slot`;
  - `Minecraft#setScreen` with an `InventoryScreen`, `ChatScreen`, `AdvancementsScreen`, `SocialInteractionsScreen` → `OPEN kind (+ chat prefix)` (other screens are W5's and open in Java already);
  - `LocalPlayer#drop(boolean)` → `DROP all`; the swap-hands packet send → `SWAP`;
  - `Options` writes for perspective and smooth camera → `PERSPECTIVE n`, `SMOOTH_CAMERA b`;
  - `MultiPlayerGameMode` calls from `startAttack`/`startUseItem`/`pickBlockOrEntity`/`continueAttack` (`attack`, `startDestroyBlock`, `continuePrediction`/`continueDestroyBlock`, `stopDestroyBlock`, `useItemOn`, `useItem`, `interact`, `interactAt`) → `ATTACK id`, `DESTROY_START pos face`, `DESTROY_CONTINUE pos face`, `DESTROY_STOP`, `USE_ON hand hit`, `USE hand`, `INTERACT id hand (at)`; the swing → `SWING hand`.
  The window applies each action with the function that performs it natively today (its hotbar select, screen opens, drop, swap, attack, mining step, use paths), in order, before its own frame continues. `ClientBridge.click`'s pre-check of the window's clicks is not used while `handleKeybinds` runs in Java (it would run them twice).
- Without the `handleKeybinds` route but with `continueAttack` routed: a window gate in the window's continue-mining step, `Recv::Client`, arg `bool` (vanilla's condition), body bridge = the window's continue-mining.

## 3. `runTick` (body; Puzzles Lib's `modify_arg` on `SoundManager.pauseAllExcept`) and `run` (HEAD; Balm)
- `ClientBridge`'s tick op emulates `runTick`'s steps (input, packets, `Minecraft.tick` per owed tick, the timer replaced by the window's). When `runTick` is routed, it calls the real `Minecraft.runTick(false)` instead, with: the timer advance replaced by the window's count (the mixin that does it today), `renderFrame` cancelled (exists), the window's own frame pacing left native. Inside, vanilla's pause handling calls `soundManager.pauseAllExcept(…)` (with Puzzles Lib's changed argument) and `resume`: `ClientSounds` forwards pause and resume to native as events (add them if missing), and the window's own pause-sound handling is skipped while this route is on, so the sound pauses exactly once, with the mod's exceptions.
- Per-frame Java work: this keeps today's call count (the op already runs per window tick); `runTick` does not add a per-frame render path.
- `Minecraft#run` (C6): at the window's main loop start, op `client_run`: Java invokes the merged `run()` whose route stub is Java-bodied and returns at once (the loop is the window's). A RETURN or TAIL change on `run` fails the boot: `route Minecraft#run: changes after a main loop are not supported`.

## 4. Smaller window gates
- `Minecraft#resizeGui()V` (konkrete HEAD): the window's GUI rescale (where it sends its new size to the Java client) becomes `client_resize_gui()`, gate first; body bridge = the window's rescale and the size send.
- `KeyboardHandler#handleDebugKeys(…KeyEvent;)Z` (glitchcore RETURN): the window's F3 combination handler becomes `client_debug_keys(key) -> bool`, gate first (`Recv::Client`, a `KeyEvent` argument: `int key, int scancode, int modifiers`, built as `ClientBridge.events` builds them); body bridge = the window's handler. `DebugKeysMixin` keeps cancelling the Java body for unrouted packs only.
- `ClientLevel#syncBlockState(…BlockPos;…BlockState;…Vec3;)V` (lootr, body): the window's acknowledgement of a predicted block (where `remote.rs` resets a predicted position to the server's state) calls `client_sync_block_state(pos, state, player_pos)`, gate first; Java runs vanilla's body on its `ClientLevel`, whose `setBlock` writes the window's chunk through `ClientWorld`'s bridges (check one exists for `setBlock`; add it if not).

## 5. Keys SPEC-1's trace moves here
For each `MISSING` key from SPEC-1 §6, one of:
- `Minecraft#<init>` (konkrete RETURN): add `Minecraft` to the classes `ViewInits` processes, and run its kept constructor injections at the end of `ClientLoader`'s constructor steps (where the client counts as constructed), as views run theirs once filled.
- `ClientPacketListener#handleRecipeBookAdd`/`Remove` (architectury RETURN): the Java client gets the recipe book packets the server sends this player (the faithful-client rule): forward them as `Menus` forwards the login's packets. No gate.
- any other: a window gate as §4, at the window site where vanilla calls it.

## 6. Tests
- Test mod mixins (client side, `J/testmod/…/mixin/client/`):
  - `handleKeybinds` `@WrapWithCondition` on `Inventory.setSelectedSlot`: false while the player holds `minecraft:barrier` in the main hand;
  - `continueAttack` HEAD: count; `run` HEAD: set a flag; `runTick` `@ModifyArg` on `pauseAllExcept`: add `SoundSource.RECORDS`;
  - `resizeGui` HEAD: count; `handleDebugKeys` RETURN: record the key; `syncBlockState` `@WrapOperation` on `setBlock`: count.
- Client checks (`ChecksClient` or the client half of `bridges`):
  - `hotbar_locked_by_barrier`: holding a barrier, `horizon-ctl key 3` leaves the selected slot unchanged on the window (`state`) and on the server; without the barrier it changes;
  - `actions_once`: one `E` press opens the native inventory once; one drop key press drops one item (no double action);
  - `run_head_once`, `resize_gui_counted` (`horizon-ctl` resize, if the offscreen window supports it; else the lead's live step), `debug_key_seen` (F3+B pressed through `key`), `sync_block_state_on_ack` (place and break a block quickly: one or more counts).
- `lan.sh` input checks and `agent_control_e2e.py` (horizon-ctl input paths) unchanged without mods.
- `cargo test` of the touched modules; clippy both ways; `java_mods.py build/check/unit`; one `bridges` run.

## Done bar
Checks pass; every key in this spec gated or handled per §5; `java_routing.py lint` (with the `window` gate kind) passes; SPEC-1's `trace-check` on a new trace shows `ok` for every key moved by §5's first two bullets.

## For the user (needs real input)
- Carry On: pick up a pig (shift + right click), then press number keys and scroll: the held slot doesn't change while carrying, as on NeoForge. Put the pig down: number keys work again.
- With the showcase pack, every normal key: inventory (E), drop (Q, Ctrl+Q), swap hands (F), chat (T, /), perspective (F5), hotbar 1–9, attack, mining (hold), placing, middle click. Each acts exactly once, with no delay you can feel.
- Open the pause menu while music plays: sounds pause as on NeoForge with Puzzles Lib's settings.
- F3+B shows hitboxes; F3+H advanced tooltips; same as before.
