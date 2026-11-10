import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile
from unittest import mock
from pathlib import Path

from tools import mod_parity as mp

INSTALL = Path(os.environ.get('HORIZON_PARITY_INSTALL', '/nonexistent'))


def trace(time, thread, *frames):
    stack = ''.join(f'    {f} line: 1\n' for f in frames)
    return (f'jdk.MethodTrace {{\n  startTime = {time} (2026-11-03)\n  duration = 0.001 ms\n  method = net.neoforged.bus.api.Event.<init>()\n'
            f'  eventThread = "{thread}" (javaThreadId = 31)\n  stackTrace = [\n{stack}  ]\n}}\n\n')


def timing(time, method, n):
    return f'jdk.MethodTiming {{\n  startTime = {time} (2026-11-03)\n  method = {method}\n  invocations = {n}\n  average = 0.1 ms\n}}\n\n'


def epoch(time):
    return mp.local_epoch(f'{time} (2026-11-03)')


STEPS = [('0', 'a', epoch('10:00:10.000')), ('1', 'b', epoch('10:00:20.000')), ('2', 'end', epoch('10:00:30.000'))]
TICK = 'net.neoforged.neoforge.event.TickEvent$Post'
TEXT = ''.join([
    trace('10:00:01.000', 'Worker-Main-1', 'net.neoforged.fml.FMLLoadEvent.<init>(int)', 'net.neoforged.fml.Base.<init>()'),
    trace('10:00:11.000', 'Render thread', 'net.neoforged.neoforge.event.TickEvent.<init>()', TICK + '.<init>(int)', 'mod.Caller.run()'),
    trace('10:00:12.000', 'Render thread', 'net.neoforged.neoforge.event.TickEvent.<init>()', TICK + '.<init>(int)', 'mod.Caller.run()'),
    trace('10:00:13.000', 'Server thread', 'mod.ChunkEvent.<init>()', 'mod.Caller.run()'),
    trace('10:00:21.000', 'Render thread', 'mod.ScreenEvent.<init>()'),
    timing('10:00:10.100', 'net.minecraft.Foo.handler$zbj000$onHurt(int)', 1),
    timing('10:00:20.100', 'net.minecraft.Foo.handler$zbj000$onHurt(int)', 4),
    timing('10:00:20.100', 'net.minecraft.Foo.redirect$zbj001$other()', 2),
    timing('10:00:30.100', 'net.minecraft.Foo.handler$zbj000$onHurt(int)', 9),
    timing('10:00:30.100', 'net.minecraft.Foo.plainNotMerged()', 7),
])
MERGED = {('net.minecraft.Foo', 'handler$zbj000$onHurt'): 'mod.FooMixin', ('net.minecraft.Foo', 'redirect$zbj001$other'): 'mod.FooMixin'}


class Parsing(unittest.TestCase):
    def test_events_have_type_thread_and_frames(self):
        events = list(mp.jfr_events(io.StringIO(TEXT)))
        self.assertEqual(len(events), 10)
        self.assertEqual(events[1]['thread'], 'Render thread')
        self.assertEqual(events[1]['frames'][0], 'net.neoforged.neoforge.event.TickEvent.<init>() line: 1')
        self.assertEqual(events[5]['invocations'], 1)

    def test_deepest_event_constructor(self):
        self.assertEqual(mp.event_class(['a.TickEvent.<init>() line: 1', 'a.TickEvent$Post.<init>(int) line: 2', 'x.Caller.run() line: 3']), 'a.TickEvent$Post')
        self.assertEqual(mp.event_class(['a.Helper.<init>() line: 1', 'a.Thing.<init>() line: 2', 'x.run() line: 3']), 'a.Helper')
        self.assertIsNone(mp.event_class(['x.run() line: 3']))

    def test_sides_by_thread(self):
        self.assertEqual([mp.side_of(t) for t in ('main', 'Render thread', 'Server thread', 'Worker-Main-1')], ['client', 'client', 'server', 'other'])

    def test_merged_names(self):
        self.assertEqual(mp.original_name('handler$zbj000$onHurt'), 'onHurt')
        self.assertEqual(mp.original_name('redirect$abc012$lambda$x$0'), 'lambda$x$0')
        self.assertEqual(mp.original_name('overwritten'), 'overwritten')
        self.assertEqual(mp.timing_target('net.minecraft.Foo$Bar.handler$zbj000$onHurt(int, net.X)'), ('net.minecraft.Foo$Bar', 'handler$zbj000$onHurt'))

    def test_steps_file(self):
        with tempfile.TemporaryDirectory() as d:
            (Path(d) / 'steps.txt').write_text('step 0 place the block 123 1762164010000000000\nstep 1 end 5 1762164020000000000\n')
            steps = mp.read_steps(Path(d) / 'steps.txt')
        self.assertEqual([s[:2] for s in steps], [('0', 'place the block'), ('1', 'end')])
        self.assertEqual(steps[0][2], 1762164010.0)


class Aggregation(unittest.TestCase):
    def setUp(self):
        self.run = mp.aggregate(mp.jfr_events(io.StringIO(TEXT)), STEPS, MERGED)

    def test_steps_are_pre_then_the_scenario(self):
        self.assertEqual(self.run['steps'], ['pre', '0 a', '1 b'])

    def test_events_fall_into_steps_and_sides(self):
        ev = self.run['events']
        self.assertEqual(sum(ev[0]['other']['counts'].values()), 1)
        self.assertEqual(ev[1]['client']['counts'], {TICK: 2})
        self.assertEqual(ev[1]['server']['counts'], {'mod.ChunkEvent': 1})
        self.assertEqual(ev[2]['client']['counts'], {'mod.ScreenEvent': 1})
        self.assertEqual(ev[2]['server']['counts'], {})

    def test_handler_counts_are_differences_of_cumulative_timings(self):
        key = 'mod.FooMixin#onHurt'
        self.assertEqual([h.get(key, 0) for h in self.run['handlers']], [1, 3, 5])
        self.assertEqual([h.get('mod.FooMixin#other', 0) for h in self.run['handlers']], [0, 2, 0])

    def test_methods_the_processor_did_not_log_are_unmapped(self):
        self.assertEqual(self.run['unmapped'], {'net.minecraft.Foo.plainNotMerged': 7})


def run_of(steps, events=None, handlers=None):
    n = len(steps)
    empty = {s: {'counts': {}, 'order': []} for s in mp.SIDES}
    ev = [json.loads(json.dumps(empty)) for _ in range(n)]
    for (i, side), counts in (events or {}).items():
        ev[i][side] = {'counts': counts, 'order': list(counts)}
    return {'steps': steps, 'events': ev, 'handlers': [dict(h) for h in (handlers or [{}] * n)], 'unmapped': {}}


class Comparison(unittest.TestCase):
    STEPS = ['pre', 'a', 'b']

    def events(self, forge, horizon):
        return mp.compare_events(run_of(self.STEPS, forge), run_of(self.STEPS, horizon))

    def test_missing_and_extra(self):
        r = self.events({(1, 'client'): {'A': 5, 'B': 1}}, {(1, 'client'): {'A': 5, 'C': 3}})
        self.assertEqual([(m['event'], m['count']) for m in r['missing']], [('B', 1)])
        self.assertEqual([(m['event'], m['count']) for m in r['extra']], [('C', 3)])

    def test_count_tolerance_is_twenty_percent_plus_two(self):
        self.assertFalse(mp.mismatch(10, 14))
        self.assertTrue(mp.mismatch(10, 15))
        self.assertFalse(mp.mismatch(0, 2))
        self.assertTrue(mp.mismatch(0, 3))
        r = self.events({(1, 'client'): {'A': 100}, (2, 'client'): {'A': 100}}, {(1, 'client'): {'A': 122}, (2, 'client'): {'A': 123}})
        self.assertEqual([(m['step'], m['forge'], m['horizon']) for m in r['count_mismatches']], [('b', 100, 123)])

    def test_frame_events_compare_by_presence(self):
        fov = 'net.neoforged.neoforge.client.event.ViewportEvent$ComputeFov'
        forge = {(1, 'client'): {fov: 600, 'A': 600}}
        r = mp.compare_events(run_of(self.STEPS, forge), run_of(self.STEPS, {(1, 'client'): {fov: 40, 'A': 40}}), {fov})
        self.assertEqual([m['event'] for m in r['count_mismatches']], ['A'])
        self.assertEqual((r['missing'], r['frame_unlistened']), ([], []))
        r = mp.compare_events(run_of(self.STEPS, forge), run_of(self.STEPS, {(1, 'client'): {'A': 600}}), {fov})
        self.assertEqual([m['event'] for m in r['missing']], [fov])
        r = mp.compare_events(run_of(self.STEPS, forge), run_of(self.STEPS, {(1, 'client'): {'A': 600}}), set())
        self.assertEqual((r['missing'], [u['event'] for u in r['frame_unlistened']]), ([], [fov]))

    def test_frame_listened_reads_replaces_json(self):
        fov = 'net.neoforged.neoforge.client.event.ViewportEvent$ComputeFov'
        self.assertEqual(mp.frame_listened_of(None), set(mp.FRAME_EVENTS))
        with tempfile.TemporaryDirectory() as d:
            (Path(d) / 'analysis').mkdir()
            (Path(d) / 'analysis' / 'replaces.json').write_text(json.dumps({'events': {fov.replace('.', '/'): {'changes': [{}]}, 'x/Other': {'changes': [{}]},
                                                                                       'net/neoforged/neoforge/client/event/ViewportEvent$RenderFog': {'changes': []}}}))
            self.assertEqual(mp.frame_listened_of(d), {fov})

    def test_a_class_in_another_step_is_a_count_mismatch_not_missing(self):
        r = self.events({(1, 'client'): {'A': 9}}, {(2, 'client'): {'A': 9}})
        self.assertEqual(r['missing'], [])
        self.assertEqual(sorted((m['step'], m['forge'], m['horizon']) for m in r['count_mismatches']), [('a', 9, 0), ('b', 0, 9)])

    def test_order_of_first_occurrence_within_a_step(self):
        r = self.events({(1, 'client'): {'A': 3, 'B': 3, 'C': 3}}, {(1, 'client'): {'B': 3, 'A': 3, 'C': 3}})
        self.assertEqual([(o['step'], o['side']) for o in r['order_differences']], [('a', 'client')])
        same = self.events({(1, 'client'): {'A': 3, 'B': 3}}, {(1, 'client'): {'A': 3, 'X': 1, 'B': 3}})
        self.assertEqual(same['order_differences'], [])

    def test_prefixed_handler_names_resolve_to_the_declared_one(self):
        known = {'m.M': {'onHurt', 'jade$x'}}
        self.assertEqual(mp.resolve_handler(known, 'm.M', 'ae2$onHurt'), 'onHurt')
        self.assertEqual(mp.resolve_handler(known, 'm.M', 'jade$x'), 'jade$x')
        self.assertEqual(mp.resolve_handler(known, 'm.M', 'unknown'), 'unknown')

    def test_mixin_classes(self):
        mixins = [{'mod': 'm', 'mixin': 'm.M', 'handler': h, 'kind': 'inject', 'access': False, 'target_class': ['T'], 'target_method': ['x'], 'side': 'common'}
                  for h in ('both', 'dead', 'only', 'none')]
        forge = run_of(self.STEPS, handlers=[{}, {'m.M#both': 2, 'm.M#dead': 5}, {'m.M#other': 1}])
        horizon = run_of(self.STEPS, handlers=[{}, {'m.M#both': 1, 'm.M#only': 4}, {}])
        rows, other = mp.compare_mixins(forge, horizon, mixins)
        self.assertEqual({r['handler']: r['state'] for r in rows}, {'both': 'both', 'dead': 'dead in Horizon', 'only': 'Horizon only', 'none': 'not exercised'})
        self.assertEqual(other, {'forge': 1, 'horizon': 0})

    def test_report_lists_the_gap(self):
        mixins = [{'mod': 'm', 'mixin': 'm.M', 'handler': 'dead', 'kind': 'inject', 'access': False, 'target_class': ['T'], 'target_method': ['x'], 'side': 'common'}]
        forge = run_of(self.STEPS, {(1, 'client'): {'A': 5}}, [{}, {'m.M#dead': 5}, {}])
        horizon = run_of(self.STEPS)
        text = mp.report_markdown(mp.compare(forge, horizon, mixins), forge, horizon)
        self.assertIn('| mixin handlers: dead in Horizon | 1 |', text)
        self.assertIn('| A | client | 5 | a |', text)


class Scenarios(unittest.TestCase):
    def test_names_label_the_next_step(self):
        self.assertEqual(mp.flatten([{'name': 'go'}, {'wait': 5}, {'close': True}]), [('go', 'wait', 5), ('close', 'close', True)])

    def test_horizon_commands(self):
        self.assertEqual(mp.ctl_commands('use', 3), [['mouse', 'right', 'down'], ['wait', '3'], ['mouse', 'right', 'up']])
        self.assertEqual(mp.ctl_commands('command', '/give @s x'), [['command', 'give @s x']])
        self.assertEqual(mp.ctl_commands('slot', 0), [['select', '1']])
        self.assertEqual(mp.ctl_commands('press', 'key.sprint'), [['key', 'ctrl', 'down']])
        self.assertEqual(mp.ctl_commands('release', 'key.forward'), [['key', 'w', 'up']])

    def test_the_motion_scenario_drives_horizon(self):
        _, scenario = mp.load_scenario('w4_motion')
        steps = [(name, kind, arg) for name, kind, arg in mp.flatten(scenario) if kind != 'quit']
        self.assertTrue({'walk', 'sprint', 'step', 'swim', 'fly', 'fall'} <= {name for name, _, _ in steps})
        for _, kind, arg in steps:
            mp.ctl_commands(kind, arg)

    def test_the_default_scenario_is_valid(self):
        _, scenario = mp.load_scenario('basic')
        for _, kind, arg in mp.flatten(scenario):
            self.assertTrue(mp.ctl_commands(kind, arg))


def jdk():
    try:
        home = json.loads((INSTALL / 'install.json').read_text())['java']
    except OSError:
        home = os.environ.get('JAVA_HOME')
    return home if home and (Path(home) / 'bin' / 'javac').is_file() and (INSTALL / 'libraries').is_dir() else None


@unittest.skipUnless(jdk(), 'needs a JDK and the install\'s ASM libraries')
class Scanner(unittest.TestCase):
    def test_java_check(self):
        java = jdk()
        with tempfile.TemporaryDirectory() as d:
            classpath = mp.asm_jars(INSTALL)
            classes = mp.compile_scanner(java, classpath, d)
            sources = sorted((mp.PARITY / 'scan').rglob('MixinScannerCheck.java'))
            subprocess.run([f'{java}/bin/javac', '--release', '25', '-proc:none', '-cp', ':'.join(map(str, [classes, *classpath])), '-d', str(classes), *map(str, sources)], check=True)
            r = subprocess.run([f'{java}/bin/java', '-cp', ':'.join(map(str, [classes, *classpath])), 'dev.horizonlab.javamods.parity.MixinScannerCheck'], capture_output=True, text=True)
            self.assertEqual(r.returncode, 0, r.stdout + r.stderr)

    def test_install_scan_on_a_tiny_jar(self):
        java = jdk()
        with tempfile.TemporaryDirectory() as d:
            work = Path(d)
            src = work / 'src' / 'org' / 'spongepowered' / 'asm' / 'mixin'
            (src / 'injection').mkdir(parents=True)
            (src / 'Mixin.java').write_text('package org.spongepowered.asm.mixin; public @interface Mixin { Class<?>[] value() default {}; }')
            (src / 'injection' / 'At.java').write_text('package org.spongepowered.asm.mixin.injection; public @interface At { String value(); }')
            (src / 'injection' / 'Inject.java').write_text('package org.spongepowered.asm.mixin.injection; public @interface Inject { String[] method(); At[] at(); }')
            sample = work / 'src' / 'p' / 'FooMixin.java'
            sample.parent.mkdir(parents=True)
            sample.write_text('package p; import org.spongepowered.asm.mixin.*; import org.spongepowered.asm.mixin.injection.*;'
                              ' @Mixin(Object.class) abstract class FooMixin { @Inject(method = "toString", at = @At("HEAD")) void h() {} }')
            subprocess.run([f'{java}/bin/javac', '-proc:none', '-d', str(work / 'cls'), *map(str, (work / 'src').rglob('*.java'))], check=True)
            inner = io.BytesIO()
            with zipfile.ZipFile(inner, 'w') as z:
                z.writestr('META-INF/neoforge.mods.toml', '[[mods]]\nmodId="nested"\n[[mixins]]\nconfig="n.mixins.json"\n')
                z.writestr('n.mixins.json', json.dumps({'package': 'p', 'mixins': ['FooMixin']}))
                z.write(work / 'cls' / 'p' / 'FooMixin.class', 'p/FooMixin.class')
            install = work / 'install'
            (install / 'mods').mkdir(parents=True)
            (install / 'libraries').symlink_to(INSTALL / 'libraries')
            (install / 'install.json').write_text(json.dumps({'java': java}))
            with zipfile.ZipFile(install / 'mods' / 'outer.jar', 'w') as z:
                z.writestr('META-INF/neoforge.mods.toml', '[[mods]]\nmodId="outer"\n')
                z.writestr('META-INF/jarjar/nested.jar', inner.getvalue())
            entries = mp.scan_install(install, work / 'out')
            self.assertEqual(len(entries), 1)
            self.assertEqual((entries[0]['mod'], entries[0]['mixin'], entries[0]['handler'], entries[0]['kind']), ('nested', 'p.FooMixin', 'h', 'inject'))
            self.assertEqual((entries[0]['target_class'], entries[0]['target_method'], entries[0]['at']), (['java.lang.Object'], ['toString'], ['HEAD']))
            self.assertTrue((work / 'out' / 'mixins.md').is_file())


class GateTests(unittest.TestCase):
    def test_no_gate_means_no_waiting(self):
        with mock.patch.dict(os.environ, {}, clear=False), mock.patch.object(mp.subprocess, 'run') as run:
            os.environ.pop('HORIZON_GAME_GATE', None)
            mp.acquire_gate('parity')
        run.assert_not_called()

    def test_gate_command_runs_with_the_owner(self):
        with mock.patch.dict(os.environ, {'HORIZON_GAME_GATE': 'my gate --quiet'}), mock.patch.object(mp.subprocess, 'run') as run:
            run.return_value.returncode = 0
            mp.acquire_gate('parity')
        self.assertEqual(run.call_args.args[0], ['my', 'gate', '--quiet'])
        self.assertEqual(run.call_args.kwargs['env']['GATE_OWNER'], 'parity')

    def test_gfx_env_comes_from_the_named_file_only(self):
        with tempfile.TemporaryDirectory() as d:
            path = Path(d) / 'gfx.env'
            path.write_text('DISPLAY=:9\nNOT A LINE\n')
            with mock.patch.dict(os.environ, {'HORIZON_GFX_ENV': str(path)}):
                self.assertEqual(mp.gfx_env(), {'DISPLAY': ':9'})
            with mock.patch.dict(os.environ, {}, clear=False):
                os.environ.pop('HORIZON_GFX_ENV', None)
                self.assertEqual(mp.gfx_env(), {})


if __name__ == '__main__':
    unittest.main()
