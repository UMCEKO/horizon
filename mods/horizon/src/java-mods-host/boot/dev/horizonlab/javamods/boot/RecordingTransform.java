package dev.horizonlab.javamods.boot;

import net.minecraft.client.renderer.DynamicGpuData;
import org.joml.Matrix4f;
import org.joml.Vector3f;
import org.joml.Vector4f;

/** What a `DynamicGpuData.writeTransform` stands for while a world frame records: the transform itself, in a buffer with no bytes. */
final class RecordingTransform extends RecordingBuffer {
    static final DynamicGpuData.Transform IDENTITY = new DynamicGpuData.Transform(new Matrix4f(), new Vector4f(1, 1, 1, 1), new Vector3f(), new Matrix4f());

    private final DynamicGpuData.Transform transform;

    RecordingTransform(RecordingDevice device, DynamicGpuData.Transform transform) {
        super(device, device.nextId(), 0, new byte[0]);
        this.transform = new DynamicGpuData.Transform(new Matrix4f(transform.modelView()), new Vector4f(transform.colorModulator()), new Vector3f(transform.modelOffset()), new Matrix4f(transform.textureMatrix()));
    }

    DynamicGpuData.Transform transform() {
        return transform;
    }
}
