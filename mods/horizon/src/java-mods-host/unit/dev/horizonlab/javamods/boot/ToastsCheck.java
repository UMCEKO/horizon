package dev.horizonlab.javamods.boot;

import java.util.ArrayDeque;
import net.minecraft.client.gui.Font;
import net.minecraft.client.gui.GuiGraphicsExtractor;
import net.minecraft.client.gui.components.toasts.Toast;
import net.minecraft.client.gui.components.toasts.ToastManager;

/** A mod's toast is updated from the window's slide: wanted visibility each turn, and a finished one leaves `queued` and hears `onFinishedRendering` once. */
public final class ToastsCheck {
    static final class Stub implements Toast {
        int updates;
        int finished;

        @Override
        public Visibility getWantedVisibility() {
            return updates > 0 && last >= 100 ? Visibility.HIDE : Visibility.SHOW;
        }

        long last;

        @Override
        public void update(ToastManager manager, long fullyVisibleForMs) {
            updates++;
            last = fullyVisibleForMs;
        }

        @Override
        public void extractRenderState(GuiGraphicsExtractor graphics, Font font, long fullyVisibleForMs) {}

        @Override
        public void onFinishedRendering() {
            finished++;
        }
    }

    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    public static void main(String[] args) throws Exception {
        // `Visibility` holds sound events, which register only once vanilla's bootstrap has run; its flag is all they read.
        var bootstrapped = net.minecraft.server.Bootstrap.class.getDeclaredField("isBootstrapped");
        bootstrapped.setAccessible(true);
        bootstrapped.setBoolean(null, true);
        var stub = new Stub();
        var queued = new ArrayDeque<Toast>();
        queued.add(stub);
        Toasts.added(stub);
        check(Toasts.count() == 1, "the toast is known");
        var shown = Toasts.apply(1, false, 50, false, queued, null, 1.0F);
        check(shown != null && shown.wanted() == Toast.Visibility.SHOW, "shown at 50 ms");
        var hidden = Toasts.apply(1, false, 150, false, queued, null, 1.0F);
        check(hidden != null && hidden.wanted() == Toast.Visibility.HIDE, "hides at 150 ms");
        check(stub.updates == 2, "one update per turn");
        check(Toasts.apply(1, true, 0, false, queued, null, 1.0F) == null, "a finished toast has no update");
        check(queued.isEmpty(), "finished leaves queued");
        check(stub.finished == 1, "onFinishedRendering once");
        check(Toasts.count() == 0, "forgotten");
        check(Toasts.apply(1, true, 0, false, queued, null, 1.0F) == null && stub.finished == 1, "a second finish is ignored");
        Toasts.added(new Stub());
        Toasts.cleared();
        check(Toasts.count() == 0, "clear forgets every toast");
    }
}
