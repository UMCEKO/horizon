package dev.horizonlab.javamods.boot;

import com.mojang.renderpearl.api.buffers.GpuBuffer;
import com.mojang.renderpearl.api.buffers.GpuBufferSlice;
import java.nio.ByteBuffer;

/** A GPU buffer's bytes on the heap, with a version that every change bumps. */
public class RecordingBuffer implements GpuBuffer {
    final long id;
    private final int usage;
    private byte[] bytes;
    private int version;
    /** A draw holds the bytes it drew from: the next change copies them first. */
    private boolean shared;
    private boolean closed;
    private final RecordingDevice device;

    RecordingBuffer(RecordingDevice device, long id, int usage, byte[] bytes) {
        this.device = device;
        this.id = id;
        this.usage = usage;
        this.bytes = bytes;
    }

    public long id() {
        return id;
    }

    public int version() {
        return version;
    }

    /** The bytes as they are now; later changes don't reach the returned array. */
    byte[] snapshot() {
        shared = true;
        return bytes;
    }

    private void changing() {
        if (shared) {
            bytes = bytes.clone();
            shared = false;
        }
        version++;
    }

    void write(long offset, ByteBuffer data) {
        changing();
        data.duplicate().get(bytes, (int) offset, data.remaining());
    }

    void copyFrom(RecordingBuffer source, long sourceOffset, long offset, long length) {
        changing();
        System.arraycopy(source.bytes, (int) sourceOffset, bytes, (int) offset, (int) length);
    }

    @Override
    public long size() {
        return bytes.length;
    }

    @Override
    public @Usage int usage() {
        return usage;
    }

    @Override
    public boolean isClosed() {
        return closed;
    }

    @Override
    public GpuBufferSlice.MappedView map(long offset, long length, boolean read, boolean write) {
        if (write) changing();
        var view = ByteBuffer.wrap(bytes, (int) offset, (int) length).slice();
        return new GpuBufferSlice.MappedView(new GpuBufferSlice(this, offset, length), view, () -> {
            if (write) version++;
        });
    }

    @Override
    public void close() {
        if (closed) return;
        closed = true;
        device.closed(id);
    }
}
