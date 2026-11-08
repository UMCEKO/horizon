<!-- chapter: Consumable effects -->
<!-- separator: 1 -->

A `LivingRef` on the server can apply one effect from an item's loaded consumable with
`living.apply_consume_effect(host, &stack, index)`. The index is zero-based in component
order; this does not eat food, shrink the stack or run consume triggers. `Some(true)` or
`Some(false)` is the effect's result; `None` means the item, index or target is unavailable
or the effect is unsupported. Mod-defined effects, mob random teleport, armor stands and
dragons are not supported. Player random teleport uses the level random as normal.
