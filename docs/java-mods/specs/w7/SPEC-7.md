# SPEC-7 (W7): native meshing of mod fluids

Read first: `DESIGN.md` §1.1 G4, §2.3, D5; `crates/horizon-client-render/src/mesher/fluid.rs` (`tesselate`: the `water` test at ~235, the tint, `emission`, the sprite choice at ~329, `target` pass, the overlay at ~468), `mesher.rs` (`MeshTables` fields at ~609, their build at ~783 with `layer(...)`, `Info` at ~555 and its build at ~759 `overlay:`, the call at ~1503, `fluid_face` at ~1143, `hdr_tier`); vanilla `FluidRenderer.tesselate` (tint at line 87, side overlay at 292-295) and `FluidModel.Unbaked.bake` (layer from transparency); SPEC-4's `FluidTable` (`ModFluid.model`, `overlay(state)`).

Depends on: SPEC-4 (with SPEC-2/3 data present). Independent of SPEC-5. Comments in code: one line max, only a non-obvious why.

## Goal

Every mod fluid with a model draws in the world as vanilla's `FluidRenderer` draws it: its still, flowing and overlay sprites, its layer (solid or translucent), its tint, the side overlay next to blocks that ask for it. Water and lava draw exactly as today.

## `MeshTables`

- New `mod_fluids: Vec<Option<FluidSprites>>`, indexed by `kind.id() - 5`:
  ```rust
  struct FluidSprites { still: u16, flow: u16, overlay: Option<u16>, translucent: bool, tint: FluidTint }
  enum FluidTint { None, Water, Constant(Vec<u32>) } // Constant: one ARGB per fluid state, in the declaration's states order
  ```
- Built where `water_still` etc. are (`layer("...")`): sprites by name from `ModFluid.model` (`still`, `flowing`, `overlay`); `translucent` = `model.layer == "TRANSLUCENT"` (vanilla's `ChunkSectionLayer.byTransparency`: `TRANSLUCENT` draws in the translucent pass, `SOLID`/`CUTOUT` in the solid one with alpha test as vanilla's cutout: check how the solid pass treats cutout texels for other blocks and do the same); `tint` from `model.tint` (`code` → `None`, D5).
- A sprite name the block atlas lacks: `None` for that fluid, one log line `Horizon: fluid <id> has no sprite <name>; it isn't drawn`, never an error for the whole table (today's `missing fluid sprite` error stays for water and lava).

## `Info`

- `overlay: s.has_class("HalfTransparentBlock") || s.has_class("LeavesBlock") || table().overlay(id)` (vanilla's test for vanilla blocks; the exported per-state answer of `shouldDisplayFluidOverlay` for mod blocks that override it).

## `fluid::tesselate`

- Replace the `water` boolean by a selection:
  ```rust
  enum Look { Water, Lava, Mod(&FluidSprites, usize /* state index */) }
  ```
  `Water`/`Lava` keep every current line (tint, emission, sprites, pass). `Mod`: sprites from `FluidSprites`; pass `translucent`; tint `Water` → `water_color & 0xFF_FFFF` as water, `Constant` → the state's RGB (`& 0xFF_FFFF`), `None` → `0xFF_FFFF`; emission: the state's own `Info` HDR tier (`tables.hdr_tier(state)`), 0 when it has none.
- The state index comes from the fluid state (kind, amount, falling) through `ModFluid::state_index` (SPEC-4).
- `fluid_face` (the face flags in the alpha byte): unchanged. The terrain shader tells water from lava by the pass (`mesher.rs:1139`); a translucent mod fluid is therefore drawn in the pass water uses, with the same shader treatment. Note it in the report; changing shaders is not in scope.
- Tint alpha: vanilla keeps the tint's alpha in the vertex colour; native's vertex alpha carries face flags. Log once per fluid whose constant tint has alpha < 0xFF: `Horizon: fluid <id> has a translucent tint (alpha <a>); drawn opaque` and list them in the report (none expected; if any, a follow-up adds a tint alpha channel).
- `same_family` (SPEC-4) decides "same fluid" faces for mod fluids as for vanilla.

## Out of this spec

- LOD and BlueMap far terrain (`lod/mesh.rs`, `bluemap.rs`) keep drawing only vanilla fluids.
- Fluid particles, drips, the bucket item models (`neoforge:fluid_container` item models already go through `ClientModels`).

## Tests

- **`mesher/fluid.rs` tests** with a test `FluidTable` (one mod pair, `Constant` tint `0xFF80FF00` for all states, sprites `minecraft:block/water_still`/`water_flow`, layer `TRANSLUCENT`) and a tiny padded section: a mod source cell alone emits 6 faces into `out.translucent`; the top quad's colour RGB is `0x80FF00` scaled by vanilla's up-face shade (`cardinal(Up)` = 1.0 in the Overworld) → `0x80FF00`; the north face uses the flow sprite's layer and shade 0.8 (vanilla's north/south shade) → `(0x80*0.8, 0xFF*0.8, 0)`; compute the expected numbers in the test from vanilla's constants, not by calling the code.
- A mod fluid with `layer SOLID` lands in `out.solid`.
- A mod state with `table().overlay` true beside a mod fluid that has an `overlay` sprite: that side uses the overlay sprite.
- Existing water/lava mesher tests pass untouched.

## Done bar

- `cargo test -p horizon-client-render mesher`, clippy; `pr_ready.py --quick`.
- One live run (`play/install-rs`, gate): `/setblock` a brine source on a floor in front of the camera, screenshot once with `horizon-ctl screenshot`, look at it once: a tinted liquid with Mekanism's texture is visible. Then stop.

## Live test (lead)

1. Place brine, lithium, sulfuric acid, a Generators fluid with `/setblock` in a row on a stone floor: each draws with its own colour, animated texture, a flowing surface sloping away from the source.
2. Look through a brine pool at the floor: translucent where Mekanism's model is translucent (compare a Forge screenshot of the same placement).
3. Put glass next to flowing water and next to a mod fluid with an overlay sprite: the side shows the overlay texture, as on Forge.
4. Biomes O' Plenty pack: `/setblock` `biomesoplenty:blood` and `biomesoplenty:liquid_null`: their own textures.
