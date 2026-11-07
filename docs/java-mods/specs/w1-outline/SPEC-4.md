# SPEC-4 (W1 outline): M2 and W12 bookkeeping for `LevelExtractor#extractBlockOutline`

Read first: DESIGN.md D6; `tools/mod_replaces.py` docstring and `route`; `tools/java_routing.py` (`REASON`, `lint`,
`report`); `tools/java_routing/deferred.tsv`; `tools/java_routing/packs/showcase.tsv`; `tools/port_map/client2-003.tsv`
rows 81–89 (the other `LevelExtractor` verdicts).

Depends on: SPEC-1 (the deferral claims the world recording runs the real method; it must be true first). Size: about 30
changed lines. No code comments involved; TSV rows carry their note in the note column.

## Gap

balm's `@Inject(method = "extractBlockOutline", at = RETURN)` is `routing: undecided` in the showcase report
(`agents/finish/rep-showcase/replaces.json` and `routing.json` `undecided`: `status: mentioned`): the port ledger has no
verdict for `LevelExtractor.extractBlockOutline`, so M2 cannot say whether the patch has an effect. After SPEC-1 it has
one: the world recording calls the real method, merged mixins included, every recording.

## Changes

1. `tools/port_map/client2-003.tsv`, after the other `LevelExtractor` rows (row 89, `LevelExtractor.blockChanged`):
   ```
   LevelExtractor.extractBlockOutline	inline	crates/horizon-client/src/game.rs::Game::render	the aimed block's outline (pick, gate, shape, translucency) is native; mods' hooks on it run in the Java client's world recording with the window's hit (W1)
   ```
   With the verdict `inline`, `mod_replaces.route` routes balm's patch `java`.
2. `tools/java_routing.py` `REASON`: `special: (animate|item_code|sounds|world_frame)`.
3. `tools/java_routing/deferred.tsv`:
   ```
   net/minecraft/client/renderer/extract/LevelExtractor#extractBlockOutline(Lnet/minecraft/client/Camera;Lnet/minecraft/client/renderer/state/level/LevelRenderState;)V	special: world_frame
   ```
4. `tools/java_routing/packs/showcase.tsv`: the key's row as a fresh `java_mods.py replaces --install /home/umceko/hz-poc/agents/w12/install --out OUT`
   writes it in `OUT/routing.tsv` (copy the line; do not hand-write kind, tail or returns).
5. `docs/java-mods-map.md` (the routing section's list of `special:` reasons): `world_frame`: a client method the world
   recording calls itself (W1).

## Tests

- `python3 tools/java_routing.py lint` rc 0 (the new pack row is deferred with a valid reason).
- `python3 tools/java_routing.py report --routing OUT/routing.json` on the fresh showcase run: the key shows as deferred
  `special: world_frame`, not `UNGATED`, and the report's `undecided` count drops by one against
  `agents/finish/rep-showcase` (74 → 73).
- `python3 -m unittest discover -s tests -k routing` if a test pins the `REASON` list (update its expected list if so).

## Done bar

The two tool runs above; no Rust or Java change.

## Live test

None (bookkeeping). Note: the boot's own routing summary reads only `route_gates.tsv` (gates), so it still lists this key
among patches without a gate, as it lists every deferred key; `java_routing.py report` is the M2 authority here.
