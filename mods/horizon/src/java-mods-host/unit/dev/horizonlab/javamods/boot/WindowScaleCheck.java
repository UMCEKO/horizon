package dev.horizonlab.javamods.boot;

import com.mojang.blaze3d.platform.Window;

/** `Window.setGuiScale` with the scale native chose (3) over a 1920x1080 framebuffer leaves vanilla's 640x360. */
public final class WindowScaleCheck {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    private static void set(Window window, String field, int value) throws Exception {
        var f = Window.class.getDeclaredField(field);
        f.setAccessible(true);
        f.setInt(window, value);
    }

    public static void main(String[] args) throws Exception {
        var window = Sprites.unallocated(Window.class);
        set(window, "framebufferWidth", 1920);
        set(window, "framebufferHeight", 1080);
        window.setGuiScale(3);
        check(window.getGuiScaledWidth() == 640, "scaled width " + window.getGuiScaledWidth());
        check(window.getGuiScaledHeight() == 360, "scaled height " + window.getGuiScaledHeight());
        check(window.getGuiScale() == 3, "scale " + window.getGuiScale());
    }
}
