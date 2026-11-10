package dev.horizonlab.javamods.unit;

import dev.horizonlab.javamods.boot.ItemCode;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.InteractionResult;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.item.BlockItem;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.Level;

/** Which clicks run a mod item's own code (`ItemCode`), on classes that are only defined, never booted. */
public final class ItemCodeCheck {
    static class Plain extends Item {
        Plain(Properties p) { super(p); }
    }

    static class EatsWithEffects extends Item {
        EatsWithEffects(Properties p) { super(p); }

        @Override
        public ItemStack finishUsingItem(ItemStack stack, Level level, LivingEntity entity) { return stack; }
    }

    static class Charges extends Item {
        Charges(Properties p) { super(p); }

        @Override
        public void onUseTick(Level level, LivingEntity entity, ItemStack stack, int remaining) {}
    }

    static class Lasts extends Item {
        Lasts(Properties p) { super(p); }

        @Override
        public int getUseDuration(ItemStack stack, LivingEntity entity) { return 72000; }
    }

    static class FirstOnBlocks extends Item {
        FirstOnBlocks(Properties p) { super(p); }

        @Override
        public InteractionResult onItemUseFirst(ItemStack stack, UseOnContext context) { return InteractionResult.PASS; }
    }

    static class Tags extends Item {
        Tags(Properties p) { super(p); }

        @Override
        public InteractionResult interactLivingEntity(ItemStack stack, Player player, LivingEntity target, InteractionHand hand) { return InteractionResult.PASS; }
    }

    static class Plainer extends Plain {
        Plainer(Properties p) { super(p); }
    }

    static class PlacesItself extends BlockItem {
        PlacesItself(net.minecraft.world.level.block.Block block, Properties p) { super(block, p); }
    }

    static class PlacesWithData extends BlockItem {
        PlacesWithData(net.minecraft.world.level.block.Block block, Properties p) { super(block, p); }

        @Override
        public InteractionResult useOn(UseOnContext context) { return InteractionResult.PASS; }
    }

    static int failures;

    static void expect(Class<?> type, boolean use, boolean useOn, boolean entity) {
        boolean[] got = {ItemCode.use(type), ItemCode.useOn(type), ItemCode.entity(type)};
        boolean[] want = {use, useOn, entity};
        if (!java.util.Arrays.equals(got, want)) {
            failures++;
            System.out.println("FAIL " + type.getSimpleName() + ": use/useOn/entity " + java.util.Arrays.toString(got) + ", expected " + java.util.Arrays.toString(want));
        } else {
            System.out.println("ok   " + type.getSimpleName());
        }
    }

    public static void main(String[] args) {
        expect(Item.class, false, false, false);
        expect(Plain.class, false, false, false);
        expect(Plainer.class, false, false, false);
        expect(EatsWithEffects.class, true, false, false);
        expect(Charges.class, true, false, false);
        expect(Lasts.class, true, false, false);
        expect(FirstOnBlocks.class, false, true, false);
        expect(Tags.class, false, false, true);
        expect(BlockItem.class, false, false, false);
        expect(PlacesItself.class, false, false, false);
        expect(PlacesWithData.class, false, true, false);
        // A vanilla subclass Horizon ports by item id only, never by class: its own use is Java's.
        expect(net.minecraft.world.item.BowItem.class, true, false, false);
        System.out.println(failures == 0 ? "ItemCodeCheck: all passed" : "ItemCodeCheck: " + failures + " failed");
        System.exit(failures == 0 ? 0 : 1);
    }
}
