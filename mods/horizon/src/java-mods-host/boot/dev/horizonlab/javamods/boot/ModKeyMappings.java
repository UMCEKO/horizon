package dev.horizonlab.javamods.boot;

import com.mojang.blaze3d.platform.InputConstants;
import java.io.DataInputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.Collections;
import java.util.IdentityHashMap;
import java.util.Set;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.neoforged.neoforge.client.settings.IKeyConflictContext;
import net.neoforged.neoforge.client.settings.KeyConflictContext;
import net.neoforged.neoforge.client.settings.KeyModifier;

/** The client's key mappings: the mods' exported once the boot has run `RegisterKeyMappingsEvent`, and the window's rebinds applied to them. */
public final class ModKeyMappings {
    private ModKeyMappings() {}

    private static final Set<KeyMapping> VANILLA = Collections.newSetFromMap(new IdentityHashMap<>());

    /** `Options.keyMappings` as vanilla made it, before the mods' registration event (`KeyMappingsMixin`). */
    public static void snapshot(KeyMapping[] vanilla) {
        for (KeyMapping mapping : vanilla) VANILLA.add(mapping);
    }

    /** A mod's context is code: only its answers about the game's own contexts reach the window. */
    private static String context(IKeyConflictContext context) {
        if (context instanceof KeyConflictContext named) return named.name();
        boolean gui = context.conflicts(KeyConflictContext.GUI), game = context.conflicts(KeyConflictContext.IN_GAME);
        if (gui && game) return "UNIVERSAL";
        if (game) return "IN_GAME";
        if (!gui) System.err.println("Horizon: " + context.getClass().getName() + " conflicts with neither the GUI's nor the game's keys; its mappings are exported as the GUI's");
        return "GUI";
    }

    /** Every mapping, a line each: name, 1 when vanilla's, category, default key, default modifier, conflict context, tab separated. */
    static byte[] export() {
        if (VANILLA.isEmpty()) throw new IllegalStateException("no snapshot of vanilla's key mappings was taken");
        var out = new StringBuilder();
        for (KeyMapping mapping : Minecraft.getInstance().options.keyMappings) {
            out.append(mapping.getName()).append('\t').append(VANILLA.contains(mapping) ? '1' : '0').append('\t').append(mapping.getCategory().id()).append('\t')
                    .append(mapping.getDefaultKey().getName()).append('\t').append(mapping.getDefaultKeyModifier().name()).append('\t').append(context(mapping.getKeyConflictContext())).append('\n');
        }
        return out.toString().getBytes(StandardCharsets.UTF_8);
    }

    private static String text(DataInputStream in) throws IOException {
        return new String(in.readNBytes(in.readInt()), StandardCharsets.UTF_8);
    }

    /** The window's bindings for the mods' mappings, each as `Options.load` binds one, then the lookup rebuilt as the controls screen does. */
    static byte[] apply(DataInputStream in) throws IOException {
        for (int n = in.readInt(); n > 0; n--) {
            KeyMapping mapping = KeyMapping.get(text(in));
            InputConstants.Key key = InputConstants.getKey(text(in));
            KeyModifier modifier = KeyModifier.valueFromString(text(in));
            if (mapping != null) mapping.setKeyModifierAndCode(modifier, key);
        }
        KeyMapping.resetMapping();
        return new byte[0];
    }
}
