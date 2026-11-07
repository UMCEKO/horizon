# SPEC-3 (W1 outline): vanilla's outline lines from Java's shape for mod blocks with a block entity

Read first: DESIGN.md §1.2 (last bullet) and D5; `G/java_shapes.rs` (all); `B/ClientBridge.java` `shape` (~1150);
`game.rs` `outline_boxes` (~3619) and the outline edges (~16602–16640); vanilla
`client/renderer/feature/ShapeOutlineFeatureRenderer.java` (the lines are `VoxelShape.forAllEdges`). Paths as SPEC-1.

Depends on: nothing (lands before or after SPEC-1/2). Size: about 180 changed lines. Comments in code: one line at most.

## Gap

For a mod's block with a block entity, native picks with Java's `getShape(level, pos, empty)` (`java_outline` →
`client_shape`), but draws the boot facts' static `outline_edges` (`game.rs` ~16636). A Refined Storage cable or a Mekanism
transmitter is picked by its connected shape and outlined by its bare core (or a full cube). Vanilla draws
`state.getShape(level, pos, context).forAllEdges(...)` (`ShapeOutlineFeatureRenderer`).

## Java (`B/ClientBridge.java` `shape`)

After the boxes, write the shape's edges:
```java
var edges = new ArrayList<double[]>();
shape.forAllEdges((x1, y1, z1, x2, y2, z2) -> edges.add(new double[] {x1, y1, z1, x2, y2, z2}));
out.writeInt(edges.size());
for (var e : edges) for (double v : e) out.writeDouble(v);
```
`shape` is the one `toAabbs` is taken from (keep one `getShape` call). Unchanged otherwise (`CollisionContext.empty()`, as
today; the per-entity context is a known limit in `docs/java-mods.md`).

## Rust (`G/java_shapes.rs`, `game.rs`)

- `Cached` becomes `(BlockStateId, i32, Vec<[f64; 6]>, Vec<[f64; 6]>)`: boxes, then edges.
- Split today's `java_outline` body into `fn java_shape(&self, pos, state) -> Option<(Vec<[f64; 6]>, Vec<[f64; 6]>)>` (the
  cache, the busy rule, the request, the parse of both lists; a reply without the edge list parses as no edges), and keep
  `java_outline` returning its boxes; add
  ```rust
  /// The block-local edges vanilla outlines this block with, from Java's shape; None when Horizon's own facts apply.
  pub(super) fn java_outline_edges(&self, pos: [i32; 3], state: BlockStateId) -> Option<Vec<[f64; 6]>>
  ```
  returning `None` also when the edge list is empty (an old reply, or an empty shape: native's facts then decide, as today).
- `game.rs` outline mapping (~16624): after the `context_outline` branch and before `f.outline_edges`,
  ```rust
  #[cfg(feature = "java-mods")]
  if let Some(edges) = self.java_outline_edges(h.pos, state) {
      return edges.iter().map(|e| std::array::from_fn(|i| p[i % 3] + e[i])).collect();
  }
  ```
  No `outline_offset` is added: Java's `getShape` already includes the block's offset, as `outline_boxes` treats Java's boxes.
- `outline_after_terrain` is unchanged.

## Tests

- Java `U/ShapeEdgesCheck.java` (new): the reply bytes of a helper `ClientBridge.shapeReply(VoxelShape)` (extract the
  writing part of `shape` into it): for `Shapes.box(0, 0, 0, 1, 0.5, 1)` the edge list is the 12 edges of that box (expected
  list written out: 4 at y = 0, 4 at y = 0.5, 4 verticals), in any order (compare as sets); for two disjoint boxes
  `box(0,0,0,0.25,0.25,0.25)` and `box(0.75,0.75,0.75,1,1,1)`, 24 edges.
- Rust `java_shapes.rs`: a reply with 1 box and 12 edges parses to both lists; a reply without the edge list parses to
  boxes and no edges, and `java_outline_edges` is `None` for it.

## Done bar

- `java_mods.py build`, `java_mods.py unit`; `cargo test -p horizon-client --features java-mods --lib java_shapes`;
  clippy with and without the feature; fmt.

## Live test (lead; the look is the user's)

Play pack, creative. Place a Refined Storage cable (`refinedstorage:cable`) between two RS blocks (e.g. a
`refinedstorage:controller` and a `refinedstorage:disk_drive`), and a Mekanism `mekanism:basic_universal_cable` between two
machines. Aim at each cable: the outline follows the connected arms, as it is picked (aiming at an arm selects the
cable). A cable alone in the air: only its core is outlined. Vanilla blocks and mod blocks without a block entity: as before.
Note: with SPEC-2 landed, Mekanism may replace the transmitter's outline with its own; then step 2 of SPEC-2 applies.
