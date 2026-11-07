# SPEC-8-4: mod layers on vanilla entities, and armour models

Design: `DESIGN.md` §6 items 3–4 (layers added through `AddLayers`, diffed at boot; `IClientItemExtensions` armour model overrides), §7 ("Mod layers on vanilla entities: native supplies the parent model's poses to the model view"). Index: `SPEC-2-8.md`. Size: ~300 lines (split 8-4a layers, 8-4b armour, if over). Needs 8-2, 8-3.

## Layers (8-4a)
- **Boot:** after `EntityRenderersEvent.AddLayers`, diff each vanilla `LivingEntityRenderer`'s (and the player skins') layer list against vanilla's (the layers vanilla's constructor adds: snapshot before the event). An added layer is a mod layer: export `text entity type or skin`, `text layerClass`, `int index`.
- **Per tick:** for each visible entity whose renderer has mod layers, native sends the **parent model's part poses** it computed for that entity (native draws vanilla models: it has them) in the 5-1 feed (`POSE` entry: part names as vanilla's model defines them, transforms); Java writes them into the Java renderer's model parts, then runs the mod layer's `submit(poseStack, collector, light, state, yRot, xRot)` with `SubmitCapture`. The recording follows 8-3's rule.
- Native draws the layer's pieces after the vanilla entity, in the entity's transform.

## Armour models (8-4b)
- **Boot:** items whose `IClientItemExtensions` (registered through `RegisterClientExtensionsEvent`) override `getHumanoidArmorModel`/`getGenericArmorModel` or `getArmorTexture`: export the item ids and which.
- **Per tick:** for each visible humanoid (players included) wearing such an item, Java runs vanilla's `HumanoidArmorLayer` path for that slot only (the layer is a vanilla class reaching the mod's extension: the mod's model is the mod code), with native's parent humanoid pose supplied as above, capturing the submits. Slots with ordinary armour stay native.
- Mods replacing `HumanoidArmorLayer.submit` itself (Mekanism's `MekanismArmorLayer`, DESIGN §1.4) are a mod layer (8-4a) and an override; when the mod's layer draws every slot, native skips its own armour for that entity (the diff tells native the vanilla armour layer was replaced, not added).

## Tests
- Test mod: a `horizon_java_test:halo` layer added to pigs (a ring above the head, following the head pose); a test helmet with `getHumanoidArmorModel` returning a cube model. Client checks: the recordings exist for a pig and for a zombie wearing the helmet.

## User steps
1. Mekanism: wear the Meka-Suit. In third person (F5), the suit's model shows on you; a friend (LAN) sees it on you.
2. A mod that adds a layer to vanilla mobs (any in the pack): it draws on the mob and moves with the head.
