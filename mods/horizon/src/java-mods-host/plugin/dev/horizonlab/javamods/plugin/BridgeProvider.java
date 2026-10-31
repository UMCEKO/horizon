package dev.horizonlab.javamods.plugin;

import net.neoforged.neoforgespi.transformation.ClassProcessorProvider;

public final class BridgeProvider implements ClassProcessorProvider {
    @Override
    public void createProcessors(Context context, Collector collector) {
        collector.add(new BridgeProcessor());
    }
}
