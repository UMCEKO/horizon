package dev.horizonlab.javamods.boot;

import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.io.IOException;
import java.io.UncheckedIOException;
import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.Map;
import net.minecraft.nbt.NbtAccounter;
import net.minecraft.nbt.NbtIo;
import net.minecraft.nbt.NbtOps;
import net.minecraft.world.scores.PlayerTeam;
import net.minecraft.world.scores.Scoreboard;

/** Java's server scoreboard made equal to Horizon's teams, set raw: the mods' handlers on the add and join methods never run for a sync. */
final class ScoreboardView {
    private ScoreboardView() {}

    private static final Map<String, Field> FIELDS = new HashMap<>();

    /** `long revision`, then `bool` and the teams when Java's view lags: `int n`, n × (length-prefixed packed team). */
    static void read(Crossing.In in) {
        in.readLong();
        if (!in.readBoolean()) return;
        var packed = new ArrayList<PlayerTeam.Packed>();
        for (int n = in.readInt(); n > 0; n--) packed.add(decode(in.readNBytes(in.readInt())));
        replace(GameBridge.server().getScoreboard(), packed);
    }

    /** The argument of a team method: the view's team of that name, else one built from the packed tag for a team the call removed. */
    static PlayerTeam team(String name, byte[] tag) {
        var board = GameBridge.server().getScoreboard();
        var team = board.getPlayerTeam(name);
        if (team != null) return team;
        var gone = new PlayerTeam(board, name);
        if (tag.length > 0) {
            var packed = decode(tag);
            copy(gone, packed);
            gone.getPlayers().addAll(packed.players());
        }
        return gone;
    }

    private static PlayerTeam.Packed decode(byte[] bytes) {
        try {
            var tag = NbtIo.read(new DataInputStream(new ByteArrayInputStream(bytes)), NbtAccounter.unlimitedHeap());
            return PlayerTeam.Packed.CODEC.parse(GameBridge.registries().createSerializationContext(NbtOps.INSTANCE), tag).getOrThrow();
        } catch (IOException e) {
            throw new UncheckedIOException(e);
        }
    }

    @SuppressWarnings("unchecked")
    private static void replace(Scoreboard board, java.util.List<PlayerTeam.Packed> packed) {
        try {
            var byName = (Map<String, PlayerTeam>) field(Scoreboard.class, "teamsByName").get(board);
            var byPlayer = (Map<String, PlayerTeam>) field(Scoreboard.class, "teamsByPlayer").get(board);
            var names = new java.util.HashSet<String>();
            for (var p : packed) names.add(p.name());
            byName.keySet().retainAll(names);
            byPlayer.clear();
            for (var p : packed) {
                var team = byName.get(p.name());
                if (team == null) {
                    team = new PlayerTeam(board, p.name());
                    byName.put(p.name(), team);
                }
                copy(team, p);
                team.getPlayers().clear();
                team.getPlayers().addAll(p.players());
                for (var member : p.players()) byPlayer.put(member, team);
            }
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("the scoreboard view could not be set", e);
        }
    }

    /** `PlayerTeam`'s properties, without its setters (each tells the scoreboard the team changed). */
    private static void copy(PlayerTeam team, PlayerTeam.Packed packed) {
        packed.displayName().ifPresent(name -> put(team, "displayName", name));
        put(team, "color", packed.color());
        put(team, "allowFriendlyFire", packed.allowFriendlyFire());
        put(team, "seeFriendlyInvisibles", packed.seeFriendlyInvisibles());
        put(team, "playerPrefix", packed.memberNamePrefix());
        put(team, "playerSuffix", packed.memberNameSuffix());
        put(team, "nameTagVisibility", packed.nameTagVisibility());
        put(team, "deathMessageVisibility", packed.deathMessageVisibility());
        put(team, "collisionRule", packed.collisionRule());
    }

    private static void put(PlayerTeam team, String name, Object value) {
        try {
            field(PlayerTeam.class, name).set(team, value);
        } catch (ReflectiveOperationException e) {
            throw new IllegalStateException("the team view could not set " + name, e);
        }
    }

    private static synchronized Field field(Class<?> owner, String name) throws ReflectiveOperationException {
        var key = owner.getName() + "#" + name;
        var found = FIELDS.get(key);
        if (found != null) return found;
        var field = owner.getDeclaredField(name);
        field.setAccessible(true);
        FIELDS.put(key, field);
        return field;
    }
}
