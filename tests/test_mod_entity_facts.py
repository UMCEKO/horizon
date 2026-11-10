import json
import subprocess
import tempfile
import unittest
from unittest import mock
from argparse import Namespace
from pathlib import Path

from tools import mod_entity_facts as ef
from tools import mod_replaces as mr
from tests.test_mod_replaces import classes_under, javac, jar

ENTITY = 'net.minecraft.world.entity.'
EVENTS = 'net.neoforged.neoforge.event.entity.'
SOURCES = {
    'net/minecraft/world/level/Level.java': 'package net.minecraft.world.level; public class Level { public boolean isClientSide() { return false; } public Object tickRateManager() { return null; } }',
    'net/minecraft/world/entity/EntityType.java': 'package net.minecraft.world.entity; public class EntityType {}',
    'net/minecraft/world/entity/EntityTypeIds.java': ('package net.minecraft.world.entity; public class EntityTypeIds { public static final Object ZOMBIE = create("zombie"); '
                                                    'public static final Object COW = create("cow"); static Object create(String s) { return s; } }'),
    'net/minecraft/world/entity/EntityTypes.java': ('package net.minecraft.world.entity; public class EntityTypes { public static final EntityType ZOMBIE = register(EntityTypeIds.ZOMBIE); '
                                                  'public static final EntityType COW = register(EntityTypeIds.COW); static EntityType register(Object k) { return null; } }'),
    'net/minecraft/world/entity/Entity.java': ('package net.minecraft.world.entity; import net.minecraft.world.level.Level; public class Entity { public Level level() { return null; } '
                                              'public EntityType getType() { return null; } public double getX() { return 0; } public void setPos(double x, double y, double z) {} '
                                              'public void tick() {} public void frobnicate() {} public void wobble() {} public void setAirSupply(int a) {} public static void spawn() {} }'),
    'net/minecraft/world/entity/LivingEntity.java': 'package net.minecraft.world.entity; public class LivingEntity extends Entity { public float getHealth() { return 0; } }',
    'net/minecraft/world/entity/player/Player.java': 'package net.minecraft.world.entity.player; public class Player extends net.minecraft.world.entity.LivingEntity { public void swing() {} }',
    'net/minecraft/world/entity/item/ItemEntity.java': ('package net.minecraft.world.entity.item; public class ItemEntity extends net.minecraft.world.entity.Entity { '
                                                       'public void tick() {} public boolean hurt(float a) { return false; } public boolean tryPick(int a) { return false; } }'),
    'net/neoforged/bus/api/Event.java': 'package net.neoforged.bus.api; public class Event {}',
    'net/neoforged/bus/api/SubscribeEvent.java': 'package net.neoforged.bus.api; import java.lang.annotation.*; @Retention(RetentionPolicy.RUNTIME) public @interface SubscribeEvent {}',
    'net/neoforged/bus/api/IEventBus.java': 'package net.neoforged.bus.api; import java.util.function.Consumer; public interface IEventBus { <T extends Event> void addListener(Consumer<T> c); '
                                                    'default <T extends Event> T post(T e) { new net.minecraft.world.entity.Entity().setPos(0, 0, 0); return e; } }',
    'net/neoforged/neoforgespi/transformation/ClassProcessorProvider.java': 'package net.neoforged.neoforgespi.transformation; public interface ClassProcessorProvider {}',
    'net/neoforged/neoforge/event/entity/EntityEvent.java': ('package net.neoforged.neoforge.event.entity; public class EntityEvent extends net.neoforged.bus.api.Event { '
                                                            'public net.minecraft.world.entity.Entity getEntity() { return null; } }'),
    'net/neoforged/neoforge/event/entity/EntityJoinLevelEvent.java': 'package net.neoforged.neoforge.event.entity; public class EntityJoinLevelEvent extends EntityEvent {}',
    'net/neoforged/neoforge/event/entity/EntityLeaveLevelEvent.java': 'package net.neoforged.neoforge.event.entity; public class EntityLeaveLevelEvent extends EntityEvent {}',
    'net/neoforged/neoforge/event/entity/ProbeEvent.java': 'package net.neoforged.neoforge.event.entity; public class ProbeEvent extends EntityEvent {}',
    'net/neoforged/neoforge/event/entity/ChildEvent.java': 'package net.neoforged.neoforge.event.entity; public class ChildEvent extends ProbeEvent {}',
    'net/neoforged/neoforge/capabilities/RegisterCapabilitiesEvent.java': ('package net.neoforged.neoforge.capabilities; public class RegisterCapabilitiesEvent { '
                                                                          'public void registerEntity(Object cap, net.minecraft.world.entity.EntityType type, Object provider) {} }'),
    'org/spongepowered/asm/mixin/Mixin.java': 'package org.spongepowered.asm.mixin; public @interface Mixin { Class<?>[] value(); }',
    'org/spongepowered/asm/mixin/injection/At.java': 'package org.spongepowered.asm.mixin.injection; public @interface At { String value(); }',
    'org/spongepowered/asm/mixin/injection/Inject.java': ('package org.spongepowered.asm.mixin.injection; import java.lang.annotation.*; @Retention(RetentionPolicy.RUNTIME) '
                                                          'public @interface Inject { String[] method(); At[] at(); boolean cancellable() default false; }'),
    'org/spongepowered/asm/mixin/injection/callback/CallbackInfo.java': 'package org.spongepowered.asm.mixin.injection.callback; public class CallbackInfo {}',
    'org/spongepowered/asm/mixin/injection/callback/CallbackInfoReturnable.java': ('package org.spongepowered.asm.mixin.injection.callback; '
                                                                                 'public class CallbackInfoReturnable<R> extends CallbackInfo {}'),
    'net/neoforged/neoforge/common/TestHooks.java': ('package net.neoforged.neoforge.common; import net.minecraft.world.entity.Entity; import net.neoforged.bus.api.IEventBus; '
                                                    'import net.neoforged.neoforge.event.entity.*; public class TestHooks { '
                                                    'public static void direct(Entity e) { e.setAirSupply(3); e.getX(); } '
                                                    'public static void outer(Entity e) { mid(e); } static void mid(Entity e) { inner(e); } static void inner(Entity e) { e.setPos(1, 2, 3); } '
                                                    'public static void posts(Entity e, IEventBus bus) { bus.post(new ProbeEvent()); new Object(); } '
                                                    'public static void lambda(Entity e) { Runnable r = () -> e.setAirSupply(1); r.run(); } '
                                                    'public static void bad(Entity e) { e.wobble(); } }'),
    'fixture/Caps.java': 'package fixture; public class Caps { public static final Object CAP = new Object(); public static final Object PROVIDER = new Object(); }',
    'fixture/Guards.java': f'''package fixture; import net.minecraft.world.entity.*; import net.minecraft.world.entity.player.Player; import net.minecraft.world.level.Level;
        import net.neoforged.bus.api.*; import {EVENTS}*; import net.neoforged.neoforge.capabilities.RegisterCapabilitiesEvent;
        public class Guards {{
          static void use(Object o) {{}} static void unknown() {{}}
          @SubscribeEvent public static void mek(EntityJoinLevelEvent e) {{
            Level level = e.getEntity().level();
            if (!level.isClientSide() && e.getEntity() instanceof LivingEntity living && !(living instanceof Player) && level.tickRateManager() == null) {{ use(living); }}
          }}
          @SubscribeEvent public static void zombie(EntityLeaveLevelEvent e) {{ if (e.getEntity().getType() == EntityTypes.ZOMBIE) {{ use(e); }} }}
          @SubscribeEvent public static void reversed(EntityLeaveLevelEvent e) {{ if (EntityTypes.COW == e.getEntity().getType()) {{ use(e); }} }}
                    @SubscribeEvent public static void after(ProbeEvent e) {{ unknown(); if (e.getEntity() instanceof Player) {{ use(e); }} }}
          @SubscribeEvent public static void stops(ProbeEvent e) {{ if (e.getEntity() instanceof Player) {{ unknown(); }} if (e.getEntity() instanceof LivingEntity) {{ use(e); }} }}
          public Guards(IEventBus bus) {{ bus.addListener((EntityJoinLevelEvent e) -> {{ if (e.getEntity() instanceof Player) {{ use(e); }} }}); }}
          public static void caps(RegisterCapabilitiesEvent ev) {{
            ev.registerEntity(Caps.CAP, EntityTypes.COW, Caps.PROVIDER);
            ev.registerEntity(Caps.CAP, pick(), Caps.PROVIDER);
          }}
          static EntityType pick() {{ return null; }}
          static void touch(Entity en, ItemHolder h) {{ en.setPos(1, 2, 3); en.getX(); en.frobnicate(); Entity.spawn(); }}
          static class ItemHolder {{}}
        }}''',
    'fixture/Zom.java': 'package fixture; public class Zom extends net.minecraft.world.entity.Entity { public void tick() {} public void own() { setPos(0, 0, 0); } }',
    'fixture/Reflects.java': ('package fixture; public class Reflects { static Object peek(net.minecraft.world.entity.Entity e) throws Exception { '
                              'e.getX(); return e.getClass().getDeclaredMethod("x").invoke(e); } }'),
    'fixture/mixin/ItemMixin.java': '''package fixture.mixin; import org.spongepowered.asm.mixin.*; import org.spongepowered.asm.mixin.injection.*; import org.spongepowered.asm.mixin.injection.callback.*;
        @Mixin(net.minecraft.world.entity.item.ItemEntity.class) public class ItemMixin {
          @Inject(method = "tick", at = @At("HEAD"), cancellable = true) void head(CallbackInfo ci) {}
          @Inject(method = "tick", at = @At("RETURN")) void ret(CallbackInfo ci) {}
          @Inject(method = "tick", at = @At("HEAD")) void captured(CallbackInfo ci, int local) {}
          @Inject(method = "hurt", at = @At("INVOKE")) void middle(float a, CallbackInfoReturnable<Boolean> ci) {}
          @Inject(method = "tryPick", at = @At("HEAD"), cancellable = true) void pick(int a, CallbackInfoReturnable<Boolean> ci) {}
        }''',
}
LEDGER = {'methods': {}, 'fns': []}
ITEM = 'net.minecraft.world.entity.item.ItemEntity'


def mixin(handler, method, at, kind='inject', config='fix.mixins.json', side='common'):
    return {'mod': 'fix', 'config': config, 'mixin': 'fixture.mixin.ItemMixin', 'handler': handler, 'kind': kind, 'target_class': [ITEM],
            'target_method': [method], 'at': [at], 'side': side, 'access': False}


@unittest.skipUnless(javac(), 'needs javac')
class Facts(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        root = Path(cls.tmp.name)
        src, out = root / 'src', root / 'classes'
        for name, text in SOURCES.items():
            (src / name).parent.mkdir(parents=True, exist_ok=True)
            (src / name).write_text(text)
        subprocess.run([javac(), '-proc:none', '-nowarn', '-d', str(out), *map(str, src.rglob('*.java'))], check=True, capture_output=True)
        install = root / 'install'
        libs = install / 'libraries' / 'net' / 'neoforged'
        jar(libs / 'minecraft-client-patched' / '1' / 'minecraft-client-patched-1.jar', classes_under(out, 'net/minecraft/'))
        jar(libs / 'minecraft-server-patched' / '1' / 'minecraft-server-patched-1.jar', {'placeholder.txt': b''})
        jar(libs / 'neoforge' / '1' / 'neoforge-1-universal.jar', classes_under(out, 'net/neoforged/neoforge/'))
        jar(libs / 'fancymodloader' / 'loader' / '1' / 'loader-1.jar', classes_under(out, 'net/neoforged/fml/', 'net/neoforged/neoforgespi/'))
        jar(libs / 'bus' / '1' / 'bus-1.jar', classes_under(out, 'net/neoforged/bus/'))
        files = classes_under(out, 'fixture/')
        files['META-INF/neoforge.mods.toml'] = '[[mods]]\nmodId = "fix"\n[[mixins]]\nconfig = "fix.mixins.json"\n[[mixins]]\nconfig = "cond.mixins.json"\n'
        files['fix.mixins.json'] = '{"package": "fixture.mixin", "mixins": []}'
        files['cond.mixins.json'] = '{"package": "fixture.mixin", "plugin": "fixture.Plugin", "mixins": []}'
        jar(install / 'mods' / 'fix.jar', files)
        jar(install / 'mods' / 'plain.jar', {'META-INF/neoforge.mods.toml': '[[mods]]\nmodId = "plain"\n',
                                             'META-INF/services/net.neoforged.neoforgespi.transformation.ClassProcessorProvider': 'x.Provider\n'})
        cls.install, cls.root = install, root
        cls.entries = [mixin('head', 'tick', 'HEAD'), mixin('ret', 'tick', 'RETURN'), mixin('captured', 'tick', 'HEAD'), mixin('middle', 'hurt', 'INVOKE', config='cond.mixins.json'),
                       mixin('pick', 'tryPick', 'HEAD', side='client')]
        cls.result = mr.analyse(install, root / 'out', LEDGER, entries=cls.entries, host=[], log=lambda *_: None)
        cls.facts = cls.result['entity_facts']

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def guards(self, event, method):
        event = f'net/neoforged/neoforge/event/entity/{event}'
        return next(l['guards'] for l in self.facts['events'][event]['listeners'] if l['method'] == method)


class Walker(Facts):
    def test_a_mekanism_shaped_prefix_gives_side_require_and_exclude_and_stops_at_the_unknown_call(self):
        self.assertEqual(self.guards('EntityJoinLevelEvent', 'mek'), {'side': 'server', 'require': ['net/minecraft/world/entity/LivingEntity'],
                                                                      'exclude': ['net/minecraft/world/entity/player/Player'], 'types': None})

    def test_comparing_the_type_to_a_vanilla_static_gives_its_registry_id_in_either_order(self):
        self.assertEqual(self.guards('EntityLeaveLevelEvent', 'zombie')['types'], ['minecraft:zombie'])
        self.assertEqual(self.guards('EntityLeaveLevelEvent', 'reversed')['types'], ['minecraft:cow'])

    def test_a_guard_after_an_unknown_call_is_not_taken(self):
        self.assertEqual(self.guards('ProbeEvent', 'after'), ef.ALL_GUARD)

    def test_a_branch_to_code_that_is_not_an_exit_stops_the_walk(self):
        self.assertEqual(self.guards('ProbeEvent', 'stops'), ef.ALL_GUARD)

    def test_a_lambda_listener_takes_the_guards_of_its_implementation(self):
        lam = next(l for l in self.facts['events']['net/neoforged/neoforge/event/entity/EntityJoinLevelEvent']['listeners'] if l['method'] == '<init>')
        self.assertEqual(lam['guards']['require'], ['net/minecraft/world/entity/player/Player'])

    def test_an_event_filter_is_all_when_any_listener_is(self):
        self.assertEqual(self.facts['events']['net/neoforged/neoforge/event/entity/ProbeEvent']['subjects'], 'all')
        join = self.facts['events']['net/neoforged/neoforge/event/entity/EntityJoinLevelEvent']['subjects']
        self.assertEqual([g['side'] for g in join], ['server', 'both'])

    def test_a_listener_of_an_event_also_hears_its_subclasses(self):
        child = self.facts['events']['net/neoforged/neoforge/event/entity/ChildEvent']
        self.assertEqual(sorted(l['method'] for l in child['listeners']), ['after', 'stops'])
        self.assertEqual(child['subjects'], 'all')

    def test_type_ids_come_from_the_vanilla_clinit(self):
        hier = mr.Hierarchy({})
        hier.jars = mr.library_jars(self.install / 'libraries')
        self.assertEqual(ef.type_ids(hier), {'ZOMBIE': 'minecraft:zombie', 'COW': 'minecraft:cow'})


class Groups(Facts):
    def test_calls_give_groups_per_receiver_and_writers_with_their_mods(self):
        self.assertIn('position', self.facts['reads']['net/minecraft/world/entity/Entity'])
        self.assertEqual([(w['key'], w['group'], w['mods']) for w in self.facts['writes'] if w['receiver'] == 'net/minecraft/world/entity/Entity'],
                         [('net/minecraft/world/entity/Entity#setPos(DDD)V', 'position', ['fix'])])

    def test_a_mods_own_method_and_a_static_are_not_entity_state(self):
        self.assertFalse(any('own' in w['key'] or 'spawn' in w['key'] for w in self.facts['writes']))
        self.assertEqual(self.facts['unclassified'], ['net/minecraft/world/entity/Entity#frobnicate (fix)'])

    def test_an_unmapped_method_fails_the_tool_with_its_name(self):
        ledger = Path(self.root) / 'ledger.json'
        ledger.write_text(json.dumps(LEDGER))
        mixins = Path(self.root) / 'mixins.json'
        mixins.write_text(json.dumps(self.entries))
        with self.assertRaises(SystemExit) as caught:
            mr.run(Namespace(ledger=str(ledger), install=str(self.install), out=str(Path(self.root) / 'run-out'), cache=None, mixins=str(mixins)))
        self.assertIn('frobnicate', str(caught.exception))
        self.assertTrue((Path(self.root) / 'run-out' / 'entity-facts.json').is_file())
        self.assertNotIn('entity_facts', json.loads((Path(self.root) / 'run-out' / 'replaces.json').read_text()))

    def test_reflection_in_a_class_that_names_entities_is_listed_per_mod(self):
        self.assertEqual(self.facts['reflective_entity_access'], {'fix': 1})


class Routes(Facts):
    def edge(self):
        return self.facts['routes']['net/minecraft/world/entity/item/ItemEntity']

    def test_head_and_return_injectors_without_locals_are_edges_with_their_cancellable_flag(self):
        tick = self.edge()['edge']['net/minecraft/world/entity/item/ItemEntity#tick()V']
        self.assertEqual([(h['handler'], h['at'], h['cancellable'], h['conditional']) for h in tick], [('head', 'HEAD', True, False), ('ret', 'RETURN', False, False)])
        pick = self.edge()['edge']['net/minecraft/world/entity/item/ItemEntity#tryPick(I)Z']
        self.assertEqual([(h['handler'], h['cancellable']) for h in pick], [('pick', True)])

    def test_a_captured_local_or_another_position_makes_a_body(self):
        self.assertEqual(self.edge()['body'], ['net/minecraft/world/entity/item/ItemEntity#hurt(F)Z', 'net/minecraft/world/entity/item/ItemEntity#tick()V'])

    def test_body_handlers_of_a_config_plugin_mixin_are_listed_for_boot_to_check(self):
        item = self.edge()
        self.assertEqual(item['body_conditional'], {'net/minecraft/world/entity/item/ItemEntity#hurt(F)Z': [{'mixin': 'fixture.mixin.ItemMixin', 'handler': 'middle', 'conditional': True, 'side': 'common'}]})

    def test_a_handler_carries_the_side_of_its_mixin(self):
        pick = self.edge()['edge']['net/minecraft/world/entity/item/ItemEntity#tryPick(I)Z']
        self.assertEqual([(h['handler'], h['side']) for h in pick], [('pick', 'client')])
        tick = self.edge()['edge']['net/minecraft/world/entity/item/ItemEntity#tick()V']
        self.assertEqual({h['side'] for h in tick}, {'common'})

    def test_own_type_overrides_are_keyed_by_the_mod_class(self):
        self.assertEqual(self.facts['overrides'], {'fixture/Zom': ['net/minecraft/world/entity/Entity#tick()V']})


class Capabilities(Facts):
    def test_a_registration_with_a_vanilla_static_goes_to_the_type_and_the_rest_is_left_for_boot(self):
        self.assertEqual(self.facts['caps'], {'minecraft:cow': ['fixture/Caps.CAP']})
        self.assertEqual(self.facts['caps_unresolved'], {'fix': 1})


class Opaque(Facts):
    def test_a_transformation_service_is_listed_with_its_mod(self):
        self.assertEqual(len(self.facts['opaque']), 1)
        self.assertTrue(self.facts['opaque'][0].startswith('plain: service '))


HOOK = 'net/neoforged/neoforge/common/TestHooks#%s(Lnet/minecraft/world/entity/Entity;%s)V'


class Hooks(Facts):
    def closure(self, name, extra='', **kw):
        jars = mr.library_jars(self.install / 'libraries')
        hier = mr.Hierarchy(mr.build_library_index(jars))
        hier.jars = jars
        return ef.hook_closure(hier, HOOK % (name, extra), **kw)

    def keys(self, name):
        return [w['key'] for w in self.closure(name)['writes']]

    def test_a_hook_calling_set_air_supply_lists_the_writer(self):
        got = self.closure('direct')
        self.assertEqual(got['writes'], [{'key': 'net/minecraft/world/entity/Entity#setAirSupply(I)V', 'receiver': 'net/minecraft/world/entity/Entity', 'group': 'air'}])
        self.assertEqual(got['reads'], ['position', 'air'])

    def test_a_hook_reaching_a_helper_follows_it(self):
        self.assertEqual(self.keys('outer'), ['net/minecraft/world/entity/Entity#setPos(DDD)V'])

    def test_a_lambda_in_a_hook_is_followed(self):
        self.assertEqual(self.keys('lambda'), ['net/minecraft/world/entity/Entity#setAirSupply(I)V'])

    def test_the_walk_stops_at_post(self):
        got = self.closure('posts', 'Lnet/neoforged/bus/api/IEventBus;')
        self.assertEqual(got['writes'], [])

    def test_the_events_a_hook_constructs_are_listed(self):
        got = self.closure('posts', 'Lnet/neoforged/bus/api/IEventBus;')
        self.assertEqual(got['events'], ['net/neoforged/neoforge/event/entity/ProbeEvent'])

    def test_an_unmapped_entity_call_in_a_hook_fails(self):
        with mock.patch.dict(ef.HOOKS, {'bad': HOOK % ('bad', '')}, clear=True):
            ledger = Path(self.root) / 'ledger-hooks.json'
            ledger.write_text(json.dumps(LEDGER))
            mixins = Path(self.root) / 'mixins-hooks.json'
            mixins.write_text(json.dumps(self.entries))
            with self.assertRaises(SystemExit) as caught:
                mr.run(Namespace(ledger=str(ledger), install=str(self.install), out=str(Path(self.root) / 'hooks-out'), cache=None, mixins=str(mixins)))
        self.assertIn('Entity#wobble', str(caught.exception))

    def test_hook_writes_join_the_top_level_writes_as_neoforge(self):
        with mock.patch.dict(ef.HOOKS, {'direct': HOOK % ('direct', '')}, clear=True):
            facts = mr.analyse(self.install, self.root / 'out2', LEDGER, entries=self.entries, host=[], log=lambda *_: None)['entity_facts']
        self.assertEqual(facts['format'], 2)
        self.assertEqual(facts['hooks']['direct']['reads'], ['position', 'air'])
        self.assertIn(['neoforge'], [w['mods'] for w in facts['writes'] if w['key'].endswith('#setAirSupply(I)V')])


class Bytecode(unittest.TestCase):
    def test_instructions_give_pool_locals_and_absolute_targets(self):
        code = bytes([0x2a, 0x4c, 0x99, 0x00, 0x04, 0xb1, 0xb1])
        self.assertEqual(ef.instructions(code), [(0, 0x2a, 0), (1, 0x4c, 1), (2, 0x99, 6), (5, 0xb1, None), (6, 0xb1, None)])
        by_offset = {o: (o, op, x) for o, op, x in ef.instructions(bytes([0xa7, 0x00, 0x03, 0xb1]))}
        self.assertTrue(ef.is_exit(by_offset, 0))
        self.assertFalse(ef.is_exit(by_offset, 99))

    def test_the_event_slot_follows_wide_parameters(self):
        self.assertEqual(ef.slot_of_last('(JLa/B;)V', True), 2)
        self.assertEqual(ef.slot_of_last('(La/B;)V', False), 1)


class Classify(unittest.TestCase):
    def test_methods_mods_call_on_the_showcase_pack_have_groups(self):
        for owner, name, want in (('net/minecraft/world/entity/Entity', 'makeStuckInBlock', ('motion', True)),
                                  ('net/minecraft/world/entity/Entity', 'getPersistentData', ('java', False)),
                                  ('net/minecraft/world/entity/decoration/ItemFrame', 'setRotation', ('item', True)),
                                  ('net/minecraft/world/entity/vehicle/minecart/AbstractMinecartContainer', 'getItem', ('species', False)),
                                  ('net/minecraft/world/entity/vehicle/minecart/AbstractMinecartContainer', 'setChanged', ('pure', False)),
                                  ('net/minecraft/world/entity/Entity', 'load', ('all', True))):
            self.assertEqual(ef.classify(owner, name), want, name)

    def test_whole_entity_save_and_load_need_every_group(self):
        self.assertEqual(ef.carried('all'), ef.STATE_GROUPS)
        self.assertEqual(ef.carried('air'), ('air',))


if __name__ == '__main__':
    unittest.main()
