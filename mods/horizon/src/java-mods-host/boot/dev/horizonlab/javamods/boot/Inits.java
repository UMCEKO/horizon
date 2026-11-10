package dev.horizonlab.javamods.boot;

import java.lang.reflect.InvocationTargetException;
import java.lang.reflect.Method;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Comparator;
import java.util.List;

/** Runs what mods added to the constructors of a view, which is built without them; `ViewInits` makes the methods. */
final class Inits {
    private static final String PREFIX = "horizon$init$";

    private Inits() {}

    private static int order(Method method) {
        return Integer.parseInt(method.getName().substring(PREFIX.length()));
    }

    private static Object argument(Class<?> type, Object[] have) {
        for (Object value : have) {
            if (type.isInstance(value)) return value;
        }
        if (!type.isPrimitive()) return null;
        if (type == boolean.class) return false;
        if (type == long.class) return 0L;
        if (type == float.class) return 0f;
        if (type == double.class) return 0d;
        if (type == char.class) return (char) 0;
        if (type == byte.class) return (byte) 0;
        if (type == short.class) return (short) 0;
        return 0;
    }

    /** Superclasses first, as constructors run; a failing one is reported and the rest still run. */
    static void run(Object view, Object... have) {
        var chain = new ArrayList<Class<?>>();
        for (Class<?> c = view.getClass(); c != null; c = c.getSuperclass()) chain.add(0, c);
        for (Class<?> c : chain) {
            List<Method> methods = new ArrayList<>(Arrays.stream(c.getDeclaredMethods()).filter(m -> m.getName().startsWith(PREFIX)).toList());
            methods.sort(Comparator.comparingInt(Inits::order));
            for (Method method : methods) {
                Object[] args = Arrays.stream(method.getParameterTypes()).map(type -> argument(type, have)).toArray();
                try {
                    method.setAccessible(true);
                    method.invoke(view, args);
                } catch (InvocationTargetException e) {
                    System.err.println("Horizon: a mod's constructor code on " + c.getName() + " failed without its arguments: " + e.getCause());
                } catch (ReflectiveOperationException e) {
                    throw new IllegalStateException(e);
                }
            }
        }
    }
}
