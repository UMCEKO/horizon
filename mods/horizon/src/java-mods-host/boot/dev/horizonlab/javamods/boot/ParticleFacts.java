package dev.horizonlab.javamods.boot;

import dev.horizonlab.javamods.boot.mixin.ParticleEngineAccess;
import java.io.ByteArrayOutputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.BitSet;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import java.util.function.Predicate;
import net.minecraft.core.particles.ParticleType;
import net.minecraft.core.registries.BuiltInRegistries;

/** Which particle types are mods', by the boot's registry snapshot, and the registry's names for Rust. */
public final class ParticleFacts {
    private ParticleFacts() {}

    private static BitSet owned;
    private static boolean reported;

    private static synchronized BitSet owned() {
        if (owned == null) {
            var registry = BuiltInRegistries.PARTICLE_TYPE;
            var bits = new BitSet();
            for (var key : registry.keySet()) {
                if (BootFacts.owns(registry, key)) bits.set(registry.getId(registry.getValue(key)));
            }
            owned = bits;
        }
        return owned;
    }

    static boolean owned(ParticleType<?> type) {
        return owned().get(BuiltInRegistries.PARTICLE_TYPE.getId(type));
    }

    private static List<String> names() {
        var registry = BuiltInRegistries.PARTICLE_TYPE;
        var names = new ArrayList<String>();
        for (int id = 0; id < registry.size(); id++) names.add(registry.getKey(registry.byId(id)).toString());
        return names;
    }

    static byte[] export() throws IOException {
        return write(names(), owned());
    }

    static byte[] write(List<String> names, BitSet owned) throws IOException {
        var bytes = new ByteArrayOutputStream();
        var out = new DataOutputStream(bytes);
        out.writeInt(names.size());
        for (int id = 0; id < names.size(); id++) {
            byte[] text = names.get(id).getBytes(StandardCharsets.UTF_8);
            out.writeInt(text.length);
            out.write(text);
            out.writeBoolean(owned.get(id));
        }
        return bytes.toByteArray();
    }

    /** The vanilla types whose provider is a mod's class, sorted. */
    static List<String> replaced(Map<String, ?> providers, Predicate<String> owned) {
        var found = new ArrayList<String>();
        for (var entry : providers.entrySet()) {
            if (!owned.test(entry.getKey()) && Shells.isMods(entry.getValue().getClass())) found.add(entry.getKey());
        }
        found.sort(null);
        return found;
    }

    /** `client`: the window's Java side exists, so the providers it loaded can be looked at. */
    static synchronized void report(boolean client) {
        if (reported) return;
        reported = true;
        var names = names();
        var mods = new ArrayList<String>();
        for (int id = 0; id < names.size(); id++) if (owned().get(id)) mods.add(names.get(id));
        mods.sort(null);
        System.err.println("Horizon: " + mods.size() + " particle types are mods', ticked in Java: " + String.join(", ", mods));
        if (!client) return;
        var providers = new TreeMap<String, Object>();
        Client.providers().forEach((id, provider) -> providers.put(id.toString(), provider));
        for (String name : replaced(providers, mods::contains)) {
            System.err.println("Horizon: " + providers.get(name).getClass().getName() + " replaced the provider of " + name + "; native keeps drawing it (W3 does not route it)");
        }
    }

    /** Apart so the server dist never loads the client's classes. */
    private static final class Client {
        static Map<net.minecraft.resources.Identifier, net.minecraft.client.particle.ParticleProvider<?>> providers() {
            return ((ParticleEngineAccess) net.minecraft.client.Minecraft.getInstance().particleEngine).horizon$resources().getProviders();
        }
    }
}
