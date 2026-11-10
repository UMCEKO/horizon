package dev.horizonlab.javamods.parity;

import net.neoforged.api.distmarker.Dist;
import net.neoforged.fml.common.Mod;

/** Runs the scenario named by -Dhorizon.parity.scenario on the real client; Horizon's driver is tools/mod_parity.py. */
@Mod(value = "horizon_parity", dist = Dist.CLIENT)
public final class ParityMod {
    public ParityMod() {
        String file = System.getProperty("horizon.parity.scenario");
        if (file != null) Scenario.install(java.nio.file.Path.of(file));
    }
}
