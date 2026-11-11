//! The player's own inventory screen (`InventoryScreen`).
use super::*;
use super::java_hooks_client::Leaf;

impl Game<'_> {
    pub(super) fn draw_inventory(&mut self, batch: &mut HudBatch) {
        screen_info::begin_container("container");
        if self.draw_station(batch) {
            return;
        }
        let (ox, oy) = self.inventory_origin(batch);
        self.java_shell_frame();
        self.shell_leaf(batch, Leaf::Start, |_, _| {});
        self.shell_leaf(batch, Leaf::Background, |this, batch| {
            let atlas = &this.gui.atlas;
            this.render_transparent_background("inventory", batch);
            let part = |batch: &mut HudBatch,
                        name: &str,
                        sx: f32,
                        sy: f32,
                        w: f32,
                        h: f32,
                        dx: f32,
                        dy: f32| {
                if let Some(mut r) = atlas.sprites.get(name).copied() {
                    let (du, dv) = ((r.u1 - r.u0) / r.w as f32, (r.v1 - r.v0) / r.h as f32);
                    r.u0 += sx * du;
                    r.v0 += sy * dv;
                    r.u1 = r.u0 + w * du;
                    r.v1 = r.v0 + h * dv;
                    batch.rect_uv(dx, dy, w, h, r, u32::MAX);
                }
            };
            let grey = rgba(64, 64, 64, 255);
            match this
                .container
                .and_then(|k| this.world.block_entities.get(&k))
            {
                Some(_) if this.draw_native_screen(batch, ox, oy) => {}
                Some(BlockEntity::Chest(_)) if this.draw_machine_screen(batch, ox, oy) => {}
                Some(BlockEntity::Chest(_)) => this.draw_chest_screen(batch, ox, oy),
                Some(BlockEntity::Furnace(f)) => {
                    let (tex, key, fallback) = match f.kind {
                        horizon_client::containers::FurnaceKind::Furnace => {
                            ("gui/container/furnace", "container.furnace", "Furnace")
                        }
                        horizon_client::containers::FurnaceKind::BlastFurnace => (
                            "gui/container/blast_furnace",
                            "container.blast_furnace",
                            "Blast Furnace",
                        ),
                        horizon_client::containers::FurnaceKind::Smoker => {
                            ("gui/container/smoker", "container.smoker", "Smoker")
                        }
                    };
                    part(batch, tex, 0.0, 0.0, 176.0, 166.0, ox, oy);
                    // A server's furnace menu has the title it was given.
                    let title = this.remote_menu_title().unwrap_or_else(|| {
                        horizon_client::text::Text::literal(
                            this.container_title(this.translate_with_fallback(key, fallback)),
                        )
                        .resolve_legacy()
                    });
                    screen_info::name_container(|| title.plain());
                    let tw = atlas.font.width_text(&title);
                    batch.text_styled(
                        atlas,
                        &title,
                        ox + (176.0 - tw) / 2.0,
                        oy + 6.0,
                        grey,
                        false,
                    );
                    let inventory =
                        this.translate_with_fallback("container.inventory", "Inventory");
                    batch.text(atlas, &inventory, ox + 8.0, oy + 72.0, grey, false);
                    if f.lit_remaining > 0 {
                        // AbstractFurnaceScreen: flame height ceil(lit * 13) + 1.
                        let total = if f.lit_total == 0 { 200 } else { f.lit_total };
                        let h = ((f.lit_remaining as f32 / total as f32) * 13.0).ceil() + 1.0;
                        part(
                            batch,
                            "gui/sprites/container/furnace/lit_progress",
                            0.0,
                            14.0 - h,
                            14.0,
                            h,
                            ox + 56.0,
                            oy + 36.0 + 14.0 - h,
                        );
                    }
                    if f.cooking_timer > 0 && f.cooking_total > 0 {
                        let w = ((f.cooking_timer as f32 / f.cooking_total as f32) * 24.0).ceil();
                        part(
                            batch,
                            "gui/sprites/container/furnace/burn_progress",
                            0.0,
                            0.0,
                            w,
                            16.0,
                            ox + 79.0,
                            oy + 34.0,
                        );
                    }
                }
                Some(BlockEntity::Brewing(_)) => {}
                Some(BlockEntity::Crafter(_)) => this.draw_crafter_screen(batch, ox, oy),
                Some(
                    BlockEntity::Sign(_)
                    | BlockEntity::Jukebox(_)
                    | BlockEntity::Beacon(_)
                    | BlockEntity::Lectern(_)
                    | BlockEntity::Sculk(_)
                    | BlockEntity::Banner(_)
                    | BlockEntity::CopperGolemStatue(_)
                    | BlockEntity::CreakingHeart(_)
                    | BlockEntity::Spawner(_)
                    | BlockEntity::TrialSpawner(_)
                    | BlockEntity::Vault(_)
                    | BlockEntity::Beehive(_)
                    | BlockEntity::CommandBlock(_)
                    | BlockEntity::EndGateway(_)
                    | BlockEntity::Skull(_)
                    | BlockEntity::DaylightDetector
                    | BlockEntity::Bell
                    | BlockEntity::Conduit(_)
                    | BlockEntity::EndPortal
                    | BlockEntity::EnchantingTable
                    | BlockEntity::Campfire(_)
                    | BlockEntity::ChiseledBookshelf(_)
                    | BlockEntity::Brushable(_)
                    | BlockEntity::Shelf(_)
                    | BlockEntity::Mod(_)
                    | BlockEntity::PotentSulfur(_)
                    | BlockEntity::StructureBlock(_)
                    | BlockEntity::JigsawBlock(_)
                    | BlockEntity::TestBlock(_)
                    | BlockEntity::TestInstanceBlock(_),
                ) => {}
                None if this.mount_active() => this.draw_mount_background(batch, ox, oy),
                None => {
                    let texture = if this.craft_size == 3 {
                        "gui/container/crafting_table"
                    } else {
                        "gui/container/inventory"
                    };
                    part(batch, texture, 0.0, 0.0, 176.0, 166.0, ox, oy);
                    let title = this.translate_with_fallback("container.crafting", "Crafting");
                    screen_info::name_container(|| title.clone());
                    let tx = if this.craft_size == 3 { 29.0 } else { 97.0 };
                    batch.text(atlas, &title, ox + tx, oy + 6.0, grey, false);
                    if this.craft_size == 3 {
                        let inventory =
                            this.translate_with_fallback("container.inventory", "Inventory");
                        batch.text(atlas, &inventory, ox + 8.0, oy + 72.0, grey, false);
                    }
                }
            }
            if this.container.is_none() && this.craft_size != 3 && !this.mount_active() {
                this.draw_inventory_player(batch, ox, oy);
            }
        });
        let mut effect_tip = None;
        self.shell_leaf(batch, Leaf::Effects, |this, batch| {
            if this.container.is_none() && this.craft_size == 2 && !this.mount_active() {
                effect_tip = this.draw_inventory_effects(batch, (ox, oy), 176);
            }
        });
        self.shell_leaf(batch, Leaf::Widgets, |this, batch| {
            this.draw_sort_buttons(batch, ox, oy);
        });
        self.shell_leaf(batch, Leaf::Labels, |_, _| {});
        self.shell_leaf(batch, Leaf::HighlightBack, |this, batch| {
            this.draw_hovered_slot_highlight(batch, false, ox, oy);
        });
        self.shell_leaf(batch, Leaf::Slots, |this, batch| {
            this.draw_inventory_slots(batch, ox, oy);
        });
        self.shell_leaf(batch, Leaf::HighlightFront, |this, batch| {
            this.draw_hovered_slot_highlight(batch, true, ox, oy);
        });
        self.shell_leaf(batch, Leaf::Extra, |_, _| {});
        self.shell_leaf(batch, Leaf::RecipeBook, |this, batch| this.draw_recipe_book(batch));
        self.shell_leaf(batch, Leaf::Carried, |this, batch| this.draw_carried(batch));
        self.shell_leaf(batch, Leaf::Tooltip, |this, batch| {
            this.draw_slot_tooltip(batch);
            this.draw_native_screen_tooltip(batch);
            if let Some(lines) = effect_tip {
                this.draw_tooltip_lines(batch, &lines, this.cursor.0, this.cursor.1);
            }
        });
        self.shell_leaf(batch, Leaf::BookTooltip, |_, _| {});
        self.shell_leaf(batch, Leaf::Deferred, |_, _| {});
    }
}
