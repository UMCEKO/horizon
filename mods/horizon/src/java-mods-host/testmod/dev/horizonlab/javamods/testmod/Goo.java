package dev.horizonlab.javamods.testmod;

import java.util.function.Supplier;
import net.minecraft.core.registries.Registries;
import net.minecraft.sounds.SoundEvents;
import net.minecraft.world.item.BucketItem;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.Items;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.LiquidBlock;
import net.minecraft.world.level.block.state.BlockBehaviour;
import net.minecraft.world.level.material.FlowingFluid;
import net.minecraft.world.level.material.Fluid;
import net.neoforged.bus.api.IEventBus;
import net.neoforged.neoforge.common.SoundActions;
import net.neoforged.neoforge.fluids.BaseFlowingFluid;
import net.neoforged.neoforge.fluids.FluidType;
import net.neoforged.neoforge.registries.DeferredRegister;
import net.neoforged.neoforge.registries.NeoForgeRegistries;

/** A fluid as mods write one, with numbers no vanilla fluid has: the independent expected values of the fluid facts. */
public final class Goo {
    private static final DeferredRegister<FluidType> TYPES = DeferredRegister.create(NeoForgeRegistries.Keys.FLUID_TYPES, "horizon_java_test");
    private static final DeferredRegister<Fluid> FLUIDS = DeferredRegister.create(Registries.FLUID, "horizon_java_test");
    private static final DeferredRegister.Blocks BLOCKS = DeferredRegister.createBlocks("horizon_java_test");

    public static final Supplier<FluidType> TYPE = TYPES.register("goo", () -> new FluidType(FluidType.Properties.create()
        .motionScale(0.002)
        .canSwim(true)
        .canDrown(true)
        .fallDistanceModifier(0.5F)
        .canExtinguish(true)
        .canConvertToSource(false)
        .lightLevel(3)
        .density(2000)
        .viscosity(3000)
        .sound(SoundActions.BUCKET_FILL, SoundEvents.BUCKET_FILL_LAVA)
        .sound(SoundActions.BUCKET_EMPTY, SoundEvents.BUCKET_EMPTY_LAVA)));
    public static final Supplier<FlowingFluid> SOURCE = FLUIDS.register("goo", () -> new BaseFlowingFluid.Source(properties()));
    public static final Supplier<FlowingFluid> FLOWING = FLUIDS.register("flowing_goo", () -> new BaseFlowingFluid.Flowing(properties()));
    public static final Supplier<LiquidBlock> BLOCK = BLOCKS.registerBlock("goo", p -> new GooBlock(SOURCE.get(), p),
        () -> BlockBehaviour.Properties.ofFullCopy(Blocks.WATER).lightLevel(s -> 3));
    private static Supplier<Item> bucket;

    private Goo() {}

    private static BaseFlowingFluid.Properties properties() {
        return new BaseFlowingFluid.Properties(TYPE, SOURCE, FLOWING).tickRate(10).levelDecreasePerBlock(2).slopeFindDistance(2).explosionResistance(100F).block(BLOCK).bucket(() -> bucket.get());
    }

    static void register(IEventBus bus, DeferredRegister.Items items) {
        bucket = items.registerItem("goo_bucket", p -> new BucketItem(SOURCE.get(), p), () -> new Item.Properties().craftRemainder(Items.BUCKET).stacksTo(1));
        TYPES.register(bus);
        FLUIDS.register(bus);
        BLOCKS.register(bus);
    }

    /** Client only: the model and extensions every mod fluid has, with numbers no vanilla fluid has. */
    static void registerClient(IEventBus bus) {
        bus.addListener((net.neoforged.neoforge.client.event.RegisterFluidModelsEvent event) -> {
            var model = new net.minecraft.client.renderer.block.FluidModel.Unbaked(
                new net.minecraft.client.resources.model.sprite.Material(net.minecraft.resources.Identifier.withDefaultNamespace("block/water_still")),
                new net.minecraft.client.resources.model.sprite.Material(net.minecraft.resources.Identifier.withDefaultNamespace("block/water_flow")),
                null, net.neoforged.neoforge.client.fluid.FluidTintSources.constant(0xFF80FF00));
            event.register(model, SOURCE.get(), FLOWING.get());
        });
        bus.addListener((net.neoforged.neoforge.client.extensions.common.RegisterClientExtensionsEvent event) -> event.registerFluidType(new net.neoforged.neoforge.client.extensions.common.IClientFluidTypeExtensions() {
            @Override
            public net.minecraft.resources.Identifier getRenderOverlayTexture(net.minecraft.client.Minecraft minecraft) {
                return net.minecraft.resources.Identifier.withDefaultNamespace("textures/block/slime_block.png");
            }

            @Override
            public void modifyFogColor(net.minecraft.client.Camera camera, float partialTick, net.minecraft.client.multiplayer.ClientLevel level, int renderDistance, float darkenWorldAmount, org.joml.Vector4f fogColor) {
                fogColor.set(0.5F, 1.0F, 0.0F, 1.0F);
            }
        }, TYPE.get()));
    }

    private static final class GooBlock extends LiquidBlock {
        GooBlock(FlowingFluid fluid, BlockBehaviour.Properties properties) {
            super(fluid, properties);
        }
    }
}
