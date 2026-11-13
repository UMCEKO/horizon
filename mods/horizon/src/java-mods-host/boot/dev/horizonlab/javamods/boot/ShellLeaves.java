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

    private static final String PKG = "net/minecraft/client/gui/screens/inventory/";
    private static final String G = "Lnet/minecraft/client/gui/GuiGraphicsExtractor;";
    private static final String XY = "(" + G + "II)V", XYA = "(" + G + "IIF)V", ONLY = "(" + G + ")V";

    /** Per declaring class: the leaf and the method (name and descriptor) vanilla runs at that step. */
    private static final Object[][] DECLARED = {
            {"AbstractContainerScreen", LABELS, "extractLabels", XY}, {"AbstractContainerScreen", HIGHLIGHT_BACK, "extractSlotHighlightBack", ONLY},
            {"AbstractContainerScreen", SLOTS, "extractSlots", XY}, {"AbstractContainerScreen", HIGHLIGHT_FRONT, "extractSlotHighlightFront", ONLY},
            {"AbstractContainerScreen", CARRIED, "extractCarriedItem", XY}, {"AbstractContainerScreen", TOOLTIP, "extractTooltip", XY},
            {"ItemCombinerScreen", BACKGROUND, "extractBackground", XYA},
            {"AnvilScreen", BACKGROUND, "extractBackground", XYA}, {"AnvilScreen", BACKGROUND, "extractErrorIcon", XY}, {"AnvilScreen", LABELS, "extractLabels", XY},
            {"SmithingScreen", BACKGROUND, "extractBackground", XYA}, {"SmithingScreen", BACKGROUND, "extractErrorIcon", XY},
            {"SmithingScreen", TOOLTIP, "extractRenderState", XYA},
            {"GrindstoneScreen", BACKGROUND, "extractBackground", XYA}, {"CartographyTableScreen", BACKGROUND, "extractBackground", XYA},
            {"LoomScreen", BACKGROUND, "extractBackground", XYA},
            {"StonecutterScreen", BACKGROUND, "extractBackground", XYA}, {"StonecutterScreen", TOOLTIP, "extractTooltip", XY},
            {"EnchantmentScreen", BACKGROUND, "extractBackground", XYA}, {"EnchantmentScreen", TOOLTIP, "extractRenderState", XYA},
            {"BrewingStandScreen", BACKGROUND, "extractBackground", XYA},
            {"AbstractFurnaceScreen", BACKGROUND, "extractBackground", XYA},
            {"BeaconScreen", BACKGROUND, "extractBackground", XYA}, {"BeaconScreen", LABELS, "extractLabels", XY},
            {"MerchantScreen", BACKGROUND, "extractBackground", XYA}, {"MerchantScreen", LABELS, "extractLabels", XY},
            {"MerchantScreen", EXTRA, "extractContents", XYA}, {"MerchantScreen$TradeOfferButton", TOOLTIP, "extractToolTip", XY},
    };

    /** The route keys (`pkg/Class#method(desc)`, as `routes.bin` writes them) of the vanilla methods a screen runs at a step, its own class's first. */
    public static String[] methods(Class<?> screen, int leaf) {
        var keys = new java.util.ArrayList<String>();
        for (Class<?> c = screen; c != null && c.getName().startsWith("net.minecraft."); c = c.getSuperclass()) {
            String name = c.getSimpleName();
            for (Object[] row : DECLARED) {
                String owner = (String) row[0];
                if ((int) row[1] == leaf && (owner.equals(name) || owner.startsWith(name + "$"))) keys.add(PKG + owner + "#" + row[2] + row[3]);
            }
        }
        return keys.toArray(new String[0]);
    }
}
