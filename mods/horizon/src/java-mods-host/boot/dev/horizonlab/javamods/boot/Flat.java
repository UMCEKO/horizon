package dev.horizonlab.javamods.boot;

import dev.horizonlab.javamods.host.Host;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.io.UncheckedIOException;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.core.Holder;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.nbt.CompoundTag;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.NbtOps;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.ComponentSerialization;
import net.minecraft.stats.Stat;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.levelgen.Heightmap;
import net.minecraft.world.phys.Vec3;

/** What a bridge passes between Java and Rust: `raw` flattens an argument, `to` + the type's name rebuilds a result. */
public final class Flat {
    private Flat() {}

    public static boolean ON = true;

    public static void link(Class<?> target) {
        Host.link(target);
    }

    /** An entity by its id: the view's identity in Horizon's level. */
    public static int raw(net.minecraft.world.entity.Entity entity) {
        return entity == null ? 0 : entity.getId();
    }

    public static int raw(net.minecraft.world.entity.Entity.RemovalReason reason) {
        return reason.ordinal();
    }

    public static int raw(net.minecraft.world.entity.MoverType type) {
        return type.ordinal();
    }

    public static int raw(net.minecraft.world.level.pathfinder.PathType type) {
        return type.ordinal();
    }

    public static int raw(net.minecraft.world.entity.EquipmentSlot slot) {
        return slot.ordinal();
    }

    public static int raw(net.minecraft.world.item.DyeColor color) {
        return color.getId();
    }

    /** An entity by its UUID, as text (empty for none): what vanilla keeps in an `EntityReference`. */
    public static String rawUuid(net.minecraft.world.entity.Entity entity) {
        return entity == null ? "" : entity.getUUID().toString();
    }

    /** A UUID as text, empty for none. */
    public static String raw(java.util.UUID id) {
        return id == null ? "" : id.toString();
    }

    /** A teleport's target: position, rotation, motion, then the level's name. */
    public static byte[] raw(net.minecraft.world.level.portal.TeleportTransition transition) {
        try {
            var bytes = new ByteArrayOutputStream();
            var out = new DataOutputStream(bytes);
            out.writeDouble(transition.position().x);
            out.writeDouble(transition.position().y);
            out.writeDouble(transition.position().z);
            out.writeFloat(transition.yRot());
            out.writeFloat(transition.xRot());
            out.writeDouble(transition.deltaMovement().x);
            out.writeDouble(transition.deltaMovement().y);
            out.writeDouble(transition.deltaMovement().z);
            byte[] dim = transition.newLevel().dimension().identifier().toString().getBytes(java.nio.charset.StandardCharsets.UTF_8);
            out.writeInt(dim.length);
            out.write(dim);
            return bytes.toByteArray();
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    public static net.minecraft.world.entity.Entity toEntity(int id) {
        return EntityViews.byId(id);
    }

    public static long raw(BlockPos pos) {
        return pos.asLong();
    }

    public static int raw(BlockState state) {
        return Block.BLOCK_STATE_REGISTRY.getId(state);
    }

    public static int raw(Block block) {
        return raw(block.defaultBlockState());
    }

    public static int raw(Direction direction) {
        return direction.ordinal();
    }

    /** The stat's type path, a tab, and its value's id. */
    public static String raw(Stat<?> stat) {
        return statKey(stat);
    }

    private static <T> String statKey(Stat<T> stat) {
        var type = stat.getType();
        return BuiltInRegistries.STAT_TYPE.getKey(type).getPath() + "\t" + type.getRegistry().getKey(stat.getValue());
    }

    public static String raw(Holder<?> holder) {
        return holder.unwrapKey().map(key -> key.identifier().toString()).orElse("");
    }

    public static double[] raw(Vec3 point) {
        return new double[] {point.x, point.y, point.z};
    }

    public static String raw(Heightmap.Types type) {
        return type.getSerializationKey().toLowerCase(java.util.Locale.ROOT);
    }

    public static byte[] raw(Component message) {
        if (message == null) return new byte[0];
        try {
            var tag = new CompoundTag();
            tag.put("text", ComponentSerialization.CODEC.encodeStart(NbtOps.INSTANCE, message).getOrThrow());
            var bytes = new ByteArrayOutputStream();
            NbtIo.write(tag, new DataOutputStream(bytes));
            return bytes.toByteArray();
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    public static byte[] raw(net.minecraft.world.item.ItemStack stack) {
        return Effects.stack(stack);
    }

    public static byte[] raw(java.util.function.Supplier<? extends net.minecraft.world.entity.Entity> factory) {
        return Effects.entity(factory);
    }

    private static final java.util.List<net.minecraft.resources.ResourceKey<net.minecraft.world.level.Level>> OTHER_LEVELS = new java.util.concurrent.CopyOnWriteArrayList<>();

    /** The level a level bridge was called on: 0 to 2 for vanilla's three, else 3 plus its place for `levelName`. */
    public static int level(net.minecraft.server.level.ServerLevel level) {
        var key = level.dimension();
        if (key == net.minecraft.world.level.Level.OVERWORLD) return 0;
        if (key == net.minecraft.world.level.Level.NETHER) return 1;
        if (key == net.minecraft.world.level.Level.END) return 2;
        int at = OTHER_LEVELS.indexOf(key);
        if (at < 0) {
            synchronized (OTHER_LEVELS) {
                at = OTHER_LEVELS.indexOf(key);
                if (at < 0) {
                    OTHER_LEVELS.add(key);
                    at = OTHER_LEVELS.size() - 1;
                }
            }
        }
        return 3 + at;
    }

    public static int level(net.minecraft.server.level.ServerChunkCache cache) {
        return level((net.minecraft.server.level.ServerLevel) cache.getLevel());
    }

    public static String levelName(int level) {
        return OTHER_LEVELS.get(level - 3).identifier().toString();
    }

    /** The dimension of a server's level, else empty. */
    public static String raw(net.minecraft.world.level.Level level) {
        return Effects.level(level);
    }

    public static String raw(net.minecraft.world.level.LevelAccessor level) {
        return Effects.level(level);
    }

    public static byte[] raw(net.minecraft.core.particles.ParticleOptions options) {
        return Effects.particle(options);
    }

    public static byte[] raw(net.minecraft.world.level.storage.loot.LootParams.Builder params) {
        return Effects.lootParams(params);
    }

    public static int raw(net.minecraft.world.level.Level.ExplosionInteraction interaction) {
        return interaction.ordinal();
    }

    public static int raw(net.minecraft.network.protocol.game.ClientboundLevelParticlesPacket.RandomizationType type) {
        return type.ordinal();
    }

    /** A damage source: its type, the entities it names, whether a creative player caused it, and where it came from. */
    public static byte[] raw(net.minecraft.world.damagesource.DamageSource source) {
        try {
            var bytes = new ByteArrayOutputStream();
            var out = new DataOutputStream(bytes);
            byte[] type = Effects.damageType(source).getBytes(java.nio.charset.StandardCharsets.UTF_8);
            out.writeInt(type.length);
            out.write(type);
            out.writeInt(source == null || source.getEntity() == null ? 0 : source.getEntity().getId());
            out.writeInt(source == null || source.getDirectEntity() == null ? 0 : source.getDirectEntity().getId());
            out.writeBoolean(source != null && source.isCreativePlayer());
            var at = source == null ? null : source.getSourcePosition();
            out.writeBoolean(at != null);
            if (at != null) {
                out.writeDouble(at.x);
                out.writeDouble(at.y);
                out.writeDouble(at.z);
            }
            return bytes.toByteArray();
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    /** A lightning bolt as `thunderHit` sees it: whether it is Horizon's, its id, damage, flag, UUID and place. */
    public static byte[] raw(net.minecraft.world.entity.LightningBolt bolt) {
        try {
            var bytes = new ByteArrayOutputStream();
            var out = new DataOutputStream(bytes);
            out.writeBoolean(EntityViews.backed(bolt));
            out.writeInt(EntityViews.backed(bolt) ? bolt.getId() : 0);
            out.writeFloat(((dev.horizonlab.javamods.boot.mixin.LightningBoltAccess) bolt).horizon$damage());
            out.writeBoolean(((dev.horizonlab.javamods.boot.mixin.LightningBoltAccess) bolt).horizon$visualOnly());
            out.writeLong(bolt.getUUID().getMostSignificantBits());
            out.writeLong(bolt.getUUID().getLeastSignificantBits());
            out.writeDouble(bolt.getX());
            out.writeDouble(bolt.getY());
            out.writeDouble(bolt.getZ());
            return bytes.toByteArray();
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    public static java.util.List<net.minecraft.world.item.ItemStack> toList(byte[] stacks) {
        return Effects.stacks(stacks);
    }

    public static int raw(net.minecraft.world.InteractionHand hand) {
        return hand.ordinal();
    }

    /** The type's ordinal and the duration in ticks. */
    public static double[] raw(net.minecraft.world.item.component.SwingAnimation animation) {
        return new double[] {animation.type().ordinal(), animation.duration(), 0};
    }

    public static int raw(net.minecraft.world.level.GameType type) {
        return type.getId();
    }

    public static String raw(net.minecraft.resources.Identifier id) {
        return id.toString();
    }

    /** The effect's id, duration, amplifier, ambient and particles, tab-separated. */
    public static String raw(net.minecraft.world.effect.MobEffectInstance effect) {
        return raw(effect.getEffect()) + "\t" + effect.getDuration() + "\t" + effect.getAmplifier() + "\t" + (effect.isAmbient() ? 1 : 0) + "\t" + (effect.isVisible() ? 1 : 0);
    }

    /** Nutrition and saturation. */
    public static double[] raw(net.minecraft.world.food.FoodProperties food) {
        return new double[] {food.nutrition(), food.saturation(), 0};
    }

    public static int raw(net.minecraft.world.level.LightLayer layer) {
        return layer.ordinal();
    }

    /** Min x y z, then max x y z. */
    public static double[] raw(net.minecraft.world.phys.AABB box) {
        return new double[] {box.minX, box.minY, box.minZ, box.maxX, box.maxY, box.maxZ};
    }

    public static int raw(net.minecraft.world.ticks.TickPriority priority) {
        return priority.getValue();
    }

    public static String raw(net.minecraft.world.level.material.Fluid fluid) {
        return BuiltInRegistries.FLUID.getKey(fluid).toString();
    }

    /** The block position of the section's lowest corner. */
    public static long raw(net.minecraft.core.SectionPos section) {
        return section.origin().asLong();
    }

    public static Holder<?> toHolder(String registryTabId) {
        return LevelApi.holder(registryTabId);
    }

    public static it.unimi.dsi.fastutil.longs.LongSet toLongSet(long[] values) {
        return new it.unimi.dsi.fastutil.longs.LongOpenHashSet(values);
    }

    public static net.minecraft.world.Difficulty toDifficulty(int id) {
        return net.minecraft.world.Difficulty.byId(id);
    }

    public static net.minecraft.world.level.GameType toGameType(int id) {
        return net.minecraft.world.level.GameType.byId(id);
    }

    public static net.minecraft.server.permissions.LevelBasedPermissionSet toLevelBasedPermissionSet(int level) {
        return net.minecraft.server.permissions.LevelBasedPermissionSet.forLevel(net.minecraft.server.permissions.PermissionLevel.byId(level));
    }

    public static String raw(net.minecraft.server.players.NameAndId player) {
        return player.id().toString();
    }

    public static net.minecraft.world.DifficultyInstance toDifficultyInstance(double[] numbers) {
        return LevelApi.difficultyInstance(numbers);
    }

    public static net.minecraft.world.level.storage.LevelData.RespawnData toRespawnData(double[] numbers) {
        return LevelApi.respawnData(numbers);
    }

    public static long raw(net.minecraft.world.phys.shapes.VoxelShape shape) {
        return ShapeHandles.of(shape);
    }

    /** The hit's x y z, its face's ordinal, 1 if inside, then the block's x y z; empty for a miss. */
    public static net.minecraft.world.phys.BlockHitResult toBlockHitResult(double[] hit) {
        if (hit == null || hit.length == 0) return null;
        var pos = new BlockPos((int) hit[5], (int) hit[6], (int) hit[7]);
        return new net.minecraft.world.phys.BlockHitResult(new Vec3(hit[0], hit[1], hit[2]), Direction.from3DDataValue((int) hit[3]), pos, hit[4] != 0);
    }

    public static BlockState toBlockState(int id) {
        return Block.BLOCK_STATE_REGISTRY.byId(id);
    }
}
