package dev.horizonlab.javamods.parity;

import net.neoforged.neoforgespi.transformation.ClassProcessorProvider;

public final class ParityProvider implements ClassProcessorProvider {
    @Override
    public void createProcessors(Context context, Collector collector) {
        collector.add(new MergedProcessor());
    }
}
