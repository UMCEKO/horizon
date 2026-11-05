package dev.horizonlab.parity;

import com.google.gson.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import java.lang.reflect.Field;
import java.lang.reflect.Modifier;
import java.lang.reflect.ParameterizedType;
import java.util.HashMap;
import java.util.Map;
import net.minecraft.world.entity.Avatar;
import net.minecraft.world.entity.EntityAttachment;
import net.minecraft.world.entity.EntityAttachments;
import net.minecraft.world.entity.EntityDimensions;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.EntityTypes;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.ai.attributes.DefaultAttributes;

/** Original exporter of entity type dimensions, attachment points, categories and default attributes. */
public final class VanillaEntityTypesExporter {
    @SuppressWarnings("unchecked")
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        var classes = new HashMap<EntityType<?>, Class<?>>();
        for (Field f : EntityTypes.class.getDeclaredFields()) {
            if (!Modifier.isStatic(f.getModifiers()) || !EntityType.class.isAssignableFrom(f.getType())) continue;
            f.setAccessible(true);
            var arg = ((ParameterizedType) f.getGenericType()).getActualTypeArguments()[0];
            var cls = arg instanceof Class<?> c ? c : (Class<?>) ((ParameterizedType) arg).getRawType();
            classes.put((EntityType<?>) f.get(null), cls);
        }
        var out = new JsonObject();
        for (var type : BuiltInRegistries.ENTITY_TYPE) {
            var o = new JsonObject();
            var d = type.getDimensions();
            o.addProperty("width", d.width());
            o.addProperty("height", d.height());
            o.addProperty("eye_height", d.eyeHeight());
            o.add("attachments", points(d.attachments()));
            o.add("dimensions", staticDimensions(classes.get(type)));
            var id = BuiltInRegistries.ENTITY_TYPE.getKey(type).toString();
            if (id.equals("minecraft:player") || id.equals("minecraft:mannequin")) {
                var poses = new JsonObject();
                Field table = Avatar.class.getDeclaredField("POSES");
                table.setAccessible(true);
                for (var e : ((Map<net.minecraft.world.entity.Pose, EntityDimensions>) table.get(null)).entrySet()) poses.add(e.getKey().name(), dimensions(e.getValue()));
                o.add("poses", poses);
            }
            o.addProperty("category", type.getCategory().getName());
            o.addProperty("fire_immune", type.fireImmune());
            if (DefaultAttributes.hasSupplier(type)) {
                var supplier = DefaultAttributes.getSupplier((EntityType<? extends LivingEntity>) type);
                var attributes = new JsonObject();
                for (var attribute : BuiltInRegistries.ATTRIBUTE.asHolderIdMap()) {
                    if (supplier.hasAttribute(attribute))
                        attributes.addProperty(attribute.unwrapKey().orElseThrow().identifier().toString(), supplier.getBaseValue(attribute));
                }
                o.add("attributes", attributes);
            }
            out.add(BuiltInRegistries.ENTITY_TYPE.getKey(type).toString(), o);
        }
        Files.writeString(Path.of(args[0]), new Gson().toJson(out), StandardCharsets.UTF_8);
    }

    private static JsonObject staticDimensions(Class<?> cls) throws Exception {
        var chain = new java.util.ArrayList<Class<?>>();
        for (Class<?> c = cls; c != null; c = c.getSuperclass()) chain.add(0, c);
        var out = new JsonObject();
        for (Class<?> c : chain) {
            for (Field f : c.getDeclaredFields()) {
                if (!Modifier.isStatic(f.getModifiers()) || f.getType() != EntityDimensions.class) continue;
                f.setAccessible(true);
                out.add(f.getName(), dimensions((EntityDimensions) f.get(null)));
            }
        }
        return out;
    }

    private static JsonObject dimensions(EntityDimensions d) {
        var o = new JsonObject();
        o.addProperty("width", d.width());
        o.addProperty("height", d.height());
        o.addProperty("eye_height", d.eyeHeight());
        o.addProperty("fixed", d.fixed());
        o.add("attachments", points(d.attachments()));
        return o;
    }

    private static JsonObject points(EntityAttachments attachments) {
        var o = new JsonObject();
        o.add("passenger", list(attachments, EntityAttachment.PASSENGER));
        o.add("vehicle", list(attachments, EntityAttachment.VEHICLE));
        return o;
    }

    private static JsonArray list(EntityAttachments attachments, EntityAttachment kind) {
        var out = new JsonArray();
        for (int i = 0; ; i++) {
            var p = attachments.getNullable(kind, i, 0f);
            if (p == null) break;
            out.add(new Gson().toJsonTree(new double[] {p.x, p.y, p.z}));
        }
        return out;
    }
}
