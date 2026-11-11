package dev.horizonlab.javamods.parity;

import java.nio.file.Files;
import java.nio.file.Path;

public final class MixinScannerEmptyCheck {
    public static void main(String[] args) throws Exception {
        Path manifest = Files.createTempFile(Path.of("."), "empty-mixins-", ".tsv");
        try {
            Files.writeString(manifest, "\n");
            MixinScanner.main(new String[] {manifest.toString()});
        } finally {
            Files.delete(manifest);
        }
        System.out.println("MixinScannerEmptyCheck ok");
    }
}
