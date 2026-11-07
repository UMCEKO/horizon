# SPEC-5-3: clicks on entities go through the Java client

Design: `DESIGN.md` §6 item 8 (clicks on entities, with native firing the interaction events with the view). Index: `SPEC-2-8.md`. Size: ~180 lines. Needs 5-1.

## Why
`java_click_canceled` (`G/java_screens.rs:835`) returns `None` when `self.entity_targeted()`: clicks on entities never reach the Java client, so mods' client handling of entity attacks and uses (`InputEvent.InteractionKeyMappingTriggered`, `PlayerInteractEvent.EntityInteract`/`EntityInteractSpecific` and `AttackEntityEvent` on the client, `Item.interactLivingEntity` client side) never runs, and their cancels don't stop native.

## Change
- Remove the `entity_targeted()` early return. When the crosshair is on an entity, the click request carries `int entityId` and the hit location (3 `double`), and `ClientBridge.click` sets `minecraft.hitResult = new EntityHitResult(view, location)` and `minecraft.crosshairPickEntity = view` (views from `ClientEntities`) before `startAttack`/`startUseItem`/`pickBlockOrEntity`, exactly as it sets a `BlockHitResult` for blocks today.
- An entity the Java client doesn't have (not yet fed: a click in the same frame as its `AddEntity`) is a race the feed order prevents for anything older than one client tick; for a younger one the click goes through with no Java hit, logged once.
- What Java sends to its server as a result is dropped as for block clicks (`ClientBridge.sentByWindow`: native already sends `Interact`/`Attack`); a cancel answers `Some(..)` and the window doesn't act.
- `pickBlockOrEntity` on an entity gives the mod's pick item (`IEntityExtension.getPickedResult` overrides) to native's pick-block path.

## Test mod (client)
A client listener cancels `InputEvent.InteractionKeyMappingTriggered` for the use key when the target entity is tagged `horizon_untouchable`. Check `entity_use_canceled_client_side` needs a real click: a user step.

## User steps
1. Right-click a villager tagged `horizon_untouchable` (`/tag @e[type=villager,limit=1,sort=nearest] add horizon_untouchable`): no trade screen opens.
2. Untagged villager: the trade screen opens as before.
3. Jade (pack): look at a cow; Jade's tooltip shows the cow's name and health.
