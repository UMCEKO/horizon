//! The `recipe_book_categories` hooks and helpers: declarations the mod API generator reads (see `hooks/mod.rs`).

pub trait Hooks {
    /// Declare categories for custom recipe types, with registered item icons.
    /// Asked lazily once per recipe table by `native_recipe_book_category`.
    /// Invalid declarations are logged and skipped; the first valid category
    /// owns its id and type. This does not yet add tabs or book displays.
    #[hook(
        side = "both",
        contract = "listen",
        neoforge = "none (RegisterRecipeBookSearchCategoriesEvent registers search groups only, not a recipe type's categories)"
    )]
    fn recipe_book_categories();
}
