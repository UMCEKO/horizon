# SPEC-12: other entity ticks (Architectury, Lootr, Sophisticated Core)

Design: `DESIGN.md` §2.6 row 12. Needs SPEC-4, SPEC-8 §1 (field bridges), W9 3-1, W9 2-10 (turns for bolts and falling blocks), W9 2-6/2-8 (mob turns, for allays). Size: ~350 changed lines.

Paths as SPEC-2. Read first: vanilla `LightningBolt.tick`, `FallingBlockEntity.tick` (and `Fallable.onLand`'s callers), `Allay.aiStep`, `Allay.shouldStopDancing` and its jukebox logic; W9 2-10's `turn_bolt`/`turn_falling_block` (or whatever 2-10 names them), `G/lightning.rs`, the falling-block code (`G/` grep `falling_block`), the allay code (`crates/horizon-client/src/mobs/` grep `allay`, `G/creatures.rs` `tick_allays` before W9 2-8 moves it); the closures in `check --analysis`.

Comments in code: one line at most.

## Keys
| Key | Kind | Mods | Seam |
|---|---|---|---|
| `LightningBolt#tick()V` | body (inject after `getEntities`) | architectury | the bolt's turn calls `bolt_tick(id)`, gate first, body = native's bolt tick |
| `FallingBlockEntity#tick()V` | body (inject before `Fallable.onLand`; wrap `BlockEntity.setChanged`) | architectury, lootr | the falling block's turn calls `falling_block_tick(id)`, gate first |
| `Allay#aiStep()V` | edge TAIL | sophisticatedcore | the allay's turn, where vanilla's `aiStep` ends: `allay_ai_step(id)`, gate first, body = native's allay `aiStep` port |
| `Allay#shouldStopDancing()Z` | edge HEAD | sophisticatedcore | `allay_should_stop_dancing(id) -> bool`, where native's dance logic asks it |

## Body routes: what running vanilla's tick in Java needs
`LightningBolt.tick` and `FallingBlockEntity.tick` run in Java on the view, for every admitted entity (every bolt and every falling block, unless SPEC-4 narrows), with their callees bridged:
- **Closure.** `check --analysis` lists them (22 and 21 in the 11-06 report: `getEntities` → W9 query, `baseTick` → native, `gameEvent`, `distanceTo`, `getStrikePosition`, `clearCopperOnLightningStrike`, `spawnAtLocation`, `handlePortal`, `callOnBrokenAfterFall`, `applyEffectsFromBlocks`…). Each gets a bridge to native's port of it; a callee native hasn't ported is a W17 P2 port (report it with the key, don't stub it).
- **Field writes.** `life`, `flashes`, `seed`, `visualOnly` (bolt) and `time`, `blockState`, `cancelDrop`, `dropItem`, `hurtEntities`, `fallDamageMax`, `fallDamagePerDistance`, `blockData` (falling block): rows in `field_bridges.tsv` (SPEC-8 §1) writing native's fields. Check the 26.3 names in the source.
- **Movement and block writes** inside the bodies (`move`, `setBlock`, `removeBlock`) already have bridges (W9 1c2d `move`, level bridges).
- **Order.** The body replaces the native tick for that entity in its turn; nothing native runs for that entity's tick besides what the body calls back.

## Tests
- Test mod mixins (`J/testmod/…/mixin/TickRouteMixin.java`):
  - `LightningBolt#tick`: `@Inject(at = @At(value = "INVOKE_ASSIGN", target = "…getEntities(…)"))` records the list size;
  - `FallingBlockEntity#tick`: `@Inject(at = @At(value = "INVOKE", target = "…Fallable;onLand(…)V"))` counts landings by block;
  - `Allay#aiStep` TAIL counts; `Allay#shouldStopDancing` HEAD returns `false` for allays tagged `horizon_dance`.
- `ChecksEntities`:
  - `bolt_body_finds_entities`: `/summon lightning_bolt` next to a pig: the handler saw a list with the pig, the pig became a zombified piglin (vanilla's `thunderHit`), the bolt removed itself after its life;
  - `falling_sand_lands_once`: `/setblock ~ ~10 ~ sand` above air with a block below the gap, update it: one landing for `minecraft:sand`, sand at the bottom;
  - `anvil_lands_and_damages`: an anvil dropped on a zombie: one landing, the zombie hurt by the falling block damage (`fallDamagePerDistance` path through the bridges);
  - `allay_keeps_dancing`: a `horizon_dance` allay next to a playing jukebox keeps dancing after the jukebox stops (`shouldStopDancing` false).
- `python3 tools/parity.py --changed-files <files> --list` and run what it names (lightning, falling block, allay harnesses): unchanged without mods.
- `cargo test` of the touched modules; clippy both ways; `java_mods.py build/check/unit`; one `bridges` run.
- Cost: a scenario that drops 200 sand blocks at once (`/fill` a column, remove its support); report `route:…FallingBlockEntity#tick` calls and mean.

## Done bar
Checks pass; four gates; `check --analysis` lists no unbridged callee or field for these bodies; lint passes.

## For the user
Sophisticated Backpacks or Storage with a jukebox upgrade playing near an allay: the allay dances, as on NeoForge. Lootr: a Lootr chest is not replaced or duplicated when a falling block lands next to it (the case Lootr's `setChanged` wrap exists for: compare with NeoForge if unsure). Architectury: nothing visible alone; covered by M1 (its lightning and landing events fire).
