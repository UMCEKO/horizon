package dev.horizonlab.javamods.boot;

import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.neoforged.bus.api.BusBuilder;
import net.neoforged.bus.api.Event;

/** What counts as mod code in crafting (`CraftFacts`): a mod item's own methods and a listener on a bus. Ingredients need vanilla's bootstrap, which a plain JVM has no loader for: the bridges check covers recipes. */
public final class CraftFactsCheck {
    static class Plain extends Item {
        Plain(Properties p) { super(p); }
    }

    static class Crafted extends Item {
        Crafted(Properties p) { super(p); }

        @Override
        public void onCraftedBy(ItemStack stack, Player player) {}
    }

    static class DamageAndRepair extends Item {
        DamageAndRepair(Properties p) { super(p); }

        @Override
        public int getMaxDamage(ItemStack stack) { return 9; }

        @Override
        public boolean canGrindstoneRepair(ItemStack stack) { return true; }
    }

    static class Inherits extends DamageAndRepair {
        Inherits(Properties p) { super(p); }
    }

    private static final class Unlistened extends Event {}

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    public static void main(String[] args) {
        check(CraftFacts.itemBits(Plain.class) == 0, "an item with no methods of its own");
        check(CraftFacts.itemBits(Crafted.class) == 1, "onCraftedBy is bit 0");
        check(CraftFacts.itemBits(DamageAndRepair.class) == (1 << 9 | 1 << 8), "getMaxDamage and canGrindstoneRepair");
        check(CraftFacts.itemBits(Inherits.class) == (1 << 9 | 1 << 8), "a subclass keeps its superclass's methods");

        var bus = BusBuilder.builder().build();
        bus.start();
        check(!Listeners.listened(bus, Unlistened.class), "an event with no listener");
        bus.addListener(Unlistened.class, event -> {});
        check(Listeners.listened(bus, Unlistened.class), "an event with a listener");
        System.out.println("CraftFactsCheck ok");
    }
}
