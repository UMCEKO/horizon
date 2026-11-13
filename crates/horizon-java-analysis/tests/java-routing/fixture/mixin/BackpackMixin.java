package fixture.mixin; import org.spongepowered.asm.mixin.*; import org.spongepowered.asm.mixin.injection.*; import org.spongepowered.asm.mixin.injection.callback.*;
        import net.minecraft.world.entity.item.ItemEntity;
        @Mixin(ItemEntity.class) public class BackpackMixin {
          private boolean isBackpack() { return ((Object) this) instanceof ItemEntity e && e.getItem().getItem() instanceof fixture.FakeBackpackItem; }
          @Inject(method = "tick", at = @At("HEAD")) void backpackTick(CallbackInfo ci) { if (!isBackpack()) return; System.out.println("backpack"); }
        }