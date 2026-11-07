# SPEC-1: one owner per route, the 74 verdicts, the `java-client` proof, and a lint that counts M2

Design: `DESIGN.md` §2.1, decisions C3 and C4. Needs nothing. Size: ~330 changed lines (Python, TSV rows, one scenario, a small `Routing.java` change in §5b). No Rust changes.

Read first: `tools/java_routing.py` (`REASON`, `lint`, `report_table`), `tools/mod_replaces.py` (`route`, `verdict_of`, `read_verdicts`), `tools/java_routing/deferred.tsv`, `tools/java_routing/packs/showcase.tsv`, `tests/test_java_routing.py`, and this folder's `owners.py`/`owners.tsv`.

Comments in code: one line at most, only for a non-obvious why.

## 1. Verdicts for the 74 undecided methods (`tools/port_map/*.tsv`)
Append one row per method to the `port_map` file that already holds its class (create `w12-verdicts.tsv` for classes that have none). Columns as the existing rows: `Class.method`, verdict, native fn, note.

For each `owners.tsv` row with source `undecided`:
- Owner `SPEC-n`, `W1`, `W5`, `W10`, `W13`: native runs it. Verdict `inline` (or `port` when one Rust fn is the whole port). The native fn is the one `python3 tools/port_ledger.py where Class.method` names (`class_home` or `mentioned_in`); when it names several, take the `Game` method the server path calls. If `where` names nothing, grep the method's vanilla name in snake case under `crates/`; still nothing: verdict `missing` and add the key to §6's report instead of guessing.
- Owner `java-client`, `java-boot`, `java-already`: verdict `skip:java` with the note `w12-coverage: <owner>`.
- Owner `inert` (gametest, `TagsProvider#run`): `skip:debug`, note `w12-coverage: gametest or datagen`.
- Owner `out-of-scope`: `skip:other`, note `w12-coverage: upgrading pre-26.3 worlds`.

Then rerun the analysis on both installs (no game needed): `python3 tools/java_mods.py replaces --install /home/umceko/hz-poc/agents/w12/install` and the same for `/home/umceko/hz-poc/play/install-rs`. Expected: `routing: … undecided 0` for both. New `java` routes appear for the native-run ones.

## 2. Verdict file (`tools/java_routing/verdicts.tsv`, new)
`key \t verdict \t evidence`, one row per route key whose owner is a verdict. `verdict` is one of `java-client`, `java-boot`, `java-already`, `java-view` (Java's view of the receiver is made by the very method, at vanilla's moment: SPEC-7, SPEC-19), `java-served` (every native call site already delegates the whole method to Java's real one: SPEC-13), `inert`, `out-of-scope`. `evidence` is free text, required, e.g. `trace: java_client_routes.jfr` or `gametest only`.

## 3. Deferral reasons (`tools/java_routing.py`)
Replace `REASON` with:
```python
REASON = re.compile(r'^(owner: (W1|W2|W4|W5|W6|W7|W8|W10|W13|W9 \d+-\d+)|spec: w12-coverage SPEC-\d+|seam: \S+:\d+|special: (animate|item_code|sounds|screens))$')
```
`seam: file:line` stays only as the partial-gate marker (`PARTIAL`). Rewrite every row of `deferred.tsv` from `owners.tsv`:
- owner `SPEC-n` → `spec: w12-coverage SPEC-n`; owner `SPEC-9 + W9 3-2` → `owner: W9 3-2` (until 3-2 lands) and `spec: w12-coverage SPEC-9` (two rows);
- owner `Wn` → `owner: Wn`; owner `W9 3-2` → `owner: W9 3-2`;
- owners that are verdicts go to `verdicts.tsv`, not here; `gated` keys have no row.
- Keep `special: screens` only for keys W5 already serves through `gui_route!` (check `route_gates.tsv`); `KeyMapping#setAll` moves to `verdicts.tsv` as `java-client`.

Write a one-shot converter as `tools/java_routing.py owners --from OWNERS.tsv` that prints the new `deferred.tsv` and `verdicts.tsv` bodies (two files with `--write`), so the next pack can be filed the same way. It reads only the five columns of `owners.tsv`.

## 4. Reference packs
- Regenerate `tools/java_routing/packs/showcase.tsv` from the §1 analysis (the existing format; `java_routing.py` has the writer).
- Add `tools/java_routing/packs/play.tsv` from `install-rs`'s analysis. `lint` reads every `packs/*.tsv` (check it globs; if it reads only `showcase.tsv`, make it glob).

## 5. Lint and report
In `lint(...)` add, after the deferral checks:
- a key in `verdicts.tsv` that is also gated or deferred: `<key>: has a verdict and a gate or deferral`;
- a `verdicts.tsv` row with an empty evidence cell or an unknown verdict;
- a pack key with neither gate, deferral nor verdict: the existing `unrouted` error now also names `verdicts.tsv`.

`report_table` gains a last line per pack, the M2 count:
```
M2 <pack>: <n> routes; covered <g gated + v verdict>; owned <o> (W5 a, W10 b, …, w12-coverage c); open <u>
```
`open` counts UNGATED keys; the lint fails if it is not 0.

### 5b. The boot report knows deferrals and verdicts (`B/Routing.java`)
Today `Routing.line` prints only `gated` or `UNGATED` (it reads `route_gates.tsv`), so a deferred key boots as `UNGATED` (`play/game31.log` L177–L196).
- `java_routing.py gates` also writes `J/boot/res/route_status.tsv` (`key \t status`), where status is the deferral reason or `verdict: <verdict>`, from `deferred.tsv` and `verdicts.tsv`; `gates --check` covers it.
- `Routing.confirm` reads it beside `route_gates.tsv`. `line(...)` prints, for a key with no gate, `deferred (<reason>)` or `<verdict>` when the file has it, else `UNGATED`.
- Only `UNGATED` keys enter `summary(...)`'s loud line. Owned-elsewhere keys are listed once more in a quiet line: `Horizon: <n> routes wait for their owner: W5 a, W10 b, …`.
- Java unit check `RoutingReportCheck` (exists): add `line` cases for a deferred and a verdict key, with the expected strings written out.

So the size is ~330 lines with this Java part.

## 6. The `java-client` proof (`tools/scenarios/java_client_routes.json`, new; `java_routing.py trace-filter`, `trace-check`)
- `java_routing.py trace-filter` prints a JFR `jdk.MethodTrace` filter (`Class::method;…`, the format `RULES.md` shows) for every `java-client` key of `verdicts.tsv` and `owners.tsv`.
- `java_routing.py trace-check FILE.jfr` runs `jfr print --events jdk.MethodTrace FILE` and, for each `java-client` key, requires at least one event of that method on a thread named `Render thread`. It prints one line per key, `ok` or `MISSING`, and exits non-zero on any `MISSING`. `java-boot` keys require one event on any thread before the first `ServerStarting`-era event; if that ordering can't be read from JFR, require only presence.
- The scenario, on the showcase install, must reach every situation vanilla calls those methods in:
  1. `{"wait": 100}` (login, configuration finished, client ticks, `ClientLevel.tickEntities`, the language reload already ran at boot);
  2. `{"key": "key.inventory"}`, `{"wait": 10}`, `{"close": true}` (`KeyboardHandler.keyPress`, `KeyMapping.click`/`set`/`isDown`/`consumeClick`, `Gui.setScreen` twice);
  3. `{"command": "/setblock ~2 ~ ~ minecraft:stone"}`, `{"lookat": [the block]}`, `{"attack": 2}` (`startAttack`), `{"use": 2}` (`startUseItem`);
  4. a middle click: add a `{"pick": true}` step to `hz_scenario.py` if none exists (horizon-ctl's pick command; `pickBlockOrEntity`);
  5. `{"command": "/recipe give @s minecraft:stick"}`, `{"wait": 10}`, `{"command": "/recipe take @s minecraft:stick"}` (`handleRecipeBookAdd`/`Remove`);
  6. `{"check": {"no_exceptions": true}}`.
- `MouseHandler.onButton` is reached by step 3's clicks only if hz_scenario sends them as mouse buttons; if it uses horizon-ctl's `attack`, record that and let the trace decide.
- `Minecraft.<init>`: the client is built by `GameBridge.construct` without its constructor (`ClientLoader.java:174`). Expect `MISSING`; the trace decides, not this spec.

Every key `trace-check` reports `MISSING` moves in `owners.py` from `java-client` to `SPEC-22`, and its row from `verdicts.tsv` to `deferred.tsv` (`spec: w12-coverage SPEC-22`). That is the expected result for `Minecraft#<init>`, and possibly for the recipe book and `syncBlockState`; it is not a failure of this spec.

## 7. Tests (`tests/test_java_routing.py`)
Each with hand-written inputs; expected values written out, not computed by the code under test:
- `test_owner_reasons_accepted`: `owner: W5`, `owner: W9 3-2`, `spec: w12-coverage SPEC-12` match; `client: phase C`, `owner: W99`, `spec: SPEC-3` don't.
- `test_verdict_and_gate_conflict`: a key gated and in `verdicts.tsv` gives exactly one error naming it.
- `test_verdict_needs_evidence`: an empty evidence cell is an error.
- `test_m2_line`: a pack of 5 keys (2 gated, 1 verdict, 1 `owner: W5`, 1 ungated) prints `M2 t: 5 routes; covered 3 (2 gated + 1 verdict); owned 1 (W5 1); open 1` and lint fails.
- `test_owners_converter`: a 4-row `owners.tsv` (one each of `SPEC-6`, `W10`, `java-client`, `gated`) gives a 2-row `deferred.tsv` and a 1-row `verdicts.tsv`, byte for byte as written in the test.
- `test_trace_check_parses`: a 6-line `jfr print` excerpt pasted in the test (two methods, one on `Render thread`, one on `Server thread`) gives `ok` for the first and `MISSING` for the second.

Run: `python3 -m unittest tests.test_java_routing tests.test_mod_routing`, `python3 tools/java_routing.py lint`, `python3 tools/java_routing.py gates --check`.

## Done bar (the implementer checks)
- Both installs' `java_mods.py replaces` print `undecided 0`.
- `java_routing.py lint` passes; `report --routing finish/rep-showcase/routing.json` (regenerated) ends with `open 0` for the showcase and play packs.
- Every row of `owners.tsv` appears in exactly one of: `route_gates.tsv`, `deferred.tsv`, `verdicts.tsv`.
- Unit tests above pass.

## For the lead
- Run the scenario once with JFR: `OPTS="-XX:StartFlightRecording=filename=/home/umceko/hz-poc/agents/w12-coverage/jc.jfr,jdk.MethodTrace#enabled=true,jdk.MethodTrace#filter=$(python3 tools/java_routing.py trace-filter)"` through `hz_scenario.py run --install /home/umceko/hz-poc/agents/w12/install --script tools/scenarios/java_client_routes.json --out …` (gate first, `HORIZON_GAME_GATE` set). Then `python3 tools/java_routing.py trace-check jc.jfr`. Expected: `ok` for the `KeyMapping`, `keyPress`, `startAttack`, `startUseItem`, `tick`, `setScreen`, `tickEntities` and configuration keys; whatever is `MISSING` is filed to SPEC-22 as §6 says. Delete the JFR file after.
- A boot of the play pack prints `Horizon: route … gated` or a deferral for each of its 20 routes, and no `UNGATED` (needs the in-flight `sendChestLockedNotifications` change).

No user steps.
