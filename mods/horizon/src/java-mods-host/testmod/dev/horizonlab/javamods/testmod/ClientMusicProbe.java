package dev.horizonlab.javamods.testmod;

import net.minecraft.sounds.Music;
import net.minecraft.sounds.SoundEvents;
import net.minecraft.sounds.SoundSource;
import net.neoforged.neoforge.client.event.SelectMusicEvent;
import net.neoforged.neoforge.client.event.sound.PlaySoundEvent;
import net.neoforged.neoforge.common.NeoForge;

/** A `SelectMusicEvent` listener: it hands the music tick to Java, records the first original, then overrides it. */
final class ClientMusicProbe {
    private ClientMusicProbe() {}

    private static boolean seen, overriding, played;

    static void register() {
        NeoForge.EVENT_BUS.addListener((SelectMusicEvent event) -> {
            if (!seen) {
                seen = true;
                Music original = event.getOriginalMusic();
                Checks.check("select_music_fired", () -> true);
                Checks.check("select_music_original_is_natives", () -> original != null && original.sound().value().location().equals(net.minecraft.sounds.Musics.CREATIVE.sound().value().location()));
                overriding = true;
                return;
            }
            if (overriding) event.overrideMusic(new Music(SoundEvents.MUSIC_CREDITS, 0, 0, true));
        });
        NeoForge.EVENT_BUS.addListener((PlaySoundEvent event) -> {
            if (!overriding || played || event.getSound() == null) return;
            if (event.getSound().getSource() != SoundSource.MUSIC || !event.getSound().getIdentifier().equals(SoundEvents.MUSIC_CREDITS.value().location())) return;
            played = true;
            Checks.check("music_override_played", () -> true);
        });
    }
}
