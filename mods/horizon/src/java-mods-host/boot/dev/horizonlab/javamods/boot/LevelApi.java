package dev.horizonlab.javamods.boot;

import net.minecraft.core.BlockPos;
import net.minecraft.core.Holder;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.ResourceKey;
import net.minecraft.world.Difficulty;
import net.minecraft.world.DifficultyInstance;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.storage.LevelData;

/** Results of the level bridges, built from the plain numbers and names Rust returns. */
final class LevelApi {
    private LevelApi() {}

    /** A registry entry from the registry's path, a tab, and the entry's id. */
    @SuppressWarnings({"unchecked", "rawtypes"})
    static Holder<?> holder(String key) {
        int tab = key.indexOf('\t');
        ResourceKey registry = ResourceKey.createRegistryKey(Identifier.parse(key.substring(0, tab)));
        return (Holder<?>) GameBridge.registries().lookupOrThrow(registry).getOrThrow(ResourceKey.create(registry, Identifier.parse(key.substring(tab + 1))));
    }

    /** Difficulty id, the time of day, the chunk's inhabited time and the moon's brightness. */
    static DifficultyInstance difficultyInstance(double[] v) {
        return new DifficultyInstance(Difficulty.byId((int) v[0]), (long) v[1], (long) v[2], (float) v[3]);
    }

    /** The overworld's spawn: x, y, z, yaw, pitch; nothing set is the default. */
    static LevelData.RespawnData respawnData(double[] v) {
        if (v == null || v.length < 5) return LevelData.RespawnData.DEFAULT;
        return LevelData.RespawnData.of(Level.OVERWORLD, new BlockPos((int) v[0], (int) v[1], (int) v[2]), (float) v[3], (float) v[4]);
    }
}
