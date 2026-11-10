package dev.horizonlab.javamods.boot;

import java.util.List;

/** The nearest source frame of a sound and whether native owns it. */
public final class SoundOwnersCheck {
    private static SoundOwners.Frame f(String cls, String method) {
        return new SoundOwners.Frame(cls, method);
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    public static void main(String[] args) {
        SoundOwners.parse(
                "net/minecraft/client/resources/sounds/BiomeAmbientSoundsHandler\t*\tnative\tambience.rs\n"
                        + "net/minecraft/client/multiplayer/ClientLevel\ttickWeatherEffects\tnative\trender_tick.rs\n"
                        + "net/minecraft/world/level/block/HoneyBlock\tmaybeDoSlideEffects\tjava\tmentioned\n",
                "block\tnet/minecraft/world/level/block/CampfireBlock\tanimateTick\t(I)V\tnet/minecraft/world/level/block/CampfireBlock\n");
        var plumbing = List.of(f("dev.horizonlab.javamods.boot.SoundOwners", "nativeOwnsCaller"), f("dev.horizonlab.javamods.boot.ClientSounds", "play"),
                f("net.minecraft.client.sounds.SoundEngine", "horizon$play"), f("net.minecraft.client.sounds.SoundManager", "play"),
                f("net.minecraft.client.multiplayer.ClientLevel", "playSound"), f("net.minecraft.client.multiplayer.ClientLevel", "playLocalSound"),
                f("net.minecraft.world.level.Level", "playLocalSound"), f("net.minecraft.world.entity.Entity", "playSound"),
                f("net.minecraft.client.player.LocalPlayer", "playNotifySound"), f("net.minecraft.client.multiplayer.ClientLevel", "lambda$playSound$0"));
        var weather = f("net.minecraft.client.multiplayer.ClientLevel", "tickWeatherEffects");
        var stack = new java.util.ArrayList<>(plumbing);
        stack.add(weather);
        stack.add(f("net.minecraft.client.multiplayer.ClientLevel", "tick"));
        check(SoundOwners.source(stack).equals(weather), "plumbing frames are skipped");
        check(SoundOwners.nativeOwns(weather), "ClientLevel#tickWeatherEffects is native's");
        check(SoundOwners.nativeOwns(f("net.minecraft.client.multiplayer.ClientLevel", "lambda$tickWeatherEffects$0")), "its lambda is too");
        check(!SoundOwners.nativeOwns(f("net.minecraft.client.multiplayer.ClientLevel", "tick")), "another ClientLevel method is not");
        check(SoundOwners.nativeOwns(f("net.minecraft.client.resources.sounds.BiomeAmbientSoundsHandler", "lambda$tick$0")), "a lambda of a * row");
        check(SoundOwners.nativeOwns(f("net.minecraft.client.resources.sounds.BiomeAmbientSoundsHandler$LoopSoundInstance", "tick")), "an inner class of a * row");
        check(!SoundOwners.nativeOwns(f("com.example.Mod", "play")), "a mod's frame is not native's");
        check(!SoundOwners.nativeOwns(f("net.minecraft.world.level.block.HoneyBlock", "maybeDoSlideEffects")), "a java row is not native's");
        check(SoundOwners.nativeOwns(f("net.minecraft.world.level.block.CampfireBlock", "animateTick")), "a closure row is native's");
        check(!SoundOwners.nativeOwns(f("net.minecraft.world.level.block.TorchBlock", "somethingElse")), "a vanilla frame outside the table plays");
        check(SoundOwners.source(plumbing) == null, "nothing but plumbing has no source");
        System.out.println("SoundOwnersCheck ok");
    }
}
