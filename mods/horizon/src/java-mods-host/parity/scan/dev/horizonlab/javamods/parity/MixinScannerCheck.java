package dev.horizonlab.javamods.parity;

import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Map;
import javax.tools.ToolProvider;

/** Compiles stub mixin annotations and a mixin class, scans it, and compares the lines; run by tests/test_mod_parity.py. */
public final class MixinScannerCheck {
    private static final Map<String, String> SOURCES = Map.of(
        "org/spongepowered/asm/mixin/Mixin.java", "package org.spongepowered.asm.mixin; public @interface Mixin { Class<?>[] value() default {}; String[] targets() default {}; }",
        "org/spongepowered/asm/mixin/injection/At.java", "package org.spongepowered.asm.mixin.injection; public @interface At { String value(); String target() default \"\"; }",
        "org/spongepowered/asm/mixin/injection/Inject.java", "package org.spongepowered.asm.mixin.injection; public @interface Inject { String[] method() default {}; At[] at() default {}; }",
        "org/spongepowered/asm/mixin/injection/Redirect.java", "package org.spongepowered.asm.mixin.injection; public @interface Redirect { String[] method() default {}; At at(); }",
        "org/spongepowered/asm/mixin/gen/Accessor.java", "package org.spongepowered.asm.mixin.gen; public @interface Accessor { String value() default \"\"; }",
        "org/spongepowered/asm/mixin/Overwrite.java", "package org.spongepowered.asm.mixin; public @interface Overwrite {}",
        "t/Sample.java", "package t; import org.spongepowered.asm.mixin.*; import org.spongepowered.asm.mixin.injection.*; import org.spongepowered.asm.mixin.gen.Accessor;"
            + " @Mixin(value = {java.util.List.class}, targets = \"a/b/Hidden\") abstract class Sample {"
            + " @Inject(method = {\"add\", \"get\"}, at = {@At(\"HEAD\"), @At(value = \"INVOKE\", target = \"Lx;y()V\")}) void first() {}"
            + " @Redirect(method = \"size\", at = @At(value = \"FIELD\", target = \"Lx;f\")) int second() { return 0; }"
            + " @Accessor(\"size\") abstract int size();"
            + " @Overwrite int hashCode2() { return 1; }"
            + " void plain() {} }");

    public static void main(String[] args) throws Exception {
        Path dir = Files.createTempDirectory("scanner-check");
        var files = new java.util.ArrayList<String>();
        for (var entry : SOURCES.entrySet()) {
            Path file = dir.resolve("src").resolve(entry.getKey());
            Files.createDirectories(file.getParent());
            Files.writeString(file, entry.getValue());
            files.add(file.toString());
        }
        var compiler = ToolProvider.getSystemJavaCompiler();
        files.addAll(0, List.of("-d", dir.resolve("out").toString(), "-proc:none"));
        if (compiler.run(null, null, null, files.toArray(new String[0])) != 0) throw new AssertionError("stubs did not compile");
        String scanned = MixinScanner.scan("m", "c.json", "client", Files.readAllBytes(dir.resolve("out/t/Sample.class")));
        var expected = List.of(
            "m\tc.json\tclient\tt.Sample\tfirst\tinject\tjava.util.List,a.b.Hidden\tadd,get\tHEAD|INVOKE:Lx;y()V",
            "m\tc.json\tclient\tt.Sample\tsecond\tredirect\tjava.util.List,a.b.Hidden\tsize\tFIELD:Lx;f",
            "m\tc.json\tclient\tt.Sample\tsize\taccessor\tjava.util.List,a.b.Hidden\tsize\t",
            "m\tc.json\tclient\tt.Sample\thashCode2\toverwrite\tjava.util.List,a.b.Hidden\thashCode2\t");
        if (!List.of(scanned.split("\n")).equals(expected)) throw new AssertionError("scanned:\n" + scanned);
        System.out.println("MixinScannerCheck ok");
    }
}
