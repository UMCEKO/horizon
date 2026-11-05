package dev.horizonlab.javamods.boot;

/** The steps of vanilla's container screen drawing native draws one by one; ids are wire values shared with Rust's `Leaf`. */
public final class ShellLeaves {
    private ShellLeaves() {}

    public static final int START = 0, BACKGROUND = 1, EFFECTS = 2, WIDGETS = 3, LABELS = 4, HIGHLIGHT_BACK = 5, SLOTS = 6, HIGHLIGHT_FRONT = 7,
            RECIPE_BOOK = 8, CARRIED = 9, TOOLTIP = 10, DEFERRED = 11, EXTRA = 12, BOOK_TOOLTIP = 13, COUNT = 14;

    private static final String[] NAMES = {"START", "BACKGROUND", "EFFECTS", "WIDGETS", "LABELS", "HIGHLIGHT_BACK", "SLOTS", "HIGHLIGHT_FRONT",
            "RECIPE_BOOK", "CARRIED", "TOOLTIP", "DEFERRED", "EXTRA", "BOOK_TOOLTIP"};

    public static String name(int leaf) {
        return NAMES[leaf];
    }
}
