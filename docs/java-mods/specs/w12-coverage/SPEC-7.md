# SPEC-7: entity save streams (Waystones, Balm, Clumps, Carry On)

Design: `DESIGN.md` §2.5 (`ValueInput`/`ValueOutput`), §2.6 row 7. Needs SPEC-3 (save-stream arguments and written outputs), W9 3-1 (`Recv::Entity`). Size: ~350 changed lines.

Paths as SPEC-2. Read first: `B/EntityViews.java` (`create`, `join`, `save`), `G/java_entities.rs` (`java_entity_joining`, `java_entities_save`, `java_merge_owned`), `G/persist.rs` (`save_entities`, `entity_chunks`), `G/entity_life.rs` (`Origin`), `horizon-client-lib`'s `entity_nbt.rs` (`orb_to_nbt` and the per-kind writers), `P/BridgeProcessor.java` (Java-bodied entries: `Entry.java()`, `javaBody`, `guiRoutes`), and vanilla `Entity.saveWithoutId`, `Entity.load`, `ExperienceOrb.addAdditionalSaveData`/`readAdditionalSaveData`, `Player.readAdditionalSaveData`, `EntityType.create(ValueInput, …)`, `EntityType.loadEntityRecursive`, `SpawnEggItem`/`EntityType.updateCustomEntityTag`.

Comments in code: one line at most.

## Keys
| Key | Kind | Mod | How it is served |
|---|---|---|---|
| `Entity#saveWithoutId(Lnet/minecraft/world/level/storage/ValueOutput;)V` | edge HEAD | waystones | §2, gated in the batch save |
| `ExperienceOrb#addAdditionalSaveData(…ValueOutput;)V` | edge TAIL | clumps | §2, nested inside the orb's save body |
| `Entity#load(…ValueInput;)V` | edge HEAD | balm, waystones | §1, the view's creation is vanilla's load |
| `ExperienceOrb#readAdditionalSaveData(…ValueInput;)V` | edge TAIL | clumps | §1, same |
| `Player#readAdditionalSaveData(…ValueInput;)V` | edge RETURN | carryon | §3, gated at native's player load |

## 1. Loads: the view's creation runs `load` exactly when vanilla does
`EntityViews.create` builds every view with `EntityType.create(input, …)`, which runs `Entity.load`, so the mods' `load` and `readAdditionalSaveData` handlers already run for views. That is right for an entity vanilla loads (from disk, `/summon`, a spawn egg with entity data, structure entities), and wrong for one it makes fresh (natural spawns, drops, breeding): vanilla never calls `load` for those, and Balm's or Waystones' HEAD handlers would fire where NeoForge fires none.
- `G/entity_life.rs` `Origin` gains `Summoned`: native's paths that vanilla runs through `EntityType.loadEntityRecursive`/`create(ValueInput…)` or `updateCustomEntityTag` with data. Find them: `spawn_entity_from_nbt` (summon, Java spawns), spawn eggs with `entity_data`, structure template entities. Pass `Summoned` there; everything else stays `Fresh`.
- `entity_view` sends `byte origin` (0 fresh, 1 disk, 2 summoned) instead of `bool fromDisk` (keep `EntityJoinLevelEvent`'s `loadedFromDisk` = origin 1).
- `EntityViews.create`: origin 1 or 2 → today's path (`EntityType.create(input, level, …)`). Origin 0 → `EntityType.create(level, EntitySpawnReason)` (the reason native's site maps to; `EVENT` when unknown is **not** allowed: add the reason byte to the request), `setId`, backed, then the groups native sends with the join (W9 1c's mask), raw. No `load`.
- Verdict rows for `Entity#load` and `ExperienceOrb#readAdditionalSaveData` in `verdicts.tsv`: `java-view`, evidence `SPEC-7 checks load_handler_*`. Add `java-view` to SPEC-1's verdict list.

## 2. Saves: `saveWithoutId` and `addAdditionalSaveData` in Java around native's encoding
Today `entities_save` (`EntityViews.save`) returns only attachments and persistent data, so a handler that writes its own keys in `saveWithoutId` (Waystones' warp-plate state) is lost at every save.
- **Java-bodied stubs for save streams.** Generalise W5's Java-bodied bridge rows: `gui_bridges.tsv` rows get an optional third column, the body class (default `GuiRoutes`). Add `J/boot/res/gui_bridges.tsv` rows (or a sibling `save_bridges.tsv` that `BridgeProcessor` reads the same way) for the two save keys with body class `dev/horizonlab/javamods/boot/SaveRoutes`. Their stub (edge-routed, so W12's exact-TAIL stub) calls `SaveRoutes.body(seam, this, args)`.
- **`B/SaveRoutes.java` (new):** `body("entity_save", entity, [output])` calls `static native byte[] entitySaveBody(int id)` and merges the compound into the output (`TagValueOutput`'s tag through an accessor mixin; `CompoundTag.merge`). `body("orb_additional", orb, [output])` the same with `orbAdditionalBody(int id)`. Both are JNI natives in `G/java_bridges/entities.rs` (Horizon-class natives, as SPEC-2's `stacksWritten`).
- **Native bodies (`G/java_entities.rs`):**
  - `entity_save_body(id) -> Bytes`: the entity's encoding as `entity_chunks` writes it today, **without** the Java-owned keys (`JAVA_KEYS`), since Java writes those itself after the body (they are `saveWithoutId`'s own NeoForge tail: attachments and persistent data).
  - For orbs, the encoding is split: `orb_to_nbt` base keys, then `orb_additional_save(id) -> Tag`, a `Game` method with the gate first: `route!(self, "…ExperienceOrb#addAdditionalSaveData(…)V", Recv::Entity(id), (OutputArg))`, reading the written output (SPEC-3 `route_call_out`). Its body fn writes `Value`, `Count`, `Health`, `Age` as `orb_to_nbt` does now. Inside `entity_save_body` this is a nested crossing, which W12 §2.5 allows.
- **The batch:** `java_entities_save` sends one more bit per call: `saveWithoutId` routed (`routes::patched` of the key, SPEC-3). Java's `save`, for each listed id, when the bit is set: `var out = TagValueOutput…; view.saveWithoutId(out); keys = out.buildResult()` (the full compound: native's body, the handlers' keys, attachments); otherwise today's attachments-only path. Native takes a full compound as the entity's saved form for this save (it replaces native's own encoding for that entity), and the attachments-only form as today. The order of ids is native's save order.
- Players: their save isn't in `entities_save`; no key in this pack routes a player save, so nothing changes there.

## 3. `Player#readAdditionalSaveData` at native's player load
- Find where native loads a joining player's saved data (`save.rs` playerdata read and the `Game` method that applies it, for the window's own player and for guests through `as_guest`). Make the part vanilla's `readAdditionalSaveData` covers a `Game` method with the gate first: `route!(self, "…Player#readAdditionalSaveData(…)V", Recv::Player, (InputArg(&tag)))`, body fn = today's code.
- The `tag` must include the Java-owned keys the player had at its last save. If native keeps none for players (check how player attachments are saved today), report it: that is W13's `PlayerEvent.SaveToFile`/`LoadFromFile` gap, and this route's handler then reads only vanilla keys. Don't invent a store.
- The route's fill creates the player's view if needed (SPEC-2 FULL), which is vanilla's order (the player object exists before `load`).

## 4. Tests
- Test mod mixins (`J/testmod/…/mixin/SaveRouteMixin.java`):
  - `@Inject(method = "saveWithoutId", at = @At("HEAD"))` on `Entity`: for entities tagged `horizon_save`, `output.putString("horizon_test", "saved")`;
  - `@Inject(method = "load", at = @At("HEAD"))` on `Entity`: counts calls per entity type and remembers whether `input.getString("horizon_test")` was `saved`;
  - `@Inject(method = "addAdditionalSaveData", at = @At("TAIL"))` on `ExperienceOrb`: `output.putInt("horizon_orb", 7)`.
- `ChecksEntities`/`ChecksRouting` (`later` and `afterChange`, the second session reloads the world):
  - `save_handler_key_round_trips`: a pig tagged `horizon_save` in session 1; in session 2 the `load` handler saw `saved` for it;
  - `orb_additional_key_saved`: `/data get entity @e[type=experience_orb,limit=1] horizon_orb` gives 7 after a save (`/save-all`), via the native `data` command on the saved form;
  - `load_handler_not_on_fresh_spawn`: the `load` count for `minecraft:item` stays 0 after the session drops 5 items, and for `minecraft:pig` is 1 after one `/summon minecraft:pig` (vanilla: summon loads, drops don't);
  - `load_handler_on_disk_load`: after the reload, the pig's count is 1 again.
- Rust: `orb_save_splits_into_base_and_additional` (the two halves merged equal today's `orb_to_nbt` for a fixed orb; expected compound written out from vanilla's field names); `origin_byte_mapping`.
- `python3 tools/parity.py save_roundtrip server_entities` (no route without mods: unchanged); `cargo test -p horizon-client --features java-mods --lib java_entities`; clippy both ways; `java_mods.py build/check/unit`; one `bridges` run (two sessions).

## Done bar
Checks pass; gates for `saveWithoutId`, `ExperienceOrb#addAdditionalSaveData` and `Player#readAdditionalSaveData` are listed by `java_routing.py gates`; `verdicts.tsv` holds the two `java-view` rows; lint passes.

## For the lead and the user
- Lead: the save harnesses above.
- User (Waystones): place a warp plate, stand on it until it charges (Waystones' particle effect), save and quit while a mob stands on it (`/summon minecraft:pig` on the plate), reload. On NeoForge the pig keeps its warp-plate cooldown state (it isn't immediately warped again); same on Horizon.
- User (Clumps): `/summon experience_orb ~ ~1 ~ {Value:5}` ×20, wait for Clumps to merge them into one, `/save-all`, quit, reload, pick it up: the XP gained equals 100 points (Clumps keeps the merged count in its save key).
