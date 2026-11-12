package net.minecraft.world.rt;
        public class Target {
          public void head(int a) {}
          public void headLocal(int a) {}
          public int tail(int a) { if (a > 0) return 1; return 2; }
          public void wrapped() { helper(); }
          public void mixed() { helper(); }
          public void varArg(int a) {}
          public void varNo(int a) {}
          public void run() { Runnable r = () -> helper(); r.run(); }
          public void hook() {}
          public void body() { bridged(); net.minecraft.world.entity.Entity e = null; e.setPos(1, 2, 3); pure(); head(1); }
          public void bridged() {}
          public void pure() { pure2(); }
          public void pure2() {}
          public void weak() {}
          public void helper() {}
          public static void util() {}
        }