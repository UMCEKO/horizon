package fixture; import net.minecraft.world.entity.*; import net.minecraft.world.entity.player.Player; import net.minecraft.world.level.Level;
        import net.neoforged.bus.api.*; import net.neoforged.neoforge.event.entity.*; import net.neoforged.neoforge.capabilities.RegisterCapabilitiesEvent;
        public class Guards {
          static void use(Object o) {} static void unknown() {}
          @SubscribeEvent public static void mek(EntityJoinLevelEvent e) {
            Level level = e.getEntity().level();
            if (!level.isClientSide() && e.getEntity() instanceof LivingEntity living && !(living instanceof Player) && level.tickRateManager() == null) { use(living); }
          }
          @SubscribeEvent public static void zombie(EntityLeaveLevelEvent e) { if (e.getEntity().getType() == EntityTypes.ZOMBIE) { use(e); } }
          @SubscribeEvent public static void reversed(EntityLeaveLevelEvent e) { if (EntityTypes.COW == e.getEntity().getType()) { use(e); } }
                    @SubscribeEvent public static void after(ProbeEvent e) { unknown(); if (e.getEntity() instanceof Player) { use(e); } }
          @SubscribeEvent public static void stops(ProbeEvent e) { if (e.getEntity() instanceof Player) { unknown(); } if (e.getEntity() instanceof LivingEntity) { use(e); } }
          public Guards(IEventBus bus) { bus.addListener((EntityJoinLevelEvent e) -> { if (e.getEntity() instanceof Player) { use(e); } }); }
          public static void caps(RegisterCapabilitiesEvent ev) {
            ev.registerEntity(Caps.CAP, EntityTypes.COW, Caps.PROVIDER);
            ev.registerEntity(Caps.CAP, pick(), Caps.PROVIDER);
          }
          static EntityType pick() { return null; }
          static void touch(Entity en, ItemHolder h) { en.setPos(1, 2, 3); en.getX(); en.frobnicate(); Entity.spawn(); }
          static class ItemHolder {}
        }
