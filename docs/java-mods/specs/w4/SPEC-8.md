# SPEC-8 (W4): mods' changes to a player's abilities and attribute modifiers reach native (server)

Design: `DESIGN.md` §1.3 (Mekanism flight row), §2.5, Q4. Size: ~380 changed lines (Java ~120, Rust ~260). Depends on: nothing.
Server side: runs on the single-player server thread and on the dedicated server alike.

Read first: `docs/java-mods.md` "The acting player's food…" row; `B/PlayerSync.java` (`acting`, `food`, `health`, the empty
bridged statics), `B/mixin/FoodSyncMixin.java`, `G/java_bridges/player.rs` (the `PlayerSync` rows at 182–230, `put_player`),
`L/lan/guest.rs` `ability_flags` (700), `G/lan_actions.rs` 2035–2060 (abilities sent to a guest), `G/lan_attributes.rs`,
`L/motion.rs` `set_modifier` (≈716) and the attribute save/load (`attribute_bases_survive_a_save_and_reload`). Vanilla:
`Player.onUpdateAbilities`, `ServerPlayer.onUpdateAbilities`, `GameType.updatePlayerAbilities`, `AttributeInstance`
(`addTransientModifier`, `addOrUpdateTransientModifier`, `addPermanentModifier`, `addOrReplacePermanentModifier`, `removeModifier`,
`setBaseValue`), `ServerGamePacketListenerImpl.handleMovePlayer`'s flight check.

Code comments: one line at most.

## Goal

When mod code changes the acting player's abilities (`getAbilities().mayfly = true; onUpdateAbilities()`, as a flight module does)
or adds, replaces or removes an attribute modifier on that player (or sets a base value), native's copy of the player changes the
same way, at that call, and native tells the player's client as vanilla does (`ClientboundPlayerAbilitiesPacket`,
`ClientboundUpdateAttributesPacket` through native's own sync). Only changes made by mod code: vanilla code running in the view
(`PlayerSync.settle`'s equipment update, `GameType.updatePlayerAbilities` on a mode change) stays native's.

Accept: Meka-Suit's Gravitational Modulating Unit lets the player fly on Horizon as on Forge.

## Java (~120 lines)
- `B/mixin/AbilitiesSyncMixin.java` (new; common mixins list `J/boot/res/horizon_java.mixins.json`): `@Inject(method =
  "onUpdateAbilities", at = @At("HEAD"))` on `ServerPlayer` → `PlayerSync.abilities(this)`.
- `B/mixin/AttributeSyncMixin.java` (new; common list) on `AttributeInstance`, RETURN of `addTransientModifier`,
  `addOrUpdateTransientModifier`, `addPermanentModifier`, `addOrReplacePermanentModifier` (each with the modifier argument),
  `removeModifier(Identifier)` (when it returned true) and `setBaseValue(D)` → `PlayerSync.modifier(this, kind, modifier, id, value)`.
- `B/PlayerSync.java`:
  - `static void abilities(Object entity)`: when `acting(entity)` and the caller is a mod's (a shared `B/Callers.java`
    `static boolean byMod(String... skipPrefixes)`: SPEC-2's `StackWalker` rule; whichever of SPEC-2 and SPEC-8 lands first creates
    it and the other uses it; here skip `net.minecraft.world.entity.` and `net.minecraft.server.level.`, so a mod calling
    `onUpdateAbilities` counts and vanilla's `GameType.updatePlayerAbilities` or Horizon's `GameBridge` doesn't), call the bridged
    `abilities(boolean invulnerable, boolean flying, boolean mayfly, boolean instabuild, boolean mayBuild, float flyingSpeed,
    float walkingSpeed)` (empty body, bridged).
  - The acting player's instances: at `acting(player)`, record `player.getAttributes()` (the map) in a field; in the mixin, an
    instance belongs to the acting player when the acting map's instance for its attribute is the same object
    (`map.getInstance(holder) == this`, a hash lookup). Only then, and only from a mod caller, call the bridged
    `modifier(String attribute, int kind, String id, double amount, int operation, boolean permanent)` (kind 0 add or replace,
    1 remove, 2 base value with `amount` the value).
  - Vanilla's own writes in the view (`settle`, mode changes) have vanilla callers and are not written through.

## Rust (~260 lines)
### Bridges (`G/java_bridges/player.rs`)
```rust
"dev/horizonlab/javamods/boot/PlayerSync".abilities "(ZZZZZFF)V" =>
fn player_abilities(game, invulnerable: bool, flying: bool, may_fly: bool, instabuild: bool, may_build: bool, fly: f32, walk: f32) -> () { … }
"dev/horizonlab/javamods/boot/PlayerSync".modifier "(Ljava/lang/String;ILjava/lang/String;DIZ)V" =>
fn player_modifier(game, attribute: String, kind: i32, id: String, amount: f64, operation: i32, permanent: bool) -> () { … }
```
- `player_abilities`: store `ModAbilities { invulnerable, flying, may_fly, instabuild, may_build, flying_speed, walking_speed }` on
  the acting player (`game.player`, a new `PlayerMotion` field `mod_abilities: Option<ModAbilities>`, swapped with the guest by
  `as_guest` like the rest of `PlayerMotion`); set `player.flying`, `player.may_fly`, `player.flying_speed`; queue the abilities
  packet for that guest (the same `C::PlayerAbilities` `lan_actions.rs` 2055 sends, with flags and speeds from the new state).
- `L/lan/guest.rs` `ability_flags` and every native read of a guest's flight permission (`grep -n "creative() || self.spectator()\|may_fly"
  crates/horizon-client/src/lan`): use `mod_abilities` when set. Native's mode change (`change_mode`, `player.rs:22`) clears
  `mod_abilities` first (vanilla's `updatePlayerAbilities` overwrites them; a mod re-applies on its next tick).
- `player_modifier`: `game.player.set_modifier(attribute, id, amount, operation_name(operation), kind == 0)` for kinds 0/1, and the base
  value for kind 2 (`AttributeMap`'s base setter beside `set_modifier`); remember permanent modifiers so they save with the player
  (native's existing permanent-modifier save; transient ones are not saved, as vanilla). Native's attribute sync then sends the change
  (`G/lan_attributes.rs` already diffs the synced snapshot per tick: verify that a modifier added this way changes the snapshot; if
  the snapshot is built only from equipment and effects, add the player's own map's extra modifiers to it).
- An unknown attribute id: log once `Horizon: a mod changed unknown attribute <id>` and ignore.

## Test mod (`TestMod.java` server listener, ~40 lines)
Server `PlayerTickEvent.Post` (guard `!isClientSide()`): while the player holds `horizon_java_test:wings` (new item) set
`getAbilities().mayfly = true; getAbilities().setFlyingSpeed(0.1f); onUpdateAbilities()` once; on dropping it, `mayfly = false;
flying = false; onUpdateAbilities()`. While holding `horizon_java_test:stilts`, `getAttribute(Attributes.STEP_HEIGHT)
.addOrUpdateTransientModifier(new AttributeModifier(Identifier.parse("horizon_java_test:stilts"), 0.5, ADD_VALUE))`, removed when not.
Checks (server side, `ChecksPlayer.later`): `wings_reach_native`: after holding the wings, native's `/data get entity @s abilities`
read through the command bridge shows `mayfly: 1b` and `flySpeed: 0.1f` (vanilla's `Abilities.addSaveData` names); `stilts_reach_native`:
`/attribute @s minecraft:step_height get` returns 1.1 (0.6 base + 0.5).

## Tests
- Rust: `player_abilities` on a test `Game` sets the fields and queues exactly one abilities packet whose flags equal vanilla's
  `ClientboundPlayerAbilitiesPacket` bits for (invulnerable, flying, mayfly, instabuild) = (false, false, true, false) → 4, typed in
  the test; a mode change clears `mod_abilities`; `player_modifier` add then remove leaves the attribute at its base.
- Java unit `CallersCheck`: the caller rule with a fake mod class (as `MotionLogCheck`).
- `cargo test -p horizon-client --features java-mods --lib java_bridges lan_attributes`, `cargo test -p horizon-client --lib
  motion::tests::attribute`, clippy with and without the feature, `java_mods.py build` (private dir), `unit`, `check`, `bridges`.
- Harness: `python3 tools/parity.py dedicated_server server_player_damage` (the guest paths) unchanged; LAN: `lan.sh` `--check
  flight` if it exists, else none.

## Done bar
Tests pass; `bridges` checks ok; `check` reports no new unbridged writer.

## Live test (lead)
1. Play install, survival: Meka-Suit with the Gravitational Modulating Unit enabled and charged: double-tap jump flies, as on Forge;
   disabling the module or running out of energy drops the player; no "flying is not enabled" kick.
2. Dedicated server with 2 clients: the flying player is seen flying by the other client; the flyer isn't rubber-banded.
3. Change game mode while flying with the unit: creative flight works, back to survival keeps flight only if the module re-enables it
   on its next tick (as on Forge).
