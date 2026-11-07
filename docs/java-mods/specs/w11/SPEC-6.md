# SPEC-6 (W11): global loot modifiers through NeoForge's own modifyLoot

Read first: `DESIGN.md` G4, D4, D5, §8; `SPEC-1.md` (modifier facts), `SPEC-5.md` (the hook, `RandomState`,
`LootStack::to/from_codec_json`, `bridge::isolated`, `NativeLootPlace.this_entity/killer`); vanilla `LootParams.Builder`,
`LootContext.Builder` (`withOptionalRandomSource`), `LootContextParams`, `LootContextParamSets`, `LegacyRandomSource`
(`setSeed` scrambles with `0x5DEECE66D`), `XoroshiroRandomSource(long, long)` and its `CODEC`; javap `CommonHooks.modifyLoot`
(it sets the context's queried table id and reads the `LootModifierManager` from the current server's resources);
`B/EntityViews.java` (finding a view by id), `B/GameBridge.java` `level`, `PLAYERS`.

Depends on: SPEC-1, SPEC-5.

Comments in code: one line at most, only for a non-obvious why.

## Goal

Every native roll of a table a loaded modifier can apply to runs NeoForge's real `CommonHooks.modifyLoot` on the rolled
stacks, with a `LootContext` holding the roll's parameters and a random that continues native's sequence; the modified
stacks and the advanced random come back before native goes on (a chest's shuffle, the drops' spawn).

## Wire: op `loot_modify` (always called inside `bridge::isolated`)

Request: `text table` ("" = inline), `text dim` (`place.level.dimension`, else `minecraft:overworld`), `byte random`
(1 legacy + `long state`, 2 xoroshiro + `long lo` + `long hi`), `bool origin` + 3 doubles, `int thisId` (-1 none) + `long
uuidHi` + `long uuidLo`, `bool killer` + 2 longs, `bool damage` + `text damage type` (from `NativeLootContext.damage`) ,
`text tool` (codec JSON or ""), `int blockState` (Java id via the `StateMap`, -1 none), `int len` + NBT of the block
entity (0 none), `bool explosion` + `float radius`, `float luck`, `int n` + n `text` stacks (codec JSON).
Reply: `byte random` + its words (as in the request), `int n` + n `text` stacks (codec JSON). An empty reply (Java threw)
leaves the roll as it was.

## Java: `B/Loot.java` (new)

`static void modify(Crossing.In in, Crossing.Out out)`, `Crossing.game("loot_modify", Loot::modify)`:
1. `var level = GameBridge.level(dim)`; `var ops = level.registryAccess().createSerializationContext(JsonOps.INSTANCE)`.
2. Random: legacy → `var r = new LegacyRandomSource(0); r.setSeed(state ^ 0x5DEECE66DL)` (one-line comment: `setSeed`
   scrambles, so this gives the raw state back); xoroshiro → `new XoroshiroRandomSource(lo, hi)`.
3. Params: `new LootParams.Builder(level)`; `withParameter(ORIGIN, vec)` when present; `THIS_ENTITY` = the entity view
   with that id (`level.getEntity(id)`) or, for a player uuid, `server.getPlayerList().getPlayer(uuid)` (the view);
   `LAST_DAMAGE_PLAYER` likewise; `DAMAGE_SOURCE` = `new DamageSource(registry holder of the type)` (W9 phase 4 will give
   its entities); `TOOL` = the parsed stack; `BLOCK_STATE` = `Block.stateById(id)`; `BLOCK_ENTITY` =
   `BlockEntity.loadStatic(pos of origin, state, tag, registries)`; `EXPLOSION_RADIUS`; `withLuck`. Build with
   `create(LootContextParamSets.ALL_PARAMS)` so no table's required set rejects it. A parameter that can't be resolved (no
   view for that id) is left out and logged once per kind with `HORIZON_JAVA_DEBUG`.
4. `var ctx = new LootContext.Builder(params).withOptionalRandomSource(r).create(Optional.empty())`.
5. Stacks: `ItemStack.CODEC.parse(ops, JsonParser.parseString(s))` each, into an `ObjectArrayList`.
6. `var id = table.isEmpty() ? LootTableIdCondition.UNKNOWN_LOOT_TABLE : Identifier.parse(table)`;
   `var result = CommonHooks.modifyLoot(id, list, ctx)` inside `GameBridge.safely`-style handling: a throw is logged as
   `Horizon: loot modifiers threw on <table>: <message>` and the reply is empty. A throw whose cause is a bridged native
   with no game (`bridge` throws its "no game" exception) is logged as DESIGN D4's line, once per modifier class.
7. Read the random back: legacy through `B/mixin/LegacySeedAccess` (`@Accessor("seed") AtomicLong`), xoroshiro through
   `XoroshiroRandomSource.CODEC.encodeStart(NbtOps.INSTANCE, r)` (two longs) or accessors to
   `randomNumberGenerator.seedLo/seedHi`. Write it and the stacks (`ItemStack.CODEC.encodeStart(ops, s)` as text).
8. Debug line: `Horizon: loot_modify <table> <n in> -> <n out>`.

## Rust: `G/java_loot.rs` (new, `java-mods` only) and `G/java_hooks.rs`

- `pub(crate) fn java_hook_loot_wanted(table: &str) -> bool`: `craft_facts().is_some_and(|f| f.modifiers_for(table))`; an
  inline table (`""`) is wanted only by an "any" modifier.
- `pub(crate) fn java_hook_loot_rolled(table, ctx, tool, random, stacks)`: builds the request (block state through
  `horizon_java_host::bridge::STATES`' `to_java`, block entity tag through `java_mods::write_tag`), calls
  `horizon_java_host::bridge::isolated(|| host.request("loot_modify", &bytes))`, parses; sets `*random`; for each returned
  stack, if its JSON equals the JSON sent at the same index, keep the original `LootStack`, else
  `LootStack::from_codec_json` (a stack that doesn't parse is logged and dropped, as vanilla's codec would fail).
- No game is used: these are free functions over `horizon_java_host::get()` and the facts.

## Tests

- **Test mod** (`TM/res/data/horizon_java_test/loot_modifiers/`): SPEC-1's `sand.json`, plus `gravel.json`: type
  `horizon_java_test:add_item`, condition `all_of [loot_table_id minecraft:blocks/gravel, random_chance 0.5]`, item
  `minecraft:flint`. Check `ChecksServer` `loot_modify` calls `Loot.modify` with a hand-built request: table
  `minecraft:blocks/sand`, one sand stack, legacy state 12345: reply holds sand then a gold nugget and the same state (the
  modifier draws nothing). Table `minecraft:blocks/gravel`, state S: flint is present iff
  `new LegacyRandomSource(0) {setSeed(S ^ 0x5DEECE66DL)}.nextFloat() < 0.5f` computed in the check with vanilla's class,
  and the returned state equals that random's state after its one draw (read through the same accessor). Run for 8 states.
- **Rust**: the request for a context with origin, a player `this_entity`, a tool and two stacks equals a hand-built `Out`;
  a reply that returns the first stack's JSON unchanged keeps the original `LootStack` (with `exploration` set) and parses
  the second.
- **Scenario** `T/scenarios/w11-loot.json` (showcase install, `HORIZON_JAVA_DEBUG=1`): forceload and build a floor first
  (RULES), then 100 × `/loot spawn ~ ~1 ~ loot minecraft:chests/simple_dungeon` (or `/loot give @s` if inventory space
  allows; chunk the commands), `{"check": {"log": "Horizon: loot_modify minecraft:chests/simple_dungeon"}}`,
  `{"check": {"no_exceptions": true}}`. Count `travelersbackpack:standard` among item entities (`horizon-ctl entities 16
  item`); expected 6 ± 7 per 100 rolls from the modifier's `random_chance 0.06` (binomial, 3σ): the check is that the
  count is ≥ 1 over 200 rolls (P(0) = 0.94^200 ≈ 4e-6).
- `cargo test -p horizon-client --features java-mods --lib java_loot`, `java_mods.py build`, `check`, `bridges`.

## Done bar

- [ ] `bridges` prints `CHECK loot_modify ok`; the scenario passes.
- [ ] Breaking sand in a test-mod run drops sand and a gold nugget (`java_mods.py play ... "setblock ~ ~-1 ~ sand"`, break).
- [ ] A world with only install-rs (no modifiers) never logs `loot_modify` (the facts answer no table).
- [ ] M3: on the showcase install and Forge with the same seed, the first opened chest of the same desert pyramid holds the
      same stacks in the same slots (the random continues native's sequence). Record the result in the report; a mismatch
      is a bug for this spec only if vanilla's slots match without modifiers.

## Live steps for the lead

1. Showcase install, creative: `/locate structure minecraft:mineshaft`, go there, open 10 chests; expect, over enough
   chests, Traveler's Backpack upgrades or a standard backpack among the loot, as on Forge (6% and 5% a chest).
2. `/loot give @s loot minecraft:chests/simple_dungeon` 50 times: a few `travelersbackpack:standard` appear.
