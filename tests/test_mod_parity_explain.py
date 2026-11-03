import json
import tempfile
import unittest
from pathlib import Path
from unittest import mock

from tools import mod_parity as mp
from tools import mod_parity_explain as ex

JAVAP = '''Compiled from "EventHooks.java"
public class net.neoforged.neoforge.event.EventHooks {
  public static void fireThing(net.minecraft.world.level.Level);
    descriptor: (Lnet/minecraft/world/level/Level;)V
    Code:
       0: new           #7                  // class net/neoforged/neoforge/event/ThingEvent$Pre
       3: dup
       4: invokespecial #9                  // Method net/neoforged/neoforge/event/ThingEvent$Pre."<init>":()V
       7: getstatic     #11                 // Field net/neoforged/neoforge/common/NeoForge.EVENT_BUS:Lnet/neoforged/bus/api/IEventBus;
      10: invokeinterface #15,  2           // InterfaceMethod net/neoforged/bus/api/IEventBus.post:(Lnet/neoforged/bus/api/Event;)Lnet/neoforged/bus/api/Event;
      15: invokestatic  #17                 // Method java/lang/Object.hashCode:()I
      18: return

  public net.neoforged.neoforge.event.EventHooks();
    descriptor: ()V
    Code:
       0: invokespecial #1                  // Method java/lang/Object."<init>":()V
       4: return
}
Compiled from "Mob.java"
public abstract class net.minecraft.world.entity.Mob extends net.minecraft.world.entity.LivingEntity {
  public void checkDespawn();
    descriptor: ()V
    Code:
       0: aload_0
       1: invokestatic  #20                 // Method net/neoforged/neoforge/event/EventHooks.fireThing:(Lnet/minecraft/world/level/Level;)V
       4: return

  private static void lambda$tick$3(net.minecraft.world.entity.Mob);
    descriptor: (Lnet/minecraft/world/entity/Mob;)V
    Code:
       0: invokestatic  #20                 // Method net/neoforged/neoforge/event/EventHooks.fireThing:(Lnet/minecraft/world/level/Level;)V
       3: return

  static {};
    descriptor: ()V
    Code:
       0: new           #2                  // class net/neoforged/neoforge/event/Other
       3: return
}
'''

FIRE = 'net/neoforged/neoforge/event/EventHooks.fireThing:(Lnet/minecraft/world/level/Level;)V'
EVENT = 'net.neoforged.neoforge.event.ThingEvent$Pre'


def tiny_index():
    return {'news': {'net/neoforged/neoforge/event/ThingEvent$Pre': [FIRE]},
            'calls': {FIRE: ['net/minecraft/world/entity/Mob.checkDespawn:()V', 'net/minecraft/world/entity/Mob.lambda$tick$3:(Lnet/minecraft/world/entity/Mob;)V'],
                      'net/minecraft/world/entity/Mob.checkDespawn:()V': ['net/minecraft/server/level/ServerLevel.tick:()V']}}


class JavapTests(unittest.TestCase):
    def test_new_and_calls_by_caller(self):
        records = ex.parse_javap_text(JAVAP)
        self.assertIn(('new', 'net/neoforged/neoforge/event/ThingEvent$Pre', FIRE), records)
        self.assertIn(('call', FIRE, 'net/minecraft/world/entity/Mob.checkDespawn:()V'), records)
        self.assertIn(('call', FIRE, 'net/minecraft/world/entity/Mob.lambda$tick$3:(Lnet/minecraft/world/entity/Mob;)V'), records)
        self.assertIn(('new', 'net/neoforged/neoforge/event/Other', 'net/minecraft/world/entity/Mob.<clinit>:()V'), records)

    def test_other_calls_and_constructors_are_dropped(self):
        records = ex.parse_javap_text(JAVAP)
        self.assertFalse([r for r in records if 'java/lang' in r[1]])
        self.assertFalse([r for r in records if r[0] == 'call' and '<init>' in r[1]])

    def test_class_list_and_cache_key(self):
        with tempfile.TemporaryDirectory() as d:
            a, b = Path(d) / 'a.jar', Path(d) / 'b.jar'
            a.write_bytes(b'one')
            b.write_bytes(b'two')
            self.assertEqual(ex.digest([a]), ex.digest([a]))
            self.assertNotEqual(ex.digest([a]), ex.digest([b]))
            with mock.patch.object(ex, 'index_jars', return_value={'news': {}, 'calls': {}}) as built:
                ex.load_index('javap', [a], Path(d) / 'cache')
                ex.load_index('javap', [a], Path(d) / 'cache')
            self.assertEqual(built.call_count, 1)


class CallerTests(unittest.TestCase):
    def test_vanilla_callers_through_a_hook_with_their_callers(self):
        found = ex.callers_of(tiny_index(), EVENT)
        self.assertEqual(found['constructors'], ['net.neoforged.neoforge.event.EventHooks.fireThing'])
        methods = {c['method']: c for c in found['callers']}
        mob = methods['net/minecraft/world/entity/Mob.checkDespawn:()V']
        self.assertEqual(mob['via'], ['EventHooks.fireThing'])
        self.assertEqual(mob['up'], ['net.minecraft.server.level.ServerLevel.tick'])

    def test_constructed_in_vanilla_has_no_hook(self):
        index = {'news': {'net/neoforged/neoforge/event/X': ['net/minecraft/server/level/ChunkMap.save:()V']}, 'calls': {}}
        found = ex.callers_of(index, 'net.neoforged.neoforge.event.X')
        self.assertEqual([(c['method'], c['via']) for c in found['callers']], [('net/minecraft/server/level/ChunkMap.save:()V', [])])

    def test_no_constructor_gives_nothing(self):
        found = ex.callers_of(tiny_index(), 'net.neoforged.neoforge.event.Nope')
        self.assertEqual((found['constructors'], found['callers']), ([], []))

    def test_extension_method_is_reached_through_the_vanilla_class(self):
        index = {'news': {'net/neoforged/neoforge/event/X': ['net/neoforged/neoforge/common/Hooks.get:()I']},
                 'calls': {'net/neoforged/neoforge/common/Hooks.get:()I': ['net/neoforged/neoforge/common/extensions/IItemExt.get:()I'],
                           'net/minecraft/world/item/ItemStack.get:()I': ['net/minecraft/world/item/Foo.use:()V']}}
        found = ex.callers_of(index, 'net.neoforged.neoforge.event.X')
        self.assertEqual([c['method'] for c in found['callers']], ['net/minecraft/world/item/Foo.use:()V'])

    def test_names(self):
        self.assertEqual(ex.readable('net/minecraft/a/B.lambda$tick$3:()V'), 'net.minecraft.a.B.tick (lambda)')
        self.assertEqual(ex.ledger_name('net/minecraft/a/B$C$1.lambda$run$0:()V'), 'B.C.run')
        self.assertEqual(ex.ledger_name('net/minecraft/a/B$C.go:()V'), 'B.C.go')


LEDGER_METHOD = json.dumps([{'vanilla': 'Mob.checkDespawn', 'status': 'ported', 'ported_in': ['crates/x/src/ai.rs:10-30 ai::despawn'], 'map': [], 'mentioned_in': [], 'same_name_in': []}])
LEDGER_MAPPED = json.dumps([{'vanilla': 'ChunkMap.scheduleUnload', 'status': 'mapped', 'ported_in': [], 'mentioned_in': [],
                             'map': ['inline: crates/x/src/game.rs:13889-14067 Game::update_world save (tools/port_map/g.tsv:41)'], 'class_home': ['crates/x/src/streaming.rs']}])
LEDGER_CLASS = json.dumps([{'class': 'Font.X', 'home': ['crates/x/src/hud.rs'], 'methods': {}}])
LEDGER_BARE = json.dumps([{'vanilla': 'A.b', 'status': 'missing', 'ported_in': [], 'map': [], 'mentioned_in': [], 'same_name_in': [], 'class_home': ['crates/x/src/home.rs']}])


class LedgerTests(unittest.TestCase):
    def test_parse(self):
        self.assertEqual(ex.parse_ledger(LEDGER_METHOD)['port'], 'crates/x/src/ai.rs:10')
        self.assertEqual(ex.parse_ledger(LEDGER_MAPPED)['port'], 'crates/x/src/game.rs:13889')
        self.assertEqual(ex.parse_ledger(LEDGER_CLASS)['home'], 'crates/x/src/hud.rs')
        bare = ex.parse_ledger(LEDGER_BARE)
        self.assertEqual((bare['port'], bare['home'], bare['status']), (None, 'crates/x/src/home.rs', 'missing'))
        self.assertIsNone(ex.parse_ledger('Nothing in the pinned version is called Z.y.'))

    def test_class_answer_when_the_method_has_none(self):
        answers = {'A.b': LEDGER_BARE, 'A': LEDGER_METHOD}
        ledger = ex.Ledger(run=lambda q: answers.get(q, ''))
        self.assertEqual(ex.port_of(ledger, 'A.b', 'A')['port'], 'crates/x/src/ai.rs:10')

    def test_answers_are_asked_once(self):
        calls = []
        ledger = ex.Ledger(run=lambda q: calls.append(q) or LEDGER_METHOD)
        ledger.where('Mob.checkDespawn')
        ledger.where('Mob.checkDespawn')
        self.assertEqual(calls, ['Mob.checkDespawn'])


class HostTests(unittest.TestCase):
    def setUp(self):
        self.root = Path(tempfile.mkdtemp())
        boot = self.root / 'boot'
        boot.mkdir()
        (boot / 'A.java').write_text('import net.x.ChunkEvent;\n// `ChunkMap.scheduleUnload` after the save\npost(new ChunkEvent.Unload(chunk));\nEventHooks.fireThing(level);\nvar t = ChunkEvent.Load.class;\n')
        (self.root / 'testmod').mkdir()
        (self.root / 'testmod' / 'T.java').write_text('new ChunkEvent.Unload(x);\n')
        self.lines = ex.host_lines(self.root)

    def test_skips_test_mods_and_finds_sites(self):
        self.assertEqual({p.name for p, _, _ in self.lines}, {'A.java'})
        sites = ex.host_sites(self.lines, 'net.neoforged.neoforge.event.level.ChunkEvent$Unload', ['fireThing'], ['ChunkMap.scheduleUnload'], self.root)
        self.assertEqual(sites['fires'], ['boot/A.java:3'])
        self.assertEqual(sites['hooks'], ['boot/A.java:4'])
        self.assertEqual(sites['near_miss'], ['boot/A.java:2'])
        self.assertEqual(sites['mentions'], [])

    def test_mention_is_not_a_firing(self):
        sites = ex.host_sites(self.lines, 'net.neoforged.neoforge.event.level.ChunkEvent$Load', [], [], self.root)
        self.assertEqual((sites['fires'], sites['mentions']), ([], ['boot/A.java:5']))


class HandlerTests(unittest.TestCase):
    ENTRY = {'mod': 'jade', 'mixin': 'a.FogMixin', 'handler': 'jade$setupFog', 'kind': 'inject', 'access': False, 'forge': 5,
             'target_class': ['net.minecraft.client.renderer.fog.FogRenderer'], 'target_method': ['Lnet/minecraft/Foo;setupFog(II)V', 'other']}

    def test_target_methods(self):
        self.assertEqual(ex.target_methods(self.ENTRY), ['setupFog', 'other'])

    def test_merged_files_say_what_java_loaded(self):
        target = 'net.minecraft.client.renderer.fog.FogRenderer'
        ledger = ex.Ledger(run=lambda q: LEDGER_METHOD)
        cases = {
            'the handler merged': f'{target}\thandler$zzk000$jade$setupFog\t()V\ta.FogMixin\n',
            'handler was not merged': f'{target}\thandler$zzk000$jade$other\t()V\ta.FogMixin\n',
            'this mixin never applied': f'{target}\thandler$zzk000$x$y\t()V\tb.Other\n',
            'never loads': 'net.minecraft.Other\thandler$zzk000$x$y\t()V\tb.Other\n',
        }
        for expect, text in cases.items():
            with tempfile.TemporaryDirectory() as d:
                (Path(d) / 'merged.txt').write_text(text)
                found = ex.explain_handler(self.ENTRY, ex.parse_merged(Path(d) / 'merged.txt'), ledger)
            self.assertIn(expect, found['java_side'])
            self.assertEqual(found['vanilla_port'], 'crates/x/src/ai.rs:10')
        self.assertEqual(ex.explain_handler(self.ENTRY, {}, ledger)['java_side'], 'unknown')

    def test_line_names_the_native_owner(self):
        ledger = ex.Ledger(run=lambda q: LEDGER_METHOD)
        line = ex.handler_line(ex.explain_handler(self.ENTRY, {}, ledger))
        self.assertIn('native owns `crates/x/src/ai.rs:10`', line)
        self.assertIn("Java's version never runs", line)


class SectionTests(unittest.TestCase):
    def test_short_events_need_some_horizon_calls_and_not_be_missing(self):
        events = {'missing': [{'event': 'M'}], 'count_mismatches': [
            {'event': 'A', 'side': 'server', 'forge': 100, 'horizon': 10}, {'event': 'A', 'side': 'client', 'forge': 50, 'horizon': 5},
            {'event': 'B', 'side': 'server', 'forge': 10, 'horizon': 0}, {'event': 'C', 'side': 'server', 'forge': 1, 'horizon': 9},
            {'event': 'M', 'side': 'server', 'forge': 9, 'horizon': 1}]}
        self.assertEqual(ex.short_events(events), [('A', 150, 15, ['client', 'server'])])

    def test_insert_replaces_an_older_section(self):
        md = '# Report\n\n|a|\n\n## Missing events (x)\n\nrows\n\n## Extra\n'
        once = ex.insert_section(md, '## Where to fix\n\nfirst\n')
        twice = ex.insert_section(once, '## Where to fix\n\nsecond\n')
        self.assertIn('second', twice)
        self.assertNotIn('first', twice)
        self.assertEqual(twice.count('## Where to fix'), 1)
        self.assertLess(twice.index('## Where to fix'), twice.index('## Missing events'))
        self.assertIn('rows', twice)

    def test_explain_a_run_end_to_end(self):
        report = {'events': {'missing': [{'event': EVENT, 'count': 7, 'sides': ['server'], 'first_step': 'pre'},
                                         {'event': 'net.neoforged.neoforge.event.Nope', 'count': 3, 'sides': ['client'], 'first_step': 'pre'}],
                             'extra': [], 'count_mismatches': [], 'order_differences': []},
                  'mixins': [dict(HandlerTests.ENTRY, state='dead in Horizon')], 'steps': ['pre']}
        with tempfile.TemporaryDirectory() as d:
            run = Path(d)
            (run / 'report.json').write_text(json.dumps(report))
            (run / 'report.md').write_text('# Mod parity report\n\n## Missing events (fired)\n\nrows\n')
            ledger = ex.Ledger(run=lambda q: LEDGER_METHOD)
            where = ex.explain(run, index=tiny_index(), ledger=ledger)
            ex.write_explanation(run, where)
            md = (run / 'report.md').read_text()
            saved = json.loads((run / 'report.json').read_text())
        self.assertEqual([e['event'] for e in saved['where_to_fix']['missing_events']], [EVENT, 'net.neoforged.neoforge.event.Nope'])
        self.assertIn('`event.ThingEvent$Pre` (server, 7) → `net.minecraft.world.entity.Mob.checkDespawn` via EventHooks.fireThing', md)
        self.assertIn('ported at crates/x/src/ai.rs:10', md)
        self.assertIn('### No vanilla caller found (1)', md)
        self.assertIn('### Dead mixin handlers (1)', md)

    def test_explain_command_is_wired_into_mod_parity(self):
        self.assertEqual(mp.explain_module().explain.__name__, 'explain')


if __name__ == '__main__':
    unittest.main()
