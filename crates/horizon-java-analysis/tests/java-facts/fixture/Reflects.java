package fixture; public class Reflects { static Object peek(net.minecraft.world.entity.Entity e) throws Exception { e.getX(); return e.getClass().getDeclaredMethod("x").invoke(e); } }
