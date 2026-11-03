package dev.horizonlab.javamods.parity;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.mojang.blaze3d.platform.InputConstants;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.time.Instant;
import java.util.ArrayList;
import java.util.List;
import jdk.jfr.FlightRecorder;
import jdk.jfr.Recording;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.common.NeoForge;

/** The scenario's steps, run from the client tick through vanilla's own key path; a step's start is logged and the JFR recording rotated first. */
final class Scenario {
    private record Step(String name, String kind, JsonElement arg) {}

    private final List<Step> steps = new ArrayList<>();
    private final Path stepsFile = Path.of(System.getProperty("horizon.parity.steps", "steps.txt"));
    private int index = -1;
    private int remaining;
    private KeyMapping held;
    private boolean finished;

    static void install(Path file) {
        var scenario = new Scenario(file);
        NeoForge.EVENT_BUS.addListener((ClientTickEvent.Post event) -> scenario.tick());
    }

    private Scenario(Path file) {
        try {
            JsonArray array = JsonParser.parseString(Files.readString(file)).getAsJsonArray();
            String pending = null;
            for (JsonElement element : array) {
                JsonObject object = element.getAsJsonObject();
                if (object.has("name")) {
                    pending = object.get("name").getAsString();
                    continue;
                }
                String kind = object.keySet().iterator().next();
                steps.add(new Step(pending != null ? pending : kind, kind, object.get(kind)));
                pending = null;
            }
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }

    private void tick() {
        Minecraft mc = Minecraft.getInstance();
        if (finished || mc.player == null || mc.level == null) return;
        if (index >= 0 && remaining > 0 && --remaining > 0) return;
        while (remaining <= 0) {
            release();
            if (++index >= steps.size()) {
                boundary(steps.size() + " end");
                finished = true;
                mc.stop();
                return;
            }
            begin(mc, steps.get(index));
        }
    }

    private void release() {
        if (held == null) return;
        KeyMapping.set(key(held), false);
        held = null;
    }

    private static InputConstants.Key key(KeyMapping mapping) {
        return InputConstants.getKey(mapping.saveString());
    }

    private void begin(Minecraft mc, Step step) {
        boundary(index + " " + step.name);
        remaining = 0;
        var player = mc.player;
        switch (step.kind) {
            case "wait" -> remaining = step.arg.getAsInt();
            case "command" -> player.connection.sendCommand(step.arg.getAsString().replaceFirst("^/", ""));
            case "look" -> {
                JsonArray rotation = step.arg.getAsJsonArray();
                player.setYRot(rotation.get(0).getAsFloat());
                player.setXRot(rotation.get(1).getAsFloat());
            }
            case "slot" -> player.getInventory().setSelectedSlot(step.arg.getAsInt());
            case "use", "attack" -> {
                held = step.kind.equals("use") ? mc.options.keyUse : mc.options.keyAttack;
                KeyMapping.set(key(held), true);
                KeyMapping.click(key(held));
                remaining = step.arg.getAsInt();
            }
            case "key" -> {
                for (KeyMapping mapping : mc.options.keyMappings) {
                    if (mapping.getName().equals(step.arg.getAsString())) KeyMapping.click(key(mapping));
                }
            }
            case "close" -> player.closeContainer();
            default -> throw new IllegalStateException("unknown step " + step.kind);
        }
    }

    private void boundary(String label) {
        try {
            Instant now = Instant.now();
            Files.writeString(stepsFile, "step " + label + " " + System.nanoTime() + " " + (now.getEpochSecond() * 1_000_000_000L + now.getNano()) + "\n",
                java.nio.file.StandardOpenOption.CREATE, java.nio.file.StandardOpenOption.APPEND);
            for (Recording recording : FlightRecorder.getFlightRecorder().getRecordings()) {
                if (recording.getName().equals("parity")) {
                    boolean last = label.endsWith(" end");
                    Path file = last ? Path.of(System.getProperty("horizon.parity.jfr")) : stepsFile.resolveSibling("rotate.jfr");
                    recording.dump(file);
                    if (!last) Files.deleteIfExists(file);
                }
            }
        } catch (IOException e) {
            throw new IllegalStateException(e);
        }
    }
}
