package dev.horizonlab.javamods.boot.mixin;

import java.util.Deque;
import net.minecraft.client.gui.components.toasts.Toast;
import net.minecraft.client.gui.components.toasts.ToastManager;
import org.spongepowered.asm.mixin.Mixin;
import org.spongepowered.asm.mixin.gen.Accessor;

@Mixin(ToastManager.class)
public interface ToastQueueAccess {
    @Accessor("queued")
    Deque<Toast> horizon$queued();
}
