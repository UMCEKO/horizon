package dev.horizonlab.javamods.boot;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonArray;
import com.mojang.brigadier.CommandDispatcher;
import com.mojang.brigadier.builder.LiteralArgumentBuilder;
import com.mojang.serialization.JsonOps;
import dev.horizonlab.javamods.host.Host;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.List;
import net.minecraft.SharedConstants;
import net.minecraft.commands.CommandSourceStack;
import net.minecraft.commands.Commands;
import net.minecraft.core.Registry;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.component.TypedDataComponent;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.resources.RegistryOps;
import net.minecraft.server.Bootstrap;
import net.minecraft.sounds.SoundEvent;
import net.minecraft.world.item.Item;
import net.minecraft.world.item.ItemStack;
import net.neoforged.bus.api.Event;
import net.neoforged.bus.api.EventPriority;
import net.neoforged.fml.ModList;
import net.neoforged.neoforge.common.NeoForge;
import net.neoforged.neoforge.event.RegisterCommandsEvent;
import net.neoforged.neoforge.server.loading.ServerModLoader;
import org.spongepowered.asm.mixin.Mixins;
import org.spongepowered.asm.mixin.transformer.Config;

public final class Boot {
    private Boot() {}

    public static final class HzPing extends Event {
        public final String payload;
        public HzPing(String payload) { this.payload = payload; }
    }

    public static void main(String[] args) {
        Host.mark("boot_main");
        SharedConstants.tryDetectVersion();
        Host.mark("detect_version");
        Bootstrap.bootStrap();
        Host.mark("bootstrap");
        Bootstrap.validate();
        Host.mark("validate");
        ServerModLoader.load(false);
        Host.mark("mod_loading_done");
        Host.entry = Boot::op;
    }

    static RegistryAccess access() {
        return RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
    }

    static String op(String op, String arg) {
        try {
            return switch (op) {
                case "mods" -> mods();
                case "registries" -> registries();
                case "items" -> items(arg);
                case "mixin" -> mixin(arg);
                case "event" -> event();
                case "stack" -> stack(arg);
                case "listen" -> listen();
                case "bind" -> bind();
                case "stats" -> stats();
                case "shapes" -> ShapeBridge.report();
                case "nmt" -> nmt();
                case "exit" -> { System.exit(0); yield ""; }
                default -> "unknown op " + op;
            };
        } catch (Throwable t) {
            var sw = new java.io.StringWriter();
            t.printStackTrace(new java.io.PrintWriter(sw));
            return "ERROR " + sw;
        }
    }

    static String bind() {
        long t = System.nanoTime();
        var vanilla = net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
        long t1 = System.nanoTime();
        var provider = net.minecraft.core.HolderLookup.Provider.create(java.util.stream.Stream.concat(access().listRegistries(), vanilla.listRegistries().filter(r -> access().lookup(r.key()).isEmpty())));
        var pending = BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(provider);
        long t2 = System.nanoTime();
        pending.forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        NeoForge.EVENT_BUS.post(new net.neoforged.neoforge.event.DefaultDataComponentsBoundEvent(false, false));
        long t3 = System.nanoTime();
        return "bound components for " + pending.size() + " registries; vanilla world lookup " + (t1 - t) / 1000000.0 + " ms, build " + (t2 - t1) / 1000000.0 + " ms, apply+event " + (t3 - t2) / 1000000.0 + " ms";
    }

    static String stats() {
        var cl = java.lang.management.ManagementFactory.getClassLoadingMXBean();
        var sb = new StringBuilder("classes_loaded=" + cl.getTotalLoadedClassCount() + " threads=" + java.lang.management.ManagementFactory.getThreadMXBean().getThreadCount());
        for (var p : java.lang.management.ManagementFactory.getMemoryPoolMXBeans()) sb.append(' ').append(p.getName().replace(' ', '_')).append('=').append(p.getUsage().getUsed() / 1048576).append("M");
        return sb.toString();
    }

    static String nmt() throws Exception {
        var mbs = java.lang.management.ManagementFactory.getPlatformMBeanServer();
        var name = new javax.management.ObjectName("com.sun.management:type=DiagnosticCommand");
        return (String) mbs.invoke(name, "vmNativeMemory", new Object[] {new String[] {"summary", "scale=MB"}}, new String[] {String[].class.getName()});
    }

    static String mods() {
        var sb = new StringBuilder();
        for (var m : ModList.get().getMods()) sb.append(m.getModId()).append('@').append(m.getVersion()).append(' ');
        return "mods(" + ModList.get().size() + "): " + sb;
    }

    static String registries() {
        int n = 0;
        var sb = new StringBuilder();
        for (var r : BuiltInRegistries.REGISTRY) n++;
        sb.append("builtin registries=").append(n);
        sb.append(" items=").append(BuiltInRegistries.ITEM.size());
        sb.append(" blocks=").append(BuiltInRegistries.BLOCK.size());
        sb.append(" data_component_types=").append(BuiltInRegistries.DATA_COMPONENT_TYPE.size());
        sb.append(" block_entity_types=").append(BuiltInRegistries.BLOCK_ENTITY_TYPE.size());
        sb.append(" entity_types=").append(BuiltInRegistries.ENTITY_TYPE.size());
        sb.append('\n');
        try {
            var id = Identifier.parse("horizon_java:late_sound");
            Registry.register(BuiltInRegistries.SOUND_EVENT, id, SoundEvent.createVariableRangeEvent(id));
            sb.append("late registration: ACCEPTED (not frozen!)");
        } catch (Throwable t) {
            sb.append("late registration rejected: ").append(t);
        }
        return sb.toString();
    }

    static String items(String ns) {
        var ops = access().createSerializationContext(JsonOps.INSTANCE);
        var out = new JsonObject();
        for (Item item : BuiltInRegistries.ITEM) {
            Identifier id = BuiltInRegistries.ITEM.getKey(item);
            if (!id.getNamespace().equals(ns)) continue;
            ItemStack stack = new ItemStack(item);
            var o = new JsonObject();
            o.addProperty("class", item.getClass().getName());
            o.addProperty("max_stack_size", stack.getMaxStackSize());
            o.addProperty("rarity", stack.getRarity().getSerializedName());
            var comps = new JsonObject();
            for (TypedDataComponent<?> c : stack.getComponents()) {
                var enc = c.encodeValue(ops).result();
                if (enc.isPresent()) comps.add(BuiltInRegistries.DATA_COMPONENT_TYPE.getKey(c.type()).toString(), enc.get());
            }
            o.add("components", comps);
            out.add(id.toString(), o);
        }
        return out.toString();
    }

    static String mixin(String target) throws Exception {
        var sb = new StringBuilder();
        for (Config c : Mixins.getConfigs()) {
            var m = c.getConfig();
            sb.append("config ").append(m.getName()).append(" package=").append(m.getMixinPackage()).append(" required=").append(m.isRequired())
                    .append(" targets=").append(m.getTargets()).append('\n');
        }
        if (target.isEmpty()) return sb.toString();
        Class<?> t = Class.forName(target, false, Boot.class.getClassLoader());
        sb.append("target ").append(t.getName()).append(" loaded by ").append(t.getClassLoader().getClass().getSimpleName()).append('\n');
        for (var info : Mixins.getMixinsForClass(target)) {
            sb.append("  IMixinInfo ").append(info.getClassName()).append(" -> ").append(info.getTargetClasses()).append(" priority=").append(info.getPriority()).append('\n');
        }
        List<String> merged = new ArrayList<>();
        for (Method m : t.getDeclaredMethods()) {
            for (var a : m.getDeclaredAnnotations()) {
                if (a.annotationType().getSimpleName().equals("MixinMerged")) merged.add(m.getName() + " " + a);
            }
        }
        sb.append("  methods=").append(t.getDeclaredMethods().length).append(" fields=").append(t.getDeclaredFields().length).append(" interfaces=").append(java.util.Arrays.toString(t.getInterfaces())).append('\n');
        sb.append("  methods carrying @MixinMerged: ").append(merged).append('\n');
        return sb.toString();
    }

    static boolean listening;

    static String listen() {
        if (listening) return "already";
        listening = true;
        NeoForge.EVENT_BUS.addListener(EventPriority.NORMAL, false, HzPing.class, e -> Host.onEvent("HzPing listener ran on thread " + Thread.currentThread().getName() + " payload=" + e.payload));
        NeoForge.EVENT_BUS.addListener(EventPriority.NORMAL, false, RegisterCommandsEvent.class, Boot::onCommands);
        return "listeners registered";
    }

    static void onCommands(RegisterCommandsEvent e) {
        CommandDispatcher<CommandSourceStack> d = e.getDispatcher();
        d.register(LiteralArgumentBuilder.<CommandSourceStack>literal("horizon_java_probe").executes(ctx -> 1));
        Host.onEvent("RegisterCommandsEvent listener ran; selection=" + e.getCommandSelection());
    }

    static String event() {
        var sb = new StringBuilder();
        listen();
        var ping = NeoForge.EVENT_BUS.post(new HzPing("from rust"));
        sb.append("posted HzPing: ").append(ping.payload).append('\n');
        var disp = new CommandDispatcher<CommandSourceStack>();
        var ev = NeoForge.EVENT_BUS.post(new RegisterCommandsEvent(disp, Commands.CommandSelection.DEDICATED, Commands.createValidationContext(access())));
        sb.append("posted RegisterCommandsEvent; dispatcher now has /horizon_java_probe: ").append(ev.getDispatcher().getRoot().getChild("horizon_java_probe") != null);
        return sb.toString();
    }

    static String stack(String id) {
        var ops = access().createSerializationContext(JsonOps.INSTANCE);
        Item item = BuiltInRegistries.ITEM.getValue(Identifier.parse(id));
        ItemStack s = new ItemStack(item);
        JsonElement j = ItemStack.CODEC.encodeStart(ops, s).getOrThrow();
        ItemStack back = ItemStack.CODEC.parse(ops, j).getOrThrow();
        return "ItemStack.CODEC -> " + j + "\nroundtrip equal=" + ItemStack.matches(s, back) + " name=" + s.getHoverName().getString() + " rarity=" + s.getRarity();
    }
}
