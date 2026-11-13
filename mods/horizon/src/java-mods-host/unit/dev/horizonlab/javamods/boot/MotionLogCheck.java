package dev.horizonlab.javamods.boot;

import java.io.ByteArrayInputStream;
import java.io.DataInputStream;
import java.net.URL;
import java.net.URLClassLoader;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Map;
import java.util.function.Consumer;
import javax.tools.ToolProvider;

/** The motion log records only a mod's writes to the watched player, outside Horizon's own fills, in its documented layout. */
public final class MotionLogCheck {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    private static final String LOG = "dev.horizonlab.javamods.boot.MotionLog";

    /** Callers compiled at run time, so their frames carry the class names the walk tells apart. */
    private static final Map<String, String> SOURCES = Map.of(
            "mod/Fake.java", "package mod; public class Fake implements java.util.function.Consumer<Object> { public void accept(Object e) { " + LOG + ".velocity(e, 0, 0.5, 0); } }",
            "net/minecraft/world/entity/HorizonCheckFrame.java", "package net.minecraft.world.entity; public class HorizonCheckFrame { public static void push(Object e) { " + LOG + ".velocity(e, 1, 2, 3); } }",
            "mod/Every.java", "package mod; public class Every implements java.util.function.Consumer<Object> { public void accept(Object e) { " + LOG + ".position(e, 1.5, 64, -2.25); "
                    + LOG + ".yaw(e, 90); " + LOG + ".pitch(e, -15.5F); " + LOG + ".ground(e, true); " + LOG + ".fallReset(e); " + LOG + ".sprint(e, true); " + LOG + ".abilities(e, true, true, 0.05F); } }",
            "mod/ViaEntity.java", "package mod; public class ViaEntity implements java.util.function.Consumer<Object> { public void accept(Object e) { net.minecraft.world.entity.HorizonCheckFrame.push(e); } }",
            "net/minecraft/client/multiplayer/HorizonCheckHandler.java", "package net.minecraft.client.multiplayer; public class HorizonCheckHandler implements java.util.function.Consumer<Object> { public void accept(Object e) { " + LOG + ".velocity(e, 4, 5, 6); } }");

    @SuppressWarnings("unchecked")
    private static Consumer<Object> caller(ClassLoader loader, String name) throws Exception {
        return (Consumer<Object>) loader.loadClass(name).getConstructor().newInstance();
    }

    private static int entries() throws Exception {
        return new DataInputStream(new ByteArrayInputStream(MotionLog.take())).readInt();
    }

    public static void main(String[] args) throws Exception {
        Path root = Files.createTempDirectory("motion-log-check");
        var files = new ArrayList<String>();
        for (var e : SOURCES.entrySet()) {
            Path file = root.resolve("src").resolve(e.getKey());
            Files.createDirectories(file.getParent());
            Files.writeString(file, e.getValue());
            files.add(file.toString());
        }
        Path classes = root.resolve("classes");
        var javac = new ArrayList<>(java.util.List.of("-proc:none", "-nowarn", "-cp", System.getProperty("java.class.path"), "-d", classes.toString()));
        javac.addAll(files);
        check(ToolProvider.getSystemJavaCompiler().run(null, null, null, javac.toArray(String[]::new)) == 0, "the callers compile");
        var loader = new URLClassLoader(new URL[] {classes.toUri().toURL()}, MotionLogCheck.class.getClassLoader());

        MotionLog.modClass = c -> c.getName().startsWith("mod.");
        Object player = new Object();
        MotionLog.watching = player;
        MotionLog.take();

        caller(loader, "mod.Fake").accept(player);
        check(entries() == 1, "a mod's call is recorded");
        caller(loader, "mod.ViaEntity").accept(player);
        check(entries() == 1, "a mod's call through vanilla's entity code is recorded");
        caller(loader, "net.minecraft.client.multiplayer.HorizonCheckHandler").accept(player);
        check(entries() == 0, "vanilla's client code is not a mod");
        MotionLog.filling = true;
        caller(loader, "mod.Fake").accept(player);
        MotionLog.filling = false;
        check(entries() == 0, "Horizon's own fill is not recorded");
        caller(loader, "mod.Fake").accept(new Object());
        check(entries() == 0, "another entity is not recorded");
        MotionLog.oldPos(player);
        check(entries() == 1, "the old position is marked whoever calls");

        var fake = caller(loader, "mod.Fake");
        fake.accept(player);
        MotionLog.oldPos(player);
        MotionLog.watching = null;
        fake.accept(player);
        MotionLog.watching = player;
        var mods = caller(loader, "mod.ViaEntity");
        mods.accept(player);
        var in = new DataInputStream(new ByteArrayInputStream(MotionLog.take()));
        check(in.readInt() == 3, "three entries");
        check(in.readByte() == MotionLog.VELOCITY && in.readDouble() == 0 && in.readDouble() == 0.5 && in.readDouble() == 0, "a velocity reads back");
        check(in.readByte() == MotionLog.OLD_POS, "the old position has no body");
        check(in.readByte() == MotionLog.VELOCITY && in.readDouble() == 1 && in.readDouble() == 2 && in.readDouble() == 3, "the second velocity reads back");
        check(in.read() == -1, "nothing after the entries");
        check(entries() == 0, "a take empties the log");

        caller(loader, "mod.Every").accept(player);
        in = new DataInputStream(new ByteArrayInputStream(MotionLog.take()));
        check(in.readInt() == 7, "seven entries");
        check(in.readByte() == MotionLog.POSITION && in.readDouble() == 1.5 && in.readDouble() == 64 && in.readDouble() == -2.25, "a position reads back");
        check(in.readByte() == MotionLog.YAW && in.readFloat() == 90F, "a yaw reads back");
        check(in.readByte() == MotionLog.PITCH && in.readFloat() == -15.5F, "a pitch reads back");
        check(in.readByte() == MotionLog.GROUND && in.readBoolean(), "the ground reads back");
        check(in.readByte() == MotionLog.FALL_RESET, "a fall reset has no body");
        check(in.readByte() == MotionLog.SPRINT && in.readBoolean(), "sprinting reads back");
        check(in.readByte() == MotionLog.ABILITIES && in.readBoolean() && in.readBoolean() && in.readFloat() == 0.05F, "the abilities read back");
        check(in.read() == -1, "nothing after the abilities");
        MotionLog.watching = null;
        System.out.println("MotionLogCheck: ok");
    }
}
