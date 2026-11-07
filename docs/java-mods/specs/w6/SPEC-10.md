# SPEC-10 (W6): event parity (M1) for chat and commands, the acceptance scenario, the map

Design: `DESIGN.md` D10 and §4. Depends on SPEC-1..9 (run it after each lands to see that spec's rows go green; it is done
when all are). Read first: `T/mod_parity.py` (`run`, `compare`, how a scenario's steps reach the real client and Horizon),
`J/parity/mod/dev/horizonlab/javamods/parity/Scenario.java` (`begin`), `T/parity_scenarios/basic.json`, `T/hz_scenario.py` and two
scenarios under `T/scenarios/` (`smoke.json`, `inventory.json`), `docs/java-mods.md` "Not yet", `docs/java-mods-map.md` §3–§5.
Comments in code: one line at most.

## 1. M1 scenario

- `Scenario.java`: a `chat` step, `player.connection.sendChat(step.arg.getAsString())` (beside `command`). The Horizon side of
  `mod_parity.py run` maps `chat` to `horizon-ctl chat TEXT` (find where it maps `command`).
- `T/parity_scenarios/chat.json`: `wait 40`; `command time query daytime`; `wait 5`; `chat hello parity`; `wait 5`;
  `command <J root>`; `wait 5`; `command <client root>`; `wait 20`. Take `<J root>` and `<client root>` from the play pack: a
  Mekanism server command that needs no target and changes no world state (`HORIZON_JAVA_DEBUG`'s `Horizon: Java commands:` line
  lists the roots; pick one whose help/usage answer is static), and an AE2 or Jade client command (SPEC-5's names). Write the two
  choices and why in the scenario's `description`.
- Run `python3 tools/mod_parity.py run --install /home/umceko/hz-poc/play/install-rs --scenario chat --out <your folder>/m1`
  (the tool takes the gate). Then `mod_parity.py compare` (it runs as part of `run`).

Done bar (from the comparison's per-step event counts, Forge against Horizon, same step):

| Event class | Expected on both |
|---|---|
| `RegisterCommandsEvent` | 1 per server start |
| `RegisterClientCommandsEvent` | equal counts (login, plus each command tree the client got) |
| `CommandEvent` | 1 at each `command` step that reaches the server (the client command's step: 0) |
| `ServerChatEvent` | 1 at the chat step |
| `ClientChatEvent` | 1 at the chat step |
| `ClientChatReceivedEvent$Player` | 1 at the chat step |
| `ClientChatReceivedEvent$System` | equal counts at each command step (the commands' feedback lines) |
| `ToastAddEvent` | equal (0 in this scenario unless an advancement fires on both) |

A difference is a bug in the spec that owns the event (SPEC-2 command, SPEC-3 trees, SPEC-5 client send, SPEC-6 receive, SPEC-7
server chat); report it with the step and both counts.

## 2. Acceptance scenario (hz_scenario)

`T/scenarios/w6_chat.json` on the showcase install (`agents/w12/install`), `HORIZON_JAVA_DEBUG=1`, creative, flat area as
`smoke.json` builds it:
1. `{"command": "time query daytime"}`, `{"wait": 5}`, `{"check": {"chat": "The time is"}}` (native still runs vanilla).
2. `{"command": "neoforge mods"}`, `{"wait": 5}`, `{"check": {"chat": "<a mod id the showcase has, e.g. jei>"}}` (a Java root).
3. `{"chat": "hello w6"}`, `{"wait": 5}`, `{"check": {"chat": "hello w6"}}` (ServerChatEvent and ClientChatReceivedEvent passed it).
4. `{"chat": "/<a client command root of the showcase>"}`, `{"wait": 5}`, `{"check": {"no_chat": "Unknown or incomplete command"}}`.
5. `{"check": {"no_exceptions": true}}`, `{"check": {"log": "Horizon: Java commands: \\d+ roots"}}`.
Use the check kinds `hz_scenario.py` has (read its checker; if `chat`/`no_chat` don't exist, use `messages` output with `log`
checks, or add the two checks to `hz_scenario.py` with a unit test in `tests/`).

## 3. Docs

- `docs/java-mods.md` "What exists": one row "Commands and chat" (SPEC-1..8 in its words: Java's roots run in Java, the tree
  players get, client commands, the four events) and one "Mods' toasts" (SPEC-9). Remove from "Not yet" what these cover; add the
  out-of-scope items of DESIGN §5 there. Upstream prose must not name NeoForge (the user's rule for upstream text): write "the
  mod loader" where the existing doc does.
- `docs/java-mods-map.md`: §3 rows for `ModCommands`, `ChatSend`, `ChatLines`, `ChatReceived`, `ChatBox`, `Toasts`; §4 the
  client pieces; §5 wire rows for every op and status field added by SPEC-1..9; §2 nothing (no boot fact). One line each.

## Done bar

The M1 table above holds on the play pack; the acceptance scenario passes on the showcase install; `docs` updated in the same
change; `python3 -m unittest discover -s tests` for any test added to `tests/`.

## Live test for the lead

Run `w6_chat.json` once through `hz_scenario.py` and read its JSON; then the user round below.

User steps (play pack, then the dedicated server with two clients):
1. `/` in chat: mods' roots listed and white. `/mek ` and `/ae2 ` complete their subcommands; run one each.
2. Run an AE2 client command and a Jade client command: their output in chat.
3. Chat a line from each client: shown once on both.
4. Finish an AE2 or RS autocrafting job: its toast slides in and out at the top right.
Screenshots of 1 and 4.
