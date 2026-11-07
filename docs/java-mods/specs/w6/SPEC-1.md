# SPEC-1 (W6): the Java server's command roots

Design: `DESIGN.md` §1.1, D1, D2. Depends on nothing. Read first: `CLAUDE.md`, `RULES.md`, `docs/java-mods-map.md` §1, §3, §6
("Add a new event firing", "Add a Java unit check"), `B/GameBridge.java` `prepare` and `register`, `G/java_mods.rs`
`java_world_phase`, `java_level_call`. `javap` only for NeoForge and mods. Comments in code: one line at most, only for a
non-obvious why.

## Goal

1. Java's `Commands` is built with the selection the dist would use.
2. At each server start Java tells Rust which root commands are Java's (JAVA) or contain Java nodes (MIXED), decided by the
   classes that own each node's code, never by names.
3. Rust keeps that table for the server thread (SPEC-2..4 read it).

## Java

### J1. Selection (`B/GameBridge.java` `prepare`)

Replace `Commands.CommandSelection.DEDICATED` in the `WorldLoader.InitConfig` with
`FMLEnvironment.getDist().isClient() ? Commands.CommandSelection.INTEGRATED : Commands.CommandSelection.DEDICATED`
(`net.neoforged.fml.loading.FMLEnvironment` is already used in `prepare`).

### J2. `B/ModCommands.java` (new, package-private, final)

```java
static final byte NATIVE = 0, JAVA = 1, MIXED = 2;
/** The class that declares this code: a lambda's hidden class up to `$$Lambda`, else its own class. */
static String owner(Object code)
static boolean vanilla(String owner)   // starts with "net.minecraft." or "com.mojang.brigadier."
/** One node's code is Java's when any of its command, requirement, redirect modifier, custom suggestions or argument type is. */
static boolean javaNode(CommandNode<?> node, Predicate<String> vanilla)
/** Root kind per D1: JAVA if the root's own node is Java's, MIXED if any node below it is (cycles through redirects visited once), else NATIVE. */
static byte kind(CommandNode<?> root, Predicate<String> vanilla)
/** Every root child of the server's dispatcher whose kind isn't NATIVE, in the dispatcher's order. */
static List<Map.Entry<String, Byte>> roots(CommandDispatcher<CommandSourceStack> dispatcher)
```
- `owner`: `code.getClass().getName()`; cut at the first `$$Lambda`; a `null` code has no owner (skip it). `RequiredArgumentBuilder`'s
  default requirement (`s -> true`) is brigadier's own lambda, so it counts as vanilla.
- A node's argument type counts by `type.getClass()` (a mod's `ArgumentType` is Java's even under a vanilla literal).
- Walk with an identity `Set` of visited nodes; follow `getRedirect()` targets only to mark visited, never classify a redirect
  target as part of the root that points at it (vanilla `execute ... run` redirects to the root).
- `roots` uses `dispatcher.getRoot().getChildren()` and `ModCommands.vanilla` as the predicate.

### J3. Op `commands_roots` (`GameBridge.register`)

`Crossing.game("commands_roots", (in, out) -> ...)`: `server.getCommands().getDispatcher()` (`server` is the `ServerView`
`MinecraftServer`), reply `int n`, n × (`text name`, `byte kind`). With `HORIZON_JAVA_DEBUG`, print one line
`Horizon: Java commands: <n> roots: <name>=<J|M> ...` (SPEC-10 and the lead read root names from it).

## Rust

### R1. `G/java_commands.rs` (new, `#[cfg(feature = "java-mods")]`, declared beside the other `java_*` modules in `game.rs`)

```rust
/// A root command Java runs, or whose tree has Java's nodes in it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum JavaRoot { Java, Mixed }
#[derive(Default)]
pub(super) struct JavaCommands { roots: HashMap<String, JavaRoot>, order: Vec<String> }
impl JavaCommands {
    pub(super) fn read(reply: &[u8]) -> Option<Self>;
    pub(super) fn root(&self, line: &str) -> Option<JavaRoot>; // first word of a command line, slash already stripped
    pub(super) fn names(&self) -> &[String];
}
thread_local! { static JAVA_COMMANDS: RefCell<JavaCommands> = Default::default(); }
pub(super) fn java_commands<T>(f: impl FnOnce(&JavaCommands) -> T) -> T;
impl Game<'_> { pub(super) fn java_read_commands(&mut self); }
```
- `java_read_commands` sends `commands_roots` through `java_request` (empty body; it isn't a level call) and stores the table;
  on `None` it logs once `Horizon: the Java server's command roots are unknown` and keeps an empty table.
- Call it from `java_world_phase(true)` right after `server_start`; `java_world_phase(false)` resets the table.
- The thread-local belongs to the server thread, as `JAVA_SERVER` does (map §8 "Thread-locals").

## Tests

- **Java unit `J/unit/dev/horizonlab/javamods/boot/ModCommandsCheck.java`** (package `boot` for package-private access; GPU-free, no
  boot). Build a `CommandDispatcher<Object>` by hand and classify with a predicate that calls `vanilla` true only for the check's
  nested class `Vanilla` (whose static lambdas stand for vanilla's) and brigadier:
  - root `a` with a `Vanilla` command → absent from the result;
  - root `b` executing a lambda declared in the check class → `JAVA`;
  - root `c` (Vanilla command) with a child literal whose command is the check's → `MIXED`;
  - root `d` (Vanilla) whose child redirects to root `b` → absent (a redirect target isn't part of `d`);
  - `owner` of a lambda declared in `Vanilla` ends with `$Vanilla`; of a plain class instance is its class name.
  Expected values are the hand-built tree's, not derived from `kind`. Print `ModCommands ok`.
- **Rust unit (in `java_commands.rs`):** `read` of a hand-written reply (`int 2`, `"mek"`/1, `"enchant"`/2) gives
  `root("mek give x") == Some(Java)`, `root("enchant @s") == Some(Mixed)`, `root("give") == None`; a short reply is `None`.
- **Test mod:** in `TestMod`, a `RegisterCommandsEvent` listener registers `horizon_java_test` (subcommands come in SPEC-2) and
  records `event.getCommandSelection()`. A server-side `Checks.check("commands_selection_follows_dist", ...)`, run where the
  family's checks run (`ChecksServer`), expects `INTEGRATED` on the client dist (`bridges` runs a client); its first line also
  checks `server.getCommands().getDispatcher().getRoot().getChild("horizon_java_test") != null`
  (`mod_command_registered`). Add both names to `java_mods.py bridges`' required list.

## Done bar (the implementer checks)

- `python3 tools/java_mods.py build && python3 tools/java_mods.py unit` prints `ModCommands ok`.
- `cargo test -p horizon-client --features java-mods --lib java_commands` passes; `cargo clippy -p horizon-client --features java-mods -- -D warnings`
  and `cargo clippy -p horizon-client -- -D warnings` (feature off) are clean.
- `java_mods.py bridges` (private `HORIZON_JAVA_MODS_DIR`, through the gate) prints both new CHECKs `ok`.

## Live test for the lead

Play pack, `HORIZON_JAVA_DEBUG=1`, open a world: the log has one `Horizon: Java commands:` line naming Mekanism's, AE2's,
Building Gadgets 2's, Jade's and NeoForge's roots (expected at least `neoforge` and `config`, each `J`). Showcase: a line too,
`enchant=J` only if puzzleslib's config replaces `/enchant`. No exceptions around `RegisterCommandsEvent`. No user steps.
