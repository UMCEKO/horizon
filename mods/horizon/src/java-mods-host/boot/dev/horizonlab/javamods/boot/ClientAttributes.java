package dev.horizonlab.javamods.boot;

import dev.horizonlab.javamods.boot.WorldFrame.Cam;
import java.io.DataOutputStream;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.util.List;
import net.minecraft.client.Minecraft;
import net.minecraft.world.attribute.AttributeTypes;
import net.minecraft.world.attribute.EnvironmentAttribute;
import net.minecraft.world.attribute.EnvironmentAttributeProbe;
import net.minecraft.world.attribute.EnvironmentAttributes;
import net.minecraft.world.phys.Vec3;
import org.joml.Vector3fc;
import org.joml.Vector4fc;

/** The values of the camera's real attribute probe when a mod patches the attribute system: native sky, fog, light and clouds read them, lerped per frame as vanilla's probe lerps. */
public final class ClientAttributes {
    private ClientAttributes() {}

    private static final String[] KEYS = {
        "net/minecraft/world/attribute/EnvironmentAttributeProbe#tick(Lnet/minecraft/world/level/Level;Lnet/minecraft/world/phys/Vec3;)V",
        "net/minecraft/world/attribute/EnvironmentAttributeSystem#addDynamicLayers(Lnet/minecraft/world/attribute/EnvironmentAttributeSystem$Builder;Lnet/minecraft/world/level/Level;)V",
        "net/minecraft/world/attribute/EnvironmentAttributeSystem$ValueSampler#computeValueNotPositional()Ljava/lang/Object;",
        "net/minecraft/world/attribute/EnvironmentAttributeSystem$ValueSampler#computeValuePositional(Lnet/minecraft/world/phys/Vec3;Lnet/minecraft/world/attribute/SpatialAttributeInterpolator;)Ljava/lang/Object;",
    };

    /** An attribute native's `Sky` reads, by the id native names it. */
    record Visual(String id, EnvironmentAttribute<?> attribute) {}

    /** Exactly what `environment.rs`' `sample` reads. */
    static final List<Visual> VISUAL = List.of(
            new Visual("visual/sky_color", EnvironmentAttributes.SKY_COLOR),
            new Visual("visual/fog_color", EnvironmentAttributes.FOG_COLOR),
            new Visual("visual/sky_light_factor", EnvironmentAttributes.SKY_LIGHT_FACTOR),
            new Visual("visual/sky_light_color", EnvironmentAttributes.SKY_LIGHT_COLOR),
            new Visual("visual/block_light_tint", EnvironmentAttributes.BLOCK_LIGHT_TINT),
            new Visual("visual/ambient_light_color", EnvironmentAttributes.AMBIENT_LIGHT_COLOR),
            new Visual("visual/sun_angle", EnvironmentAttributes.SUN_ANGLE),
            new Visual("visual/moon_angle", EnvironmentAttributes.MOON_ANGLE),
            new Visual("visual/star_angle", EnvironmentAttributes.STAR_ANGLE),
            new Visual("visual/star_brightness", EnvironmentAttributes.STAR_BRIGHTNESS),
            new Visual("visual/sunrise_sunset_color", EnvironmentAttributes.SUNRISE_SUNSET_COLOR),
            new Visual("visual/cloud_color", EnvironmentAttributes.CLOUD_COLOR),
            new Visual("visual/fog_start_distance", EnvironmentAttributes.FOG_START_DISTANCE),
            new Visual("visual/fog_end_distance", EnvironmentAttributes.FOG_END_DISTANCE),
            new Visual("visual/cloud_fog_end_distance", EnvironmentAttributes.CLOUD_FOG_END_DISTANCE),
            new Visual("visual/cloud_height", EnvironmentAttributes.CLOUD_HEIGHT),
            new Visual("visual/water_fog_color", EnvironmentAttributes.WATER_FOG_COLOR),
            new Visual("gameplay/sky_light_level", EnvironmentAttributes.SKY_LIGHT_LEVEL));

    private static volatile boolean routed;

    /** Runs once the routes are known. */
    static void decide() {
        var on = new java.util.ArrayList<String>();
        for (String key : KEYS) if (Routing.route(key) >= 0) on.add(key.substring(key.indexOf('#') + 1));
        routed = !on.isEmpty();
        if (routed) System.err.println("Horizon: environment attributes from Java's probe (" + String.join(", ", on) + ")");
    }

    /** Before the tick: `Camera.tick` (inside `Minecraft.tick`) probes at the window's camera, as vanilla's does at the camera's. */
    static void place(Minecraft minecraft, Cam cam) {
        if (!routed || cam == null || minecraft.level == null || minecraft.player == null) return;
        ((dev.horizonlab.javamods.boot.mixin.CameraAccess) minecraft.gameRenderer.mainCamera()).horizon$position(new Vec3(cam.x(), cam.y(), cam.z()));
    }

    /** The status tail: whether a mod patched the probe, then the table of what the probe holds now. */
    static void write(DataOutputStream out, Minecraft minecraft) throws IOException {
        out.writeBoolean(routed);
        if (!routed || minecraft.level == null || minecraft.player == null) {
            out.writeInt(0);
            return;
        }
        table(out, minecraft.gameRenderer.mainCamera().attributeProbe());
    }

    /** Each visual attribute as the probe's last and new values: at 0 and 1 the types' lerps return their ends (the angle's snaps to the new one past 90 degrees, which native's lerp does too). */
    static void table(DataOutputStream out, EnvironmentAttributeProbe probe) throws IOException {
        out.writeInt(VISUAL.size());
        for (var visual : VISUAL) {
            var bytes = visual.id().getBytes(StandardCharsets.UTF_8);
            out.writeInt(bytes.length);
            out.write(bytes);
            put(out, visual.attribute(), probe);
        }
    }

    private static <V> void put(DataOutputStream out, EnvironmentAttribute<V> attribute, EnvironmentAttributeProbe probe) throws IOException {
        V last = probe.getValue(attribute, 0.0F);
        V now = probe.getValue(attribute, 1.0F);
        out.writeInt(tag(attribute));
        value(out, last);
        value(out, now);
    }

    static int tag(EnvironmentAttribute<?> attribute) {
        var type = attribute.type();
        if (type == AttributeTypes.FLOAT) return 0;
        if (type == AttributeTypes.RGB_COLOR) return 1;
        if (type == AttributeTypes.ARGB_COLOR) return 2;
        if (type == AttributeTypes.ANGLE_DEGREES) return 3;
        throw new IllegalStateException("an attribute native's sky does not read: " + attribute);
    }

    private static void value(DataOutputStream out, Object value) throws IOException {
        if (value instanceof Float f) out.writeFloat(f);
        else if (value instanceof Vector3fc v) {
            out.writeFloat(v.x());
            out.writeFloat(v.y());
            out.writeFloat(v.z());
        } else if (value instanceof Vector4fc v) {
            out.writeFloat(v.x());
            out.writeFloat(v.y());
            out.writeFloat(v.z());
            out.writeFloat(v.w());
        } else throw new IllegalStateException("unexpected attribute value " + value);
    }
}
