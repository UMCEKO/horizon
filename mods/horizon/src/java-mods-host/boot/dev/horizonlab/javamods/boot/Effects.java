package dev.horizonlab.javamods.boot;

import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.function.Supplier;
import net.minecraft.core.particles.BlockParticleOption;
import net.minecraft.core.particles.ColorParticleOption;
import net.minecraft.core.particles.DustColorTransitionOptions;
import net.minecraft.core.particles.DustParticleOptions;
import net.minecraft.core.particles.ParticleOptions;
import net.minecraft.core.particles.PowerParticleOption;
import net.minecraft.core.particles.SimpleParticleType;
import net.minecraft.core.particles.SpellParticleOption;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.NbtOps;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.util.ProblemReporter;
import net.minecraft.world.damagesource.DamageSource;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.item.ItemEntity;
import net.minecraft.world.item.ItemInstance;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.item.ItemStackTemplate;
import net.minecraft.world.level.LevelAccessor;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.storage.TagValueOutput;
import net.minecraft.world.level.storage.loot.LootParams;
import net.minecraft.world.level.storage.loot.parameters.LootContextParams;
import net.minecraft.world.phys.Vec3;

/** What the drop, particle and explosion bridges pass between Java and Rust. */
final class Effects {
    private Effects() {}

    static byte[] nbt(CompoundTag tag) {
        try {
            var bytes = new ByteArrayOutputStream();
            NbtIo.write(tag, new DataOutputStream(bytes));
            return bytes.toByteArray();
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    /** A stack's item NBT; empty for no stack, which has no encoding. */
    static byte[] stack(ItemStack stack) {
        if (stack == null || stack.isEmpty()) return new byte[0];
        var ops = GameBridge.registries().createSerializationContext(NbtOps.INSTANCE);
        return nbt((CompoundTag) ItemStack.CODEC.encodeStart(ops, stack).getOrThrow());
    }

    private static ItemStack stack(byte[] bytes) throws IOException {
        var tag = NbtIo.read(new DataInputStream(new ByteArrayInputStream(bytes)), NbtAccounter.unlimitedHeap());
        return ItemStack.CODEC.parse(GameBridge.registries().createSerializationContext(NbtOps.INSTANCE), tag).getOrThrow();
    }

    /** A count, then each stack's length and NBT. */
    static List<ItemStack> stacks(byte[] bytes) {
        try {
            var in = new DataInputStream(new ByteArrayInputStream(bytes));
            var stacks = new ArrayList<ItemStack>();
            for (int n = in.readInt(); n > 0; n--) stacks.add(stack(in.readNBytes(in.readInt())));
            return stacks;
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    /** The entity a drop would spawn, as NBT, with the pickup delay `Block.popResource` gives it. */
    static byte[] entity(Supplier<? extends Entity> factory) {
        Entity entity = factory.get();
        if (entity instanceof ItemEntity item) item.setDefaultPickUpDelay();
        var output = TagValueOutput.createWithContext(ProblemReporter.DISCARDING, entity.level().registryAccess());
        return entity.save(output) ? nbt(output.buildResult()) : new byte[0];
    }

    /** The dimension of a level that is a server's, else nothing. */
    static String level(LevelAccessor level) {
        return level instanceof ServerLevel server ? server.dimension().identifier().toString() : "";
    }

    static String damageType(DamageSource source) {
        return source == null ? "" : source.typeHolder().unwrapKey().map(key -> key.identifier().toString()).orElse("");
    }

    private static void text(DataOutputStream out, String value) throws IOException {
        byte[] bytes = value.getBytes(StandardCharsets.UTF_8);
        out.writeInt(bytes.length);
        out.write(bytes);
    }

    private static void colour(DataOutputStream out, org.joml.Vector3f colour) throws IOException {
        out.writeFloat(colour.x());
        out.writeFloat(colour.y());
        out.writeFloat(colour.z());
    }

    /** The type's name, a kind (0 plain, 1 block, 2 dust, 3 dust transition, 4 colour, 5 spell, 6 power, 9 a payload Horizon can't carry), then its fields. */
    static byte[] particle(ParticleOptions options) {
        try {
            var bytes = new ByteArrayOutputStream();
            var out = new DataOutputStream(bytes);
            text(out, BuiltInRegistries.PARTICLE_TYPE.getKey(options.getType()).toString());
            switch (options) {
                case BlockParticleOption block -> {
                    out.writeInt(1);
                    out.writeInt(Block.BLOCK_STATE_REGISTRY.getId(block.getState()));
                }
                case DustParticleOptions dust -> {
                    out.writeInt(2);
                    colour(out, dust.getColor());
                    out.writeFloat(dust.getScale());
                }
                case DustColorTransitionOptions transition -> {
                    out.writeInt(3);
                    colour(out, transition.getFromColor());
                    colour(out, transition.getToColor());
                    out.writeFloat(transition.getScale());
                }
                case ColorParticleOption colour -> {
                    out.writeInt(4);
                    out.writeFloat(colour.getRed());
                    out.writeFloat(colour.getGreen());
                    out.writeFloat(colour.getBlue());
                    out.writeFloat(colour.getAlpha());
                }
                case SpellParticleOption spell -> {
                    out.writeInt(5);
                    out.writeFloat(spell.getRed());
                    out.writeFloat(spell.getGreen());
                    out.writeFloat(spell.getBlue());
                    out.writeFloat(spell.getPower());
                }
                case PowerParticleOption power -> {
                    out.writeInt(6);
                    out.writeFloat(power.getPower());
                }
                case SimpleParticleType plain -> out.writeInt(0);
                default -> out.writeInt(9);
            }
            return bytes.toByteArray();
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    /** What `BlockBehaviour.getDrops` reads of its loot parameters: the tool, the origin, whether an entity broke it, the explosion's radius. */
    static byte[] lootParams(LootParams.Builder params) {
        try {
            ItemInstance tool = params.getOptionalParameter(LootContextParams.TOOL);
            ItemStack held = tool instanceof ItemStack stack ? stack : tool instanceof ItemStackTemplate template ? template.create() : ItemStack.EMPTY;
            Vec3 origin = params.getOptionalParameter(LootContextParams.ORIGIN);
            Float radius = params.getOptionalParameter(LootContextParams.EXPLOSION_RADIUS);
            var bytes = new ByteArrayOutputStream();
            var out = new DataOutputStream(bytes);
            byte[] stack = stack(held);
            out.writeInt(stack.length);
            out.write(stack);
            out.writeBoolean(origin != null);
            out.writeDouble(origin == null ? 0 : origin.x);
            out.writeDouble(origin == null ? 0 : origin.y);
            out.writeDouble(origin == null ? 0 : origin.z);
            out.writeBoolean(params.getOptionalParameter(LootContextParams.THIS_ENTITY) != null);
            out.writeBoolean(radius != null);
            out.writeFloat(radius == null ? 0 : radius);
            return bytes.toByteArray();
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }
}
