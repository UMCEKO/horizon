use super::*;

impl Game<'_> {
    /// an item fallback after a native mob's ordinary interaction passed.
    pub(in crate::game) fn native_item_interact_mob(
        &mut self,
        entity: i32,
        kind: &str,
        stack: &ItemStack,
        swing: &mut Option<bool>,
    ) {
        // Java's own swing ran with its claim.
        if self.java_hook_interact_living(entity) {
            *swing = Some(false);
            return;
        }
        self.fire_item_interact_mob(entity, kind, stack, swing);
    }
}
