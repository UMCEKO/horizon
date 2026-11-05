import io
import json
import os
import shutil
import subprocess
import tempfile
import unittest
import zipfile
from pathlib import Path

from tools import mod_replaces as mr

SOURCES = {
    'net/minecraft/world/Base.java': 'package net.minecraft.world; public abstract class Base { public void tick() {} public void tick(int a) {} public abstract void must(); public static void st() {} }',
    'net/minecraft/world/Thing.java': 'package net.minecraft.world; import net.neoforged.neoforge.common.extensions.IThingExtension; public abstract class Thing extends Base implements IThingExtension {}',
    'net/neoforged/neoforge/common/extensions/IThingExtension.java': 'package net.neoforged.neoforge.common.extensions; public interface IThingExtension { default int friction() { return 1; } }',
    'net/neoforged/bus/api/Event.java': 'package net.neoforged.bus.api; public class Event {}',
    'net/neoforged/bus/api/ICancellableEvent.java': 'package net.neoforged.bus.api; public interface ICancellableEvent {}',
    'net/neoforged/bus/api/EventPriority.java': 'package net.neoforged.bus.api; public enum EventPriority { HIGH, NORMAL, LOW }',
    'net/neoforged/bus/api/SubscribeEvent.java': 'package net.neoforged.bus.api; import java.lang.annotation.*; @Retention(RetentionPolicy.RUNTIME) public @interface SubscribeEvent { EventPriority priority() default EventPriority.NORMAL; boolean receiveCanceled() default false; }',
    'net/neoforged/bus/api/IEventBus.java': ('package net.neoforged.bus.api; import java.util.function.Consumer; public interface IEventBus { <T extends Event> void addListener(Consumer<T> c); '
                                             '<T extends Event> void addListener(EventPriority p, Consumer<T> c); <T extends Event> void addListener(EventPriority p, boolean b, Class<T> k, Consumer<T> c); }'),
    'net/neoforged/fml/event/IModBusEvent.java': 'package net.neoforged.fml.event; public interface IModBusEvent {}',
    'net/neoforged/fml/common/EventBusSubscriber.java': 'package net.neoforged.fml.common; import java.lang.annotation.*; @Retention(RetentionPolicy.RUNTIME) public @interface EventBusSubscriber { Bus bus() default Bus.GAME; enum Bus { GAME, MOD } }',
    'net/neoforged/neoforgespi/transformation/ClassProcessorProvider.java': 'package net.neoforged.neoforgespi.transformation; public interface ClassProcessorProvider {}',
    'net/neoforged/neoforge/event/PlainEvent.java': 'package net.neoforged.neoforge.event; public class PlainEvent extends net.neoforged.bus.api.Event {}',
    'net/neoforged/neoforge/event/CancelEvent.java': 'package net.neoforged.neoforge.event; public class CancelEvent extends net.neoforged.bus.api.Event implements net.neoforged.bus.api.ICancellableEvent {}',
    'net/neoforged/neoforge/event/ModThing.java': 'package net.neoforged.neoforge.event; public class ModThing extends net.neoforged.bus.api.Event implements net.neoforged.fml.event.IModBusEvent {}',
    'fixture/One.java': ('package fixture; import net.minecraft.world.Thing; public class One extends Thing { '
                         '@Override public void tick() {} public void tick(long x) {} public void must() {} @Override public int friction() { return 2; } public void own() {} }'),
    'fixture/Listeners.java': ('package fixture; import net.neoforged.bus.api.*; import net.neoforged.neoforge.event.*; public class Listeners { '
                               '@SubscribeEvent(priority = EventPriority.HIGH, receiveCanceled = true) public void onPlain(PlainEvent e) {} '
                               'public Listeners(IEventBus bus) { bus.addListener(this::onCancel); bus.addListener(EventPriority.LOW, (ModThing e) -> {}); '
                               'bus.addListener(EventPriority.NORMAL, false, PlainEvent.class, e -> {}); } void onCancel(CancelEvent e) {} }'),
    'fixture/Sub.java': ('package fixture; import net.neoforged.bus.api.SubscribeEvent; import net.neoforged.fml.common.EventBusSubscriber; import net.neoforged.neoforge.event.PlainEvent; '
                         '@EventBusSubscriber(bus = EventBusSubscriber.Bus.MOD) public class Sub { @SubscribeEvent public static void on(PlainEvent e) {} }'),
    'inner/Deep.java': 'package inner; import net.minecraft.world.Base; public abstract class Deep extends Base { public void tick() {} public void must() {} }',
}
TOML = 'modId = "{}"'
LEDGER = {
    'methods': {'net/minecraft/world/Base#tick': {}, 'net/minecraft/world/Base#must': {}, 'net/minecraft/world/Base#other': {}},
    'fns': [{'file': 'crates/a.rs', 'line': 7}],
    'ported': {'net/minecraft/world/Base#tick': [0]},
    'absent': {'net/minecraft/world/Base#must': {'note': 'nothing'}},
    'skipped': {}, 'classes': {},
}


def javac():
    for home in (os.environ.get('JAVA_HOME'),):
        if home and (Path(home) / 'bin' / 'javac').is_file():
            return f'{home}/bin/javac'
    return shutil.which('javac')


def jar(path, files):
    path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(path, 'w') as z:
        for name, data in files.items():
            z.writestr(name, data)


def classes_under(root, *prefixes):
    return {str(p.relative_to(root)): p.read_bytes() for p in Path(root).rglob('*.class') if str(p.relative_to(root)).startswith(prefixes)}


@unittest.skipUnless(javac(), 'needs javac')
class Fixture(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        root = Path(cls.tmp.name)
        src, out = root / 'src', root / 'classes'
        for name, text in SOURCES.items():
            (src / name).parent.mkdir(parents=True, exist_ok=True)
            (src / name).write_text(text)
        subprocess.run([javac(), '-proc:none', '-nowarn', '-d', str(out), *map(str, src.rglob('*.java'))], check=True, capture_output=True)
        cls.classes = out
        install = root / 'install'
        libs = install / 'libraries' / 'net' / 'neoforged'
        jar(libs / 'minecraft-client-patched' / '1' / 'minecraft-client-patched-1.jar', classes_under(out, 'net/minecraft/'))
        jar(libs / 'minecraft-server-patched' / '1' / 'minecraft-server-patched-1.jar', {'placeholder.txt': b''})
        jar(libs / 'neoforge' / '1' / 'neoforge-1-universal.jar', classes_under(out, 'net/neoforged/neoforge/'))
        jar(libs / 'fancymodloader' / 'loader' / '1' / 'loader-1.jar', classes_under(out, 'net/neoforged/fml/', 'net/neoforged/neoforgespi/'))
        jar(libs / 'bus' / '1' / 'bus-1.jar', classes_under(out, 'net/neoforged/bus/'))
        inner = io.BytesIO()
        with zipfile.ZipFile(inner, 'w') as z:
            z.writestr('META-INF/neoforge.mods.toml', '[[mods]]\n' + TOML.format('inner'))
            for name, data in classes_under(out, 'inner/').items():
                z.writestr(name, data)
        files = classes_under(out, 'fixture/')
        files.update({
            'META-INF/neoforge.mods.toml': '[[mods]]\n' + TOML.format('fix') + '\n[[mixins]]\nconfig = "fix.mixins.json"\n',
            'fix.mixins.json': '{"package": "fixture.mixin", "plugin": "fixture.Plugin", "mixins": []}',
            'data/minecraft/recipe/a.json': '{}', 'data/minecraft/recipe/b.json': '{}', 'data/minecraft/tags/block/c.json': '{}',
            'data/minecraft/worldgen/biome/d.json': '{}', 'data/minecraft/loot_table/blocks/e.json': '{}', 'data/other/recipe/no.json': '{}',
            'assets/minecraft/lang/en_us.json': '{}',
            'META-INF/accesstransformer.cfg': '# comment\npublic net.minecraft.world.Base tick()V\n\npublic net.minecraft.world.Base must()V # tail\n',
            'META-INF/jarjar/inner-1.jar': inner.getvalue(),
        })
        jar(install / 'mods' / 'fix.jar', files)
        jar(install / 'mods' / 'plain.jar', {'META-INF/neoforge.mods.toml': '[[mods]]\n' + TOML.format('plain'),
                                             'META-INF/services/net.neoforged.neoforgespi.transformation.ClassProcessorProvider': 'x.Provider\n'})
        cls.install = install
        cls.entries = [
            {'mod': 'fix', 'config': 'fix.mixins.json', 'mixin': 'fixture.mixin.A', 'handler': 'onTick', 'kind': 'inject', 'target_class': ['net.minecraft.world.Thing'],
             'target_method': ['tick'], 'at': ['HEAD'], 'side': 'common', 'access': False},
            {'mod': 'fix', 'config': 'fix.mixins.json', 'mixin': 'fixture.mixin.A', 'handler': 'onMust', 'kind': 'redirect', 'target_class': ['net.minecraft.world.Base'],
             'target_method': ['Lnet/minecraft/world/Base;must()V'], 'at': ['INVOKE'], 'side': 'client', 'access': False},
            {'mod': 'fix', 'config': 'fix.mixins.json', 'mixin': 'fixture.mixin.B', 'handler': 'getSecret', 'kind': 'accessor', 'target_class': ['net.minecraft.world.Base'],
             'target_method': [], 'at': [], 'side': 'common', 'access': True},
        ]
        cls.result = mr.analyse(install, root / 'out', LEDGER, entries=cls.entries, host=[], log=lambda *_: None)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def changes(self, key, mechanism=None):
        return [c for c in self.result['methods'][key]['changes'] if mechanism in (None, c['mechanism'])]


@unittest.skipUnless(javac(), 'needs javac')
class Classes(Fixture):
    def test_the_parser_reads_methods_annotations_and_calls(self):
        info = mr.parse_class((self.classes / 'fixture' / 'Listeners.class').read_bytes(), annotations=True, code=True)
        on = next(m for m in info['methods'] if m['name'] == 'onPlain')
        self.assertEqual(on['annos'][mr.SUBSCRIBE], {'priority': 'HIGH', 'receiveCanceled': 1})
        calls = [e for m in info['methods'] if m['name'] == '<init>' for e in m['events'] if e[0] == 'call' and e[2] == 'addListener']
        self.assertEqual(len(calls), 3)
        self.assertEqual(info['super'], 'java/lang/Object')

    def test_descriptors_split(self):
        self.assertEqual(mr.params('(ILa/B;[Lc/D;J[I)V'), ['I', 'a/B', '[Lc/D;', 'J', '[I'])


@unittest.skipUnless(javac(), 'needs javac')
class Overrides(Fixture):
    def test_an_override_needs_the_same_name_and_descriptor(self):
        tick = self.changes('net/minecraft/world/Base#tick', 'override')
        self.assertEqual({(c['mod'], c['desc']) for c in tick}, {('fix', '()V'), ('inner', '()V')})
        self.assertFalse(any(c['desc'] in ('(J)V', '(I)V') for c in tick))
        self.assertEqual(tick[0]['scope'], 'own-type')

    def test_a_new_method_and_a_static_are_not_overrides(self):
        keys = set(self.result['methods'])
        self.assertNotIn('fixture/One#own', keys)
        self.assertFalse(any(k.endswith('#own') or k.endswith('#st') for k in keys))

    def test_extension_defaults_count(self):
        change = self.changes('net/neoforged/neoforge/common/extensions/IThingExtension#friction')[0]
        self.assertEqual((change['mechanism'], change['extension']), ('override', True))

    def test_implementing_an_abstract_method_is_not_a_replacement(self):
        must = self.changes('net/minecraft/world/Base#must', 'implement')
        self.assertEqual({c['mod'] for c in must}, {'fix', 'inner'})
        self.assertTrue(all(c['routing'] == 'java-already' for c in must))

    def test_jar_in_jar_mods_are_their_own(self):
        self.assertEqual(self.result['mods']['inner']['counts'], {'implement': 1, 'override': 1})


@unittest.skipUnless(javac(), 'needs javac')
class Mixins(Fixture):
    def test_a_mixin_resolves_to_the_declaring_class(self):
        change = self.changes('net/minecraft/world/Base#tick', 'mixin')[0]
        self.assertEqual((change['kind'], change['at'], change['side']), ('inject', ['HEAD'], 'common'))
        self.assertEqual(change['routing'], 'java')

    def test_a_config_plugin_makes_it_conditional(self):
        self.assertEqual(self.changes('net/minecraft/world/Base#tick', 'mixin')[0]['conditional'], 'config plugin fixture.Plugin')

    def test_descriptor_in_a_target_is_split_off(self):
        self.assertEqual(mr.split_target('Lnet/minecraft/world/Base;must()V'), ('must', '()V'))
        self.assertEqual(mr.split_target('tick'), ('tick', None))

    def test_accessors_are_inert(self):
        change = self.changes('net/minecraft/world/Base#getSecret')[0]
        self.assertEqual((change['mechanism'], change['routing']), ('access', 'inert'))


@unittest.skipUnless(javac(), 'needs javac')
class Listeners(Fixture):
    def event(self, name):
        return self.result['events'][f'net/neoforged/neoforge/event/{name}']

    def test_subscribe_event_keeps_priority_and_receive_canceled(self):
        change = next(c for c in self.event('PlainEvent')['changes'] if c['via'] == 'subscribe')
        self.assertEqual((change['priority'], change['receive_canceled'], change['class'], change['method']), ('HIGH', True, 'fixture.Listeners', 'onPlain'))

    def test_a_method_reference_gives_the_event(self):
        change = self.event('CancelEvent')['changes'][0]
        self.assertEqual((change['via'], change['method'], change['priority']), ('addListener', '<init>', 'NORMAL'))
        self.assertTrue(self.event('CancelEvent')['cancellable'])
        self.assertFalse(self.event('PlainEvent')['cancellable'])

    def test_a_lambda_gives_the_event_and_the_priority(self):
        change = self.event('ModThing')['changes'][0]
        self.assertEqual((change['priority'], change['bus']), ('LOW', 'mod'))

    def test_an_explicit_class_argument_wins(self):
        vias = [(c['via'], c['bus']) for c in self.event('PlainEvent')['changes']]
        self.assertIn(('addListener', 'game'), vias)

    def test_a_subscriber_class_names_its_bus(self):
        change = next(c for c in self.event('PlainEvent')['changes'] if c['via'] == 'subscriber-class')
        self.assertEqual((change['class'], change['bus']), ('fixture.Sub', 'mod'))

    def test_events_route_to_the_native_hook(self):
        self.assertEqual({e['routing'] for e in self.result['events'].values()}, {'native-hook'})


@unittest.skipUnless(javac(), 'needs javac')
class Files(Fixture):
    def test_data_overrides_by_kind_in_the_minecraft_namespace_only(self):
        data = self.result['data']
        self.assertEqual(data['data:recipe']['fix']['files'], 2)
        self.assertEqual(sorted(data), ['assets:lang', 'data:loot_table', 'data:recipe', 'data:tags/block', 'data:worldgen/biome'])

    def test_access_transformer_lines_are_counted(self):
        self.assertEqual(self.result['mods']['fix']['counts']['access_transformer'], 2)

    def test_a_transformation_service_is_opaque(self):
        plain = self.result['mods']['plain']
        self.assertFalse(plain['can_analyse_ahead'])
        self.assertIn('ClassProcessorProvider', plain['opaque'][0])

    def test_a_mixin_config_plugin_only_makes_its_mixins_conditional(self):
        fix = self.result['mods']['fix']
        self.assertTrue(fix['can_analyse_ahead'])
        self.assertEqual(fix['opaque'], [])
        self.assertTrue(self.result['mods']['inner']['can_analyse_ahead'])


@unittest.skipUnless(javac(), 'needs javac')
class Routing(Fixture):
    def test_native_owned_changes_run_in_java_and_the_rest_already_do(self):
        tick, must = self.result['methods']['net/minecraft/world/Base#tick'], self.result['methods']['net/minecraft/world/Base#must']
        self.assertEqual((tick['routing'], tick['native']['status'], tick['native']['fns']), ('java', 'ported', ['crates/a.rs:7']))
        self.assertEqual((must['routing'], must['native']['status']), ('undecided', 'absent'))
        self.assertEqual({c['routing'] for c in must['changes'] if c['mechanism'] == 'mixin'}, {'undecided'})

    def test_a_verdict_decides_a_weak_status(self):
        native = mr.Native(LEDGER).info('net/minecraft/world/Base#must')
        key = 'net/minecraft/world/Base#must'
        self.assertEqual(mr.route('mixin', native, key, {'Base.must': 'port'}), 'java')
        self.assertEqual(mr.route('mixin', native, key, {'Base.must': 'skip:server'}), 'java-already')
        self.assertEqual(mr.route('mixin', native, key, {'Base.must': 'missing'}), 'java-already')
        self.assertEqual(mr.route('mixin', native, key, {}), 'undecided')

    def test_totals_and_summary(self):
        self.assertEqual(self.result['mods']['fix']['routing'], {'java': 1, 'java-own-type': 1, 'undecided': 1, 'java-already': 2, 'native-hook': 3, 'inert': 1})
        self.assertEqual(self.result['totals']['access'], 1)

    def test_the_markdown_lists_mods_opaque_flags_and_the_top_methods(self):
        text = mr.markdown(self.result)
        self.assertIn('| fix |', text)
        self.assertIn("Can't be analysed ahead", text)
        self.assertIn('`Base#tick`', text)
        json.dumps(self.result)


class Synthetic(unittest.TestCase):
    def test_native_ownership_needs_a_rust_fn(self):
        native = mr.Native({'methods': {'a#b': {}}, 'fns': [{'file': 'f.rs', 'line': 1}], 'mapped': {'a#b': {'fn': None, 'note': 'inline'}}})
        self.assertFalse(native.info('a#b')['owned'])
        self.assertEqual(mr.route('override', native.info('a#b')), 'java-already')
        self.assertEqual(native.info('a#c')['status'], 'unknown')

    def test_data_kinds(self):
        self.assertEqual(mr.data_kind('data/minecraft/tags/item/x.json'), 'data:tags/item')
        self.assertEqual(mr.data_kind('assets/minecraft/textures/block/x.png'), 'assets:textures')


if __name__ == '__main__':
    unittest.main()
