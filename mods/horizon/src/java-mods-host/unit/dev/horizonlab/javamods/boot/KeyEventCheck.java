package dev.horizonlab.javamods.boot;

import net.minecraft.client.gui.components.EditBox;
import net.minecraft.client.input.KeyEvent;
import net.minecraft.network.chat.Component;

/** A `KEY` entry's keycode reaches `EditBox`: Backspace (keycode 8) deletes, E (scancode 8, keycode 101) does not. */
public final class KeyEventCheck {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    private static EditBox box() throws Exception {
        var box = new EditBox(null, 0, 0, 100, 20, Component.empty());
        box.setValue("abc");
        var focused = net.minecraft.client.gui.components.AbstractWidget.class.getDeclaredField("focused");
        focused.setAccessible(true);
        focused.set(box, true);
        return box;
    }

    public static void main(String[] args) throws Exception {
        var box = box();
        KeyEvent backspace = ClientBridge.keyEvent("key.keyboard.backspace", 0, 8);
        check(backspace.key() == 42 && backspace.input() == 42, "the scancode stays the key");
        box.keyPressed(backspace);
        check(box.getValue().equals("ab"), "Backspace leaves ab, got " + box.getValue());
        box = box();
        KeyEvent e = ClientBridge.keyEvent("key.keyboard.e", 0, 101);
        check(e.key() == 8 && e.input() == 8, "E's scancode is 8");
        box.keyPressed(e);
        check(box.getValue().equals("abc"), "E alone deletes nothing, got " + box.getValue());
    }
}
