//! `ItemStack.getTooltipLines` for any slot's stack.
use super::*;

impl Game<'_> {
    /// `ItemStack.getTooltipLines`. A `nested` stack is a crossbow's
    /// projectile: `ChargedProjectiles.addProjectileTooltip` takes its details
    /// without the name, the player and the advanced flag. `creative` is
    /// `TooltipFlag.isCreative`: a creative tab's own slot.
    pub(super) fn stack_tooltip_lines(
        &self,
        stack: &ItemStack,
        nested: bool,
        creative: bool,
    ) -> Vec<(horizon_client::text::Text, u32)> {
        if !nested && let Some(text) = self.java_hook_item_text(stack, creative) {
            return text.lines.iter().map(|l| (l.clone(), u32::MAX)).collect();
        }
        self.build_stack_tooltip_lines(
            stack,
            nested,
            creative,
            self.options_advanced_tooltips(),
            true,
        )
    }
}
