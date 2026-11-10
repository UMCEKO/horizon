import io
import json
import shutil
import subprocess
import tempfile
import unittest
import zipfile
from pathlib import Path

from tools import port_triage as pt

TOOLS = shutil.which('javac') and shutil.which('javap')

STUBS = {
    'net/minecraft/world/Thing.java': 'package net.minecraft.world; public class Thing { public void hurt() {} public void tick() {} }',
    'net/minecraft/world/Kid.java': 'package net.minecraft.world; public class Kid extends Thing { }',
}
MOD = {
    'mod/Caller.java': 'package mod; import net.minecraft.world.Kid; public class Caller { void go(Kid k) { k.hurt(); } }',
    'mod/Base.java': 'package mod; import net.minecraft.world.Thing; public class Base extends Thing { }',
    'mod/Deep.java': 'package mod; public class Deep extends Base { @Override public void hurt() { } }',
}


def build_jar(path, files, stubs=None):
    """Compiles files (and stubs on the class path) and zips only the files' classes."""
    with tempfile.TemporaryDirectory() as work:
        work = Path(work)
        for name, text in {**(stubs or {}), **files}.items():
            (work / 'src' / name).parent.mkdir(parents=True, exist_ok=True)
            (work / 'src' / name).write_text(text)
        subprocess.run(['javac', '-nowarn', '-d', str(work / 'out'), *map(str, (work / 'src').rglob('*.java'))], check=True, capture_output=True)
        with zipfile.ZipFile(path, 'w') as z:
            for name in files:
                for cls in (work / 'out' / Path(name).parent).glob(Path(name).stem + '*.class'):
                    z.write(cls, cls.relative_to(work / 'out').as_posix())


def fake_ledger():
    m = lambda cls, name, **kw: {'class': cls, 'name': name, 'size': kw.get('size', 10), 'touches': kw.get('touches', ['hp']), 'abstract': False, 'accessor': False}  # noqa: E731
    T, K, B = 'net/minecraft/world/Thing', 'net/minecraft/world/Kid', 'net/minecraft/world/level/Block'
    c = 'net/minecraft/client/gui/Screen'
    return {
        'classes': {T: {'extends': []}, K: {'extends': [T]}, B: {'extends': []}, c: {'extends': []}},
        'methods': {f'{T}#hurt': m(T, 'hurt'), f'{T}#tick': m(T, 'tick'), f'{B}#setBlock': m(B, 'setBlock'), f'{B}#getName': m(B, 'getName'), f'{B}#quiet': m(B, 'quiet', size=2),
                    f'{c}#init': m(c, 'init'), f'{B}#harnessed': m(B, 'harnessed', size=2, touches=[])},
        'absent': {f'{T}#hurt': {'note': 'n', 'src': 's'}, f'{B}#setBlock': {'note': '', 'src': ''}, f'{B}#getName': {'note': '', 'src': ''}, f'{B}#quiet': {'note': '', 'src': ''},
                   f'{c}#init': {'note': '', 'src': ''}, f'{B}#harnessed': {'note': '', 'src': ''}},
        'harness': {f'{B}#other': ['block_harness']}, 'homes': {},
    }


@unittest.skipUnless(TOOLS, 'needs javac and javap')
class JarScan(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.dir = Path(self.tmp.name)
        self.javap = shutil.which('javap')
        self.vanilla = pt.Vanilla(fake_ledger(), set(fake_ledger()['absent']))
        build_jar(self.dir / 'mod.jar', MOD, STUBS)

    def tearDown(self):
        self.tmp.cleanup()

    def test_call_through_a_subclass_resolves_to_the_declaring_class(self):
        from tools import mod_parity_explain as ex
        index = ex.load_index(self.javap, [self.dir / 'mod.jar'], self.dir / 'cache')
        self.assertEqual(pt.calls_signal(self.vanilla, [('mekanism', index)]), {'net/minecraft/world/Thing#hurt': {'mekanism': 1}})

    def test_override_through_a_mod_base_class(self):
        decls = pt.declarations(self.javap, self.dir / 'mod.jar', self.dir / 'cache')
        found = pt.overrides_signal(self.vanilla, [('ae2', decls)])
        self.assertEqual(found, {'net/minecraft/world/Thing#hurt': {'ae2': ['mod.Deep']}})

    def test_nested_jar_belongs_to_its_mod(self):
        outer = self.dir / 'mods' / 'outer-1.jar'
        outer.parent.mkdir()
        buf = io.BytesIO(( self.dir / 'mod.jar').read_bytes())
        with zipfile.ZipFile(outer, 'w') as z:
            z.writestr('META-INF/jarjar/inner.jar', buf.getvalue())
        found = pt.mod_jars(self.dir, self.dir / 'cache')
        self.assertEqual([m for m, _ in found], ['outer-1', 'outer-1>inner'])


class Pure(unittest.TestCase):
    def setUp(self):
        self.ledger = fake_ledger()
        self.targets = pt.targets_of(self.ledger)
        self.vanilla = pt.Vanilla(self.ledger, set(self.targets))

    def test_mixin_target_by_method_name_and_descriptor(self):
        entries = [{'mod': 'jade', 'mixin': 'j.ThingMixin', 'handler': 'onHurt', 'kind': 'inject', 'target_class': ['net.minecraft.world.Kid'], 'target_method': ['Lnet/minecraft/world/Kid;hurt(F)V']}]
        self.assertEqual(pt.mixins_signal(self.vanilla, entries), {'net/minecraft/world/Thing#hurt': {'jade': ['ThingMixin.onHurt (inject)']}})

    def test_event_poster_through_a_hook(self):
        index = {'news': {'net/neoforged/neoforge/event/HurtEvent': ['net/neoforged/neoforge/event/EventHooks.fireHurt:()V'],
                          'net/neoforged/neoforge/common/UsernameCache$SaveThread': ['net/neoforged/neoforge/event/EventHooks.fireHurt:()V']},
                 'calls': {'net/neoforged/neoforge/event/EventHooks.fireHurt:()V': ['net/minecraft/world/Thing.hurt:()V', 'net/minecraft/world/Thing$1.lambda$tick$0:()V']}}
        found = pt.events_signal(self.vanilla, index)
        self.assertEqual(found, {'net/minecraft/world/Thing#hurt': ['event.HurtEvent'], 'net/minecraft/world/Thing#tick': ['event.HurtEvent']})

    def test_extension_alias_reaches_vanilla_callers_but_generic_names_do_not(self):
        ext = 'net/neoforged/neoforge/common/extensions/IThingExtension.onHurt:()V'
        gen = 'net/neoforged/neoforge/client/gui/Loading.tick:()V'
        index = {'news': {'net/neoforged/neoforge/event/AEvent': [ext], 'net/neoforged/neoforge/event/BEvent': [gen]},
                 'calls': {'net/minecraft/world/Thing.onHurt:()V': ['net/minecraft/world/Thing.hurt:()V'], 'net/minecraft/world/Thing.tick:()V': ['net/minecraft/world/Kid.hurt:()V']}}
        found = pt.events_signal(self.vanilla, index)
        self.assertEqual(found, {'net/minecraft/world/Thing#hurt': ['event.AEvent']})

    def test_classification_and_order(self):
        signals = {'calls': {'net/minecraft/world/Thing#hurt': {'mekanism': 3}}, 'overrides': {}, 'mixins': {}, 'events': {}, 'hot': {}}
        rows = pt.triage(self.vanilla, self.targets, signals, '', {'net/minecraft/world/level/Block': ['block_harness']})
        by = {r['method']: r for r in rows}
        self.assertEqual(by['Thing.hurt']['class'], 'P2')
        self.assertIn('mekanism (3)', by['Thing.hurt']['reason'])
        self.assertEqual(by['Block.setBlock']['class'], 'P1-candidate')
        self.assertIn('block_harness', by['Block.setBlock']['reason'])
        self.assertEqual(by['Screen.init']['class'], 'deferred')
        self.assertEqual([r['class'] for r in rows], sorted((r['class'] for r in rows), key=pt.CLASSES.index))

    def test_gameplay_state_change_without_harness(self):
        rows = pt.triage(self.vanilla, self.targets, {'calls': {}, 'overrides': {}, 'mixins': {}, 'events': {}, 'hot': {}}, '', {})
        by = {r['method']: r for r in rows}
        self.assertEqual(by['Thing.hurt']['class'], 'P1-candidate')
        self.assertEqual(by['Block.getName']['class'], 'deferred')
        self.assertEqual(by['Block.quiet']['class'], 'deferred')
        self.assertIn('changes state', by['Thing.hurt']['reason'])

    def test_client_call_only_ranks_below_a_real_reference(self):
        calls = {'net/minecraft/client/gui/Screen#init': {'jade': 5}, 'net/minecraft/world/Thing#hurt': {'jade': 1}}
        rows = pt.triage(self.vanilla, self.targets, {'calls': calls, 'overrides': {}, 'mixins': {}, 'events': {}, 'hot': {}}, '', {})
        self.assertEqual([r['method'] for r in rows[:2]], ['Thing.hurt', 'Screen.init'])
        self.assertIn('java-serves', rows[1]['reason'])

    def test_unanswered_are_opt_in(self):
        self.ledger.update({k: {} for k in ('ported', 'mapped', 'skipped', 'mentioned', 'named', 'linked', 'structural', 'inferred')})
        self.ledger['methods']['net/minecraft/world/Thing#extra'] = {'class': 'net/minecraft/world/Thing', 'name': 'extra', 'size': 9}
        self.assertNotIn('net/minecraft/world/Thing#extra', pt.targets_of(self.ledger))
        self.assertEqual(pt.targets_of(self.ledger, True)['net/minecraft/world/Thing#extra']['status'], 'unanswered')

    def test_hot_frames(self):
        self.assertEqual(pt.hot_frame('net.minecraft.world.Thing$1.hurt(float) line: 12'), 'net/minecraft/world/Thing$1#hurt')
        self.assertIsNone(pt.hot_frame('java.lang.Thread.run() line: 1'))
        self.assertEqual(pt.hot_signal(self.vanilla, {'net/minecraft/world/Kid#hurt': 7}), {'net/minecraft/world/Thing#hurt': 7})

    def test_markdown_counts(self):
        rows = pt.triage(self.vanilla, self.targets, {'calls': {}, 'overrides': {}, 'mixins': {}, 'events': {}, 'hot': {}}, '', {})
        by_class, by_package = pt.counts_of(rows)
        text = pt.markdown({'methods': rows, 'counts': by_class, 'packages': by_package, 'set': 'x', 'install': 'i', 'hotness_note': 'no --run given'})
        self.assertIn('| P1-candidate | 2 |', text)
        self.assertIn('no --run given', text)


class Verdicts(unittest.TestCase):
    def test_lines_and_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp = Path(tmp)
            (tmp / 'd.tsv').write_text('# c\nThing.hurt\tp2\tmekanism calls it\nKid.x\tskip:java-serves\tJava runs it\nBlock.y\tskip:ui\tours\nBlock.z\tdeferred\t\nnet/minecraft/world/Thing#tick\tp1\tvisible\n')
            src = tmp / 'port_ledger.py'
            src.write_text('for p in MAP_DIR.glob("*.tsv"):\n')
            out, n = pt.write_verdicts(tmp / 'd.tsv', '2026-11-04', tmp, src)
            self.assertEqual(out.name, 'triage-2026-11-04.tsv')
            self.assertEqual(n, 4)
            self.assertEqual(out.read_text().splitlines(), ['Thing.hurt\tmissing\t\tP2: mekanism calls it', 'Kid.x\tskip:java-serves\t\tJava runs it',
                                                           'Block.y\tskip:ui\t\tours', 'Thing.tick\tmissing\t\tP1: visible'])

    def test_refuses_when_the_ledger_ignores_extra_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            src = Path(tmp) / 'port_ledger.py'
            src.write_text('open("tools/port_map/rest-1.tsv")\n')
            (Path(tmp) / 'd.tsv').write_text('Thing.hurt\tp2\tr\n')
            with self.assertRaises(SystemExit):
                pt.write_verdicts(Path(tmp) / 'd.tsv', '2026-11-04', tmp, src)

    def test_bad_decision_and_missing_reason(self):
        with tempfile.TemporaryDirectory() as tmp:
            f = Path(tmp) / 'd.tsv'
            for text in ('A.b\tmaybe\tr\n', 'A.b\tp2\t\n'):
                f.write_text(text)
                with self.assertRaises(SystemExit):
                    pt.read_decisions(f)

    def test_real_ledger_reads_extra_files(self):
        self.assertTrue(pt.ledger_reads_extra_files())


if __name__ == '__main__':
    unittest.main()
