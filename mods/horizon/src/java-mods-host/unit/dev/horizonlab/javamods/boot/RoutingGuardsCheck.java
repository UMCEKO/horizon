package dev.horizonlab.javamods.boot;

import com.google.gson.JsonArray;
import com.google.gson.JsonParser;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.TreeSet;

/** Admission narrowed by a route's guards: mod-only classes make a route inert, an item guard admits the registry's items of that class. */
public final class RoutingGuardsCheck {
    static class ModOnly {}

    interface ModMarker {}

    private static final String SELF = "dev/horizonlab/javamods/boot/RoutingGuardsCheck";
    private static final String CONTAINER = "net/minecraft/world/SimpleContainer";

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    private static RouteGuards.Admission admit(String family, String guards) throws Exception {
        var owner = Class.forName(CONTAINER.replace('/', '.'), false, RoutingGuardsCheck.class.getClassLoader());
        return Routing.admission(family, owner, CONTAINER, "setChanged", "()V", new JsonArray(), JsonParser.parseString(guards), RoutingGuardsCheck.class.getClassLoader());
    }

    private static int[] written(RouteGuards.Admission admission, String family) throws Exception {
        var bytes = new ByteArrayOutputStream();
        Routing.write(new DataOutputStream(bytes), admission, family);
        var in = new DataInputStream(new ByteArrayInputStream(bytes.toByteArray()));
        return new int[] {in.readInt(), in.readInt(), in.readInt(), in.available()};
    }

    public static void main(String[] args) throws Exception {
        var modRecv = admit("value", "{\"recv\": [\"" + SELF + "$ModOnly\"]}");
        check(modRecv.inert(), "a mod-only receiver class on a value family admits nothing");
        check(java.util.Arrays.equals(written(modRecv, "value"), new int[] {-3, -1, -1, 0}), "an inert route is written as -3 with any items and blocks");
        check(!admit("value", "{\"recv\": [\"" + SELF + "$ModMarker\"]}").inert(), "an interface may be injected into a vanilla class");
        check(!admit("value", "{\"recv\": [\"" + CONTAINER + "\"]}").inert(), "a vanilla receiver class stays open");
        check(!admit("value", "{\"recv\": [\"no/such/Klass\"]}").inert(), "a class that doesn't load narrows nothing");
        check(admit("static", "{\"arg_class\": {\"0\": [\"" + SELF + "$ModOnly\"]}}").inert(), "a mod-only argument class admits nothing");
        check(!admit("value", "null").inert() && admit("value", "null").items() == null, "no guards admit all");
        check(java.util.Arrays.equals(written(admit("value", "null"), "value"), new int[] {-1, -1, -1, 0}), "no guards are written as any");

        var guards = RouteGuards.parse(JsonParser.parseString("{\"recv_item\": [\"net/minecraft/world/item/BucketItem\"]}"), RoutingGuardsCheck.class.getClassLoader());
        var buckets = RouteGuards.admission("value", null, guards, RoutingGuardsCheck::bucketRun);
        // Items.java registers these with BucketItem or MobBucketItem; powder snow's SolidBucketItem is a BlockItem
        var expected = Set.of("minecraft:bucket", "minecraft:water_bucket", "minecraft:lava_bucket", "minecraft:pufferfish_bucket", "minecraft:salmon_bucket", "minecraft:cod_bucket",
                "minecraft:tropical_fish_bucket", "minecraft:axolotl_bucket", "minecraft:sulfur_cube_bucket", "minecraft:tadpole_bucket");
        check(new TreeSet<>(buckets.items()).equals(new TreeSet<>(expected)), "bucket items: " + new TreeSet<>(buckets.items()));
        check(!buckets.inert(), "an item list with members is live");
        var bytes = new ByteArrayOutputStream();
        Routing.write(new DataOutputStream(bytes), buckets, "value");
        var in = new DataInputStream(new ByteArrayInputStream(bytes.toByteArray()));
        check(in.readInt() == -1 && in.readInt() == expected.size(), "the item list follows the receivers");
        var read = new TreeSet<String>();
        for (int i = 0; i < expected.size(); i++) read.add(new String(in.readNBytes(in.readInt()), java.nio.charset.StandardCharsets.UTF_8));
        check(read.equals(new TreeSet<>(expected)) && in.readInt() == -1 && in.available() == 0, "the item ids, then any block");
        var modItems = RouteGuards.parse(JsonParser.parseString("{\"arg_item\": {\"1\": [\"" + SELF + "$ModOnly\"]}}"), RoutingGuardsCheck.class.getClassLoader());
        check(RouteGuards.admission("static", null, modItems, RoutingGuardsCheck::bucketRun).inert(), "no registered item of a class admits nothing");
        System.out.println("RoutingGuardsCheck ok");
    }

    /** The items Items.java registers from `BUCKET` to `TADPOLE_BUCKET`, each with the class it constructs; the registry itself needs a loader the unit JVM has not. */
    private static List<Map.Entry<String, Class<?>>> bucketRun() {
        try {
            var loader = RoutingGuardsCheck.class.getClassLoader();
            Class<?> bucket = Class.forName("net.minecraft.world.item.BucketItem", false, loader), mob = Class.forName("net.minecraft.world.item.MobBucketItem", false, loader);
            Class<?> solid = Class.forName("net.minecraft.world.item.SolidBucketItem", false, loader), snowball = Class.forName("net.minecraft.world.item.SnowballItem", false, loader);
            Class<?> item = Class.forName("net.minecraft.world.item.Item", false, loader);
            return List.of(Map.entry("minecraft:bucket", bucket), Map.entry("minecraft:water_bucket", bucket), Map.entry("minecraft:lava_bucket", bucket),
                    Map.entry("minecraft:powder_snow_bucket", solid), Map.entry("minecraft:snowball", snowball), Map.entry("minecraft:leather", item), Map.entry("minecraft:milk_bucket", item),
                    Map.entry("minecraft:pufferfish_bucket", mob), Map.entry("minecraft:salmon_bucket", mob), Map.entry("minecraft:cod_bucket", mob), Map.entry("minecraft:tropical_fish_bucket", mob),
                    Map.entry("minecraft:axolotl_bucket", mob), Map.entry("minecraft:sulfur_cube_bucket", mob), Map.entry("minecraft:tadpole_bucket", mob));
        } catch (ClassNotFoundException e) {
            throw new AssertionError(e);
        }
    }
}
