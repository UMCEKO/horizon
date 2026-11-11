package dev.horizonlab.javamods.boot;

/** Whether the code that made a call is a mod's. */
public final class Callers {
    private Callers() {}

    private static final String[] PLUMBING = {"java.", "jdk.", "sun.", "org.spongepowered.", "com.llamalad7."};
    private static final String[] HOST = {"net.minecraft.", "com.mojang.", "dev.horizonlab."};

    /** The first frame outside this class, the skipped prefixes and the JDK decides: a mod's when it is none of the game's or Horizon's. */
    public static boolean byMod(String... skipPrefixes) {
        return StackWalker.getInstance().walk(frames -> frames.map(StackWalker.StackFrame::getClassName)
                .filter(name -> !Callers.class.getName().equals(name) && !starts(name, skipPrefixes) && !starts(name, PLUMBING))
                .findFirst().map(name -> !starts(name, HOST)).orElse(false));
    }

    private static boolean starts(String name, String[] prefixes) {
        for (String prefix : prefixes) if (name.startsWith(prefix)) return true;
        return false;
    }
}
