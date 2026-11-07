# SPEC-1c2c-1: attachment points from the exporter; mob riders placed from them

Index and decisions: `SPEC-1c2c.md` (D5). Size: ~250 lines. Depends on nothing.

## Why
`mobs/riders.rs` `base_attachment` and `vehicle_attachment` are hand tables: 12 vehicle rows and 15 rider rows, with everything else falling back to "top of the box". Vanilla reads `EntityType`'s `EntityAttachments` (`Entity.java:2456–2470`). Every kind that rides later (steps 9–11) needs the real values, so they come from the game's own data.

## 1. Exporter (`mods/horizon/src/test/java/dev/horizonlab/parity/VanillaEntityTypesExporter.java`)
Per type, beside `width`/`height`, add:
- `"attachments"`: `{"passenger": [[x,y,z],…], "vehicle": [[x,y,z],…]}` from `type.getDimensions().attachments()`. Read each list with `attachments.getNullable(a, i, 0f)` for `i = 0, 1, …` until null. At `yRot` 0, `get` doesn't rotate. `EntityAttachments.Builder.build` always fills a fallback, so index 0 always exists.
- `"dimensions"`: every `static final EntityDimensions` field of the entity class and its superclasses, keyed by field name (`"BABY_DIMENSIONS"`, `"SITTING_DIMENSIONS"`…), each as `{width, height, eye_height, attachments}`.
  - The class comes from `EntityType`'s own static field for the type: `Field.getGenericType()` is `EntityType<Pig>`, so take its type argument. No instance and no level are needed.
- For `minecraft:player` and `minecraft:mannequin`: `"poses"`, `Avatar.POSES` (a static map) as `{pose name: {width, height, eye_height, attachments}}`.

**The bump:**
- `crates/horizon-prepare/src/lib.rs` `OUTPUT_VERSION` 3 → 4.
- The exporter's sha (`entity_types_exporters_sha256`) already makes prepare re-export.
- Add the new digest line to `crates/horizon-prepare/output-versions.txt` with `python3 tools/prepare_output.py digest <resources>`, then `… check --base origin/main`.

## 2. Reading them (`mobs.rs`, where `entity-types.json` is read, line ~413)
`MobType` gains `pub attachments: Attachments`:
```rust
pub struct Attachments { pub passenger: Vec<[f64; 3]>, pub vehicle: [f64; 3], pub baby: Option<Box<Attachments>>, pub poses: Vec<(Pose, Attachments)> }
```
- `baby` comes from `BABY_DIMENSIONS` when the class has the field.
- `vehicle` keeps only index 0: vanilla `getVehicleAttachmentPoint` reads `get(VEHICLE, 0, yRot)`.
- A missing `attachments` key (an older install) makes the load fail with "re-prepare (OUTPUT_VERSION 4)". There is no fallback.

**The dimensions a living entity uses** are `LivingEntity.getDimensions(pose)` (line 3752): `getDefaultDimensions(pose).scale(getScale())`.
- `getDefaultDimensions` is the type's dimensions `.scale(getAgeScale())`, or the class's override.
- `BABY_DIMENSIONS` for babies is the common override. Other overrides (sitting camel, digging sniffer) apply where native already switches that kind's box: grep `getDefaultDimensions` in vanilla for the list, and use the exported field of the same name.
- `EntityDimensions.scale` scales the attachments too. New `fn attachments(m: &Mob) -> Attachments` returns the scaled set the mob's box uses now.

## 3. Positions (`riders.rs`)
**Vehicle side.** `passenger_attachment(v, index)` (new `index` parameter) returns:
- **the kind's override**, for kinds that override `getPassengerAttachmentPoint`: `AbstractHorse` (+ `Llama`), `Camel`, `Strider`, `AbstractCubeMob`, `AbstractMinecart`, `AbstractBoat`. Mobs keep native's ports (`equine::rear_offset`, `camel::passenger_attachment`), now added to the exported base point in place of the table row. Strider and cube mobs: port their bodies if native has only a constant.
- **otherwise** `getClamped(PASSENGER, index, yRot)`: the exported list, index clamped to its last entry, then `rotated`.

**Rider side.** `vehicle_attachment(r)` becomes the exported `vehicle[1]` of the rider's current attachments, with x and z too: `Spider` and `CaveSpider` override `getVehicleAttachmentPoint`, so port those two.

**Callers.** `position_riders` (`game/passengers.rs:75`) passes the rider's index in its vehicle's passenger list. Mob vehicles hold one mob rider today, so that is 0. Boats keep `seat_boat_mobs` until step 5.

Delete `base_attachment` and the `vehicle_attachment` match.

## Tests
- **Equivalence test, `mobs::riders::tests`** (skips without `.local`, like `crossing_tests.rs`): for every row of the old tables (copy them into the test), the exported value equals the table's. A row that differs is checked against `EntityType.java`'s builder (`passengerAttachments`, `vehicleAttachment`, `ridingOffset`) and the class's `BABY_DIMENSIONS`. Name the vanilla value in your report; don't adjust the test to pass.
- `attachments_rotate_and_clamp`: a two-point list at yaw 90, with indices 0, 1 and 5.
- `python3 tools/parity.py mob_ai`.
- `python3 tools/lan_e2e.py --check camel`, `--check horse`, `--check strider`.

## Parent check
Live: `/summon zombie ~ ~ ~ {IsBaby:1b}` on a chicken (`/summon chicken ~ ~ ~ {Passengers:[{id:zombie,IsBaby:1b}]}`) and a skeleton on a spider. The rider sits as in vanilla (user, side by side with Java).
