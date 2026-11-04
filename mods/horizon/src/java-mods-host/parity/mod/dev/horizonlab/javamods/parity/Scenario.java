package dev.horizonlab.javamods.parity;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import com.mojang.blaze3d.platform.InputConstants;
import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.io.File;
import java.time.Instant;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;
import java.util.function.BooleanSupplier;
import jdk.jfr.FlightRecorder;
import jdk.jfr.Recording;
import net.minecraft.client.KeyMapping;
import net.minecraft.client.Minecraft;
import net.minecraft.client.Screenshot;
import net.minecraft.commands.arguments.EntityAnchorArgument;
import net.minecraft.world.phys.Vec3;
import net.neoforged.neoforge.client.event.RenderFrameEvent;
import net.neoforged.neoforge.client.event.ClientTickEvent;
import net.neoforged.neoforge.common.NeoForge;

/** The scenario's steps (also camera, hud, screenshot, press, release, lookat, tp), run from the client tick through vanilla's own key path; a step's start is logged and the JFR recording rotated first. */
final class Scenario {
    private record Step(String name, String kind, JsonElement arg) {}

    private final List<Step> steps = new ArrayList<>();
    private final Path stepsFile = Path.of(System.getProperty("horizon.parity.steps", "steps.txt"));
    private int index = -1;
    private int remaining;
    private KeyMapping held;
    private boolean finished;
    private BooleanSupplier until;
    private long frames;
    private final Map<String, KeyMapping> pressed = new HashMap<>();
    private final Path shots = Path.of(System.getProperty("horizon.parity.shots", "."));

    private static final int SETTLE_FRAMES = 10;
    private static final int CHUNK_QUIET_TICKS = 20;
    private static final int CHUNK_TICK_CAP = 600;

    static void install(Path file) {
        var scenario = new Scenario(file);
        NeoForge.EVENT_BUS.addListener((ClientTickEvent.Post event) -> scenario.tick());
        NeoForge.EVENT_BUS.addListener((RenderFrameEvent.Post event) -> scenario.frames++);
    }

    private Scenario(Path file) {
        try {
            JsonElement root = JsonParser.parseString(Files.readString(file));
            JsonArray array = root.isJsonObject() ? root.getAsJsonObject().getAsJsonArray("steps") : root.getAsJsonArray();
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
        while (true) {
            if (index >= 0) {
                if (until != null) {
                    if (!until.getAsBoolean()) return;
                    until = null;
                    remaining = 0;
                } else if (remaining > 0 && --remaining > 0) {
                    return;
                }
                release();
            }
            if (++index >= steps.size()) {
                boundary(steps.size() + " end");
                finished = true;
                mc.stop();
                return;
            }
            begin(mc, steps.get(index));
            if (until != null || remaining > 0) return;
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
            case "press", "release" -> {
                String name = step.arg.getAsString();
                boolean down = step.kind.equals("press");
                for (KeyMapping mapping : mc.options.keyMappings) {
                    if (!mapping.getName().equals(name)) continue;
                    KeyMapping.set(key(mapping), down);
                    if (down) pressed.put(name, mapping);
                    else pressed.remove(name);
                }
            }
            case "lookat" -> {
                JsonArray at = step.arg.getAsJsonArray();
                player.lookAt(EntityAnchorArgument.Anchor.EYES, new Vec3(at.get(0).getAsDouble(), at.get(1).getAsDouble(), at.get(2).getAsDouble()));
            }
            case "tp" -> teleport(player, step.arg.getAsJsonArray());
            case "camera" -> {
                JsonArray at = step.arg.getAsJsonArray();
                teleport(player, at);
                Vec3 target = new Vec3(at.get(0).getAsDouble(), at.get(1).getAsDouble(), at.get(2).getAsDouble());
                int[] quiet = {0};
                int[] ticks = {0};
                until = () -> {
                    ticks[0]++;
                    boolean there = mc.player != null && mc.player.position().distanceTo(target) < 1.5;
                    boolean idle = mc.levelRenderer.hasRenderedAllSections();
                    quiet[0] = there && idle ? quiet[0] + 1 : 0;
                    return (ticks[0] >= CHUNK_QUIET_TICKS && quiet[0] >= CHUNK_QUIET_TICKS) || ticks[0] >= CHUNK_TICK_CAP;
                };
            }
            case "hud" -> {
                boolean show = step.arg.getAsBoolean();
                if (mc.gui.hud.isHidden() == show) mc.gui.hud.toggle();
            }
            case "screenshot" -> {
                String name = step.arg.getAsString() + ".png";
                long first = frames + SETTLE_FRAMES;
                long[] second = {-1};
                until = () -> {
                    if (frames < first) return false;
                    if (second[0] < 0) {
                        Screenshot.grab(shots.toFile(), name, mc.gameRenderer.mainRenderTarget(), 1, message -> {});
                        second[0] = frames + SETTLE_FRAMES;
                    }
                    return frames >= second[0];
                };
            }
            case "check" -> {}
            case "quit" -> index = steps.size();
            default -> throw new IllegalStateException("unknown step " + step.kind);
        }
    }

    private static void teleport(net.minecraft.client.player.LocalPlayer player, JsonArray at) {
        String rotation = at.size() >= 5 ? " " + at.get(3).getAsDouble() + " " + at.get(4).getAsDouble() : "";
        player.connection.sendCommand("tp @s " + at.get(0).getAsDouble() + " " + at.get(1).getAsDouble() + " " + at.get(2).getAsDouble() + rotation);
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
