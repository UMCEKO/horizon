package dev.horizonlab.javamods.boot;

import net.minecraft.world.Difficulty;
import net.minecraft.world.level.storage.WritableLevelData;

/** What a view level answers about its world; Horizon's values arrive with each call. */
final class ViewLevelData implements WritableLevelData {
    long gameTime;
    RespawnData respawn = RespawnData.DEFAULT;

    @Override public RespawnData getRespawnData() { return respawn; }
    @Override public void setSpawn(RespawnData respawnData) { respawn = respawnData; }
    @Override public long getGameTime() { return gameTime; }
    // Bridged: Horizon's level data answers.
    @Override public boolean isHardcore() { throw new IllegalStateException("not bridged"); }
    @Override public Difficulty getDifficulty() { throw new IllegalStateException("not bridged"); }
    @Override public boolean isDifficultyLocked() { throw new IllegalStateException("not bridged"); }
}
