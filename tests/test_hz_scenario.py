import json
import os
import socket
import socketserver
import sys
import tempfile
import threading
import unittest
from pathlib import Path
from unittest import mock

from tools import hz_scenario as hs

FAKE_CTL = '''#!{python}
import json, socket, sys
args = sys.argv[1:]
sock = args[1]
rest = args[2:]
s = socket.socket(socket.AF_UNIX)
s.connect(sock)
s.sendall(json.dumps(rest).encode() + b"\\n")
reply = json.loads(s.makefile().readline())
if reply.get("code", 0):
    print(json.dumps(reply["body"]), file=sys.stderr)
else:
    print(json.dumps(reply["body"]))
sys.exit(reply.get("code", 0))
'''


class Server:
    """A unix socket that answers each fake horizon-ctl call from a handler, and records the calls."""
    def __init__(self, path, handler):
        self.calls = []
        outer = self

        class H(socketserver.StreamRequestHandler):
            def handle(self):
                args = json.loads(self.rfile.readline())
                outer.calls.append(args)
                self.wfile.write(json.dumps(handler(args)).encode() + b'\n')

        self.server = socketserver.ThreadingUnixStreamServer(str(path), H)
        threading.Thread(target=self.server.serve_forever, daemon=True).start()

    def close(self):
        self.server.shutdown()
        self.server.server_close()


def default_handler(world):
    def handle(args):
        cmd = args[0]
        if cmd == 'block':
            return {'body': {'block': world.get(tuple(map(int, args[1:4])), 'air'), 'state': 'x'}}
        if cmd == 'inventory':
            return {'body': {'held': {'item': 'stone', 'count': 4}, 'hotbar': [{'slot': 0, 'item': 'stone', 'count': 4}] + [{'item': None}] * 8, 'main': [{'item': None}] * 27}}
        if cmd == 'state':
            return {'body': {'player': {'position': [1.0, 64.0, 1.0]}}}
        if cmd == 'command':
            return {'body': {'ok': True, 'chat': ['Gave 4 [Stone] to Dev']}}
        if cmd == 'fail':
            return {'code': 1, 'body': {'error': 'boom'}}
        return {'body': {'ok': True}}
    return handle


class Base(unittest.TestCase):
    def setUp(self):
        self.dir = tempfile.TemporaryDirectory()
        self.tmp = Path(self.dir.name)
        ctl = self.tmp / 'horizon-ctl'
        ctl.write_text(FAKE_CTL.format(python=sys.executable))
        ctl.chmod(0o755)
        self.sock = self.tmp / 's.sock'
        self.log = hs.Log(self.tmp / 'latest.log')
        self.log.path.write_text('')
        self.world = {}
        self.server = Server(self.sock, default_handler(self.world))
        self.ctl = hs.Ctl(ctl, self.sock, os.environ.copy(), self.tmp)

    def tearDown(self):
        self.server.close()
        self.dir.cleanup()

    def run_script(self, script, **kw):
        kw.setdefault('timeout', 30)
        return hs.execute(self.ctl, hs.flatten(script), self.tmp, self.log, sleep=lambda s: None, **kw)


class StepTests(Base):
    def test_flatten_labels_the_next_step(self):
        steps = hs.flatten([{'name': 'a'}, {'wait': 3}, {'command': '/x'}])
        self.assertEqual(steps, [('a', 'wait', 3), ('command', 'command', '/x')])

    def test_steps_run_in_order_as_ctl_lines(self):
        result = self.run_script([{'name': 'go'}, {'command': '/fill 1'}, {'wait': 5}, {'slot': 2}, {'attack': 4}, {'tp': [1, 2, 3, 90, 10]}, {'lookat': [1.5, 2, 3]}, {'key': 'key.inventory'}, {'close': True}])
        self.assertTrue(result['ok'], result)
        self.assertEqual(self.server.calls, [['command', 'fill 1'], ['wait', '5'], ['select', '3'], ['mouse', 'left', 'down'], ['wait', '4'], ['mouse', 'left', 'up'],
                                             ['tp', '1', '2', '3'], ['look', '90', '10'], ['look-at', '1.5', '2', '3'], ['key', 'e', 'tap'], ['close']])
        self.assertEqual(result['steps'][0]['name'], 'go')

    def test_ctl_failure_stops_the_run(self):
        result = self.run_script([{'wait': 1}, {'fail': 1}, {'wait': 2}])
        self.assertFalse(result['ok'])
        self.assertEqual(result['failed']['i'], 1)
        self.assertEqual(len(result['steps']), 2)

    def test_screenshot_path_is_in_out(self):
        result = self.run_script([{'screenshot': 'shot one'}])
        self.assertEqual(result['screenshots'], [str(self.tmp / 'shot_one.png')])
        self.assertEqual(self.server.calls[0], ['screenshot', str(self.tmp / 'shot_one.png')])


class CheckTests(Base):
    def check(self, check):
        return self.run_script([{'check': check}])

    def test_block(self):
        self.world[(1, 2, 3)] = 'stone'
        self.assertTrue(self.check({'block': [1, 2, 3], 'is': 'minecraft:stone', 'within': 0})['ok'])
        result = self.check({'block': [1, 2, 4], 'is': 'stone', 'within': 0})
        self.assertFalse(result['ok'])
        self.assertEqual(result['steps'][0]['checks'][0]['got'], 'air')
        self.assertEqual(result['steps'][0]['checks'][0]['want'], 'stone')

    def test_held_slot_and_near(self):
        self.assertTrue(self.check({'held': 'stone', 'within': 0})['ok'])
        self.assertTrue(self.check({'slot': 0, 'item': 'stone', 'count': 4, 'within': 0})['ok'])
        self.assertFalse(self.check({'slot': 1, 'item': 'stone', 'within': 0})['ok'])
        self.assertTrue(self.check({'slot': 1, 'item': 'air', 'within': 0})['ok'])
        self.assertTrue(self.check({'near': [2, 64, 1], 'radius': 2, 'within': 0})['ok'])
        self.assertFalse(self.check({'near': [20, 64, 1], 'radius': 2, 'within': 0})['ok'])

    def test_output_of_the_last_command(self):
        result = self.run_script([{'command': '/give'}, {'check': {'output': 'Gave 4', 'within': 0}}, {'check': {'output': 'Unknown', 'within': 0}}])
        self.assertEqual([s['ok'] for s in result['steps']], [True, True, False])

    def test_log_since_the_last_step_started(self):
        self.log.path.write_text('old line Exception\n')
        script = [{'wait': 1}, {'check': {'log': 'Exception', 'present': False, 'within': 0}}]
        self.assertTrue(self.run_script(script)['ok'])
        self.assertTrue(self.run_script(script[:1] + [{'check': {'log': 'Exception', 'since': 'start', 'within': 0}}])['ok'])

    def test_log_written_during_a_step_is_seen(self):
        original = self.server.server.RequestHandlerClass.handle

        def handle(h):
            self.log.path.write_text(self.log.path.read_text() + 'java.lang.IllegalStateException: bad\n\tat x.y(Z.java:1)\n')
            original(h)
        self.server.server.RequestHandlerClass.handle = handle
        result = self.run_script([{'wait': 1}, {'check': {'no_exceptions': True, 'within': 0}}])
        self.assertFalse(result['ok'])
        self.assertIn('IllegalStateException', result['steps'][1]['checks'][0]['got'][0])
        self.assertIn('IllegalStateException', result['errors'][0])


class ExceptionTests(unittest.TestCase):
    def test_heads_skip_continuations_and_merge(self):
        text = ('[12:00:01] [main/ERROR]: java.lang.NullPointerException: x 5\n\tat a.b(C.java:1)\nCaused by: java.io.IOException: y\n'
                '[12:00:02] [main/ERROR]: java.lang.NullPointerException: x 9\n\tSuppressed: java.lang.Error: z\nplain INFO line\n')
        heads = hs.exception_heads(text)
        self.assertEqual(len(heads), 2)
        merged = hs.merged_heads(text)
        self.assertEqual(len(merged), 1)
        self.assertTrue(merged[0].endswith('(x2)'))

    def test_ignore(self):
        self.assertEqual(hs.exception_heads('a.b.FooException: x\n', 'Foo'), [])


class TimeoutTests(Base):
    def test_step_timeout_kills_the_call_and_cleans_up(self):
        import time
        self.server.server.RequestHandlerClass.handle = lambda h: time.sleep(5)
        cleaned = []
        start = time.monotonic()
        result = self.run_script([{'command': 'x'}], step_timeout=1, cleanup=lambda: cleaned.append(1))
        self.assertLess(time.monotonic() - start, 4)
        self.assertFalse(result['ok'])
        self.assertIn('timeout', result['failed']['error'])
        self.assertEqual(cleaned, [1])

    def test_whole_run_deadline(self):
        result = self.run_script([{'wait': 1}, {'wait': 1}], timeout=-1)
        self.assertEqual(result['failed']['error'], 'run timeout')
        self.assertEqual(self.server.calls, [])

    def test_cleanup_runs_when_a_step_raises(self):
        cleaned = []
        result = self.run_script([{'bogus': 1}], cleanup=lambda: cleaned.append(1))
        self.assertFalse(result['ok'])
        self.assertEqual(cleaned, [1])


class ResultTests(Base):
    def test_shape_and_file(self):
        self.log.path.write_text('x.YError: boom\n')
        result = self.run_script([{'name': 'a'}, {'wait': 1}])
        self.assertEqual(set(result), {'ok', 'steps', 'failed', 'errors', 'log', 'screenshots'})
        self.assertEqual(set(result['steps'][0]), {'i', 'name', 'ok', 'checks', 'ms'})
        self.assertIsNone(result['failed'])
        self.assertEqual(json.loads((self.tmp / 'result.json').read_text()), result)
        self.assertEqual(result['errors'], ['x.YError: boom'])


class InstallTests(unittest.TestCase):
    def test_install_is_not_written(self):
        with tempfile.TemporaryDirectory() as d:
            d = Path(d)
            install = d / 'install'
            for sub in ('libraries', 'mods', 'host', 'client/config', 'home/horizon/saves/W', 'home/horizon/logs'):
                (install / sub).mkdir(parents=True)
            (install / 'install.json').write_text('{}')
            (install / 'host' / 'horizon-java-host.jar').write_text('old')
            (install / 'home/horizon/saves/W/level.dat').write_text('x')
            (install / 'home/horizon/logs/latest.log').write_text('old log')
            jars = d / 'jars'
            jars.mkdir()
            (jars / 'horizon-java-host.jar').write_text('new')
            scratch, homes = hs.scratch_install(install, d / 'out', jars, False)
            self.assertEqual((scratch / 'host' / 'horizon-java-host.jar').read_text(), 'new')
            self.assertEqual((install / 'host' / 'horizon-java-host.jar').read_text(), 'old')
            self.assertTrue((homes / 'horizon' / 'saves' / 'W' / 'level.dat').is_file())
            self.assertFalse((homes / 'horizon' / 'logs' / 'latest.log').exists())
            self.assertTrue((scratch / 'libraries').is_symlink())


HISTO = ''' num     #instances         #bytes  class name (module)
-------------------------------------------------------
   1:       1150435      157681488  [B (java.base@25.0.4.1)
  40646:             1             16  yalter.mousetweaks.neoforge.MouseTweaksNeo$$Lambda/0x00000000181c9460 (mousetweaks@2.31)
Total      11002079      534824632
'''
METASPACE = '''Total Usage - 1757 loaders, 64799 classes (1618 shared):
  Non-Class: 10699 chunks,   195.41 MB capacity,  195.28 MB (>99%) committed,   194.07 MB (>99%) used,     1.2 MB ( <1%) free
       Both: 15022 chunks,   238.93 MB capacity,  238.62 MB (>99%) committed,   236.51 MB ( 99%) used,     2.10 MB ( <1%) free,     0 bytes (  0%) waste , deallocated: 696 blocks with 83.68 KB
'''


class HeapTests(Base):
    def test_histogram_total_and_metaspace_used(self):
        self.assertEqual(hs.histogram_total(HISTO), 534824632)
        self.assertEqual(hs.metaspace_used(METASPACE), int(236.51 * (1 << 20)))
        self.assertIsNone(hs.histogram_total('nothing'))
        self.assertIsNone(hs.metaspace_used('nothing'))

    def test_the_check_compares_both_limits(self):
        self.ctl.java, self.ctl.marker = '/jdk', 'm'
        outputs = {'GC.class_histogram': HISTO, 'VM.metaspace': METASPACE}
        with mock.patch.object(hs, 'game_pid', return_value=7), mock.patch.object(hs, 'jcmd', side_effect=lambda java, pid, command: outputs[command]):
            ok = hs.read_java_heap(self.ctl, {'max_live_mb': 600, 'max_metaspace_mb': 260})
            low = hs.read_java_heap(self.ctl, {'max_live_mb': 450, 'max_metaspace_mb': 260})
        self.assertEqual(ok, (True, {'max_live_mb': 600, 'max_metaspace_mb': 260}, {'live_mb': 510, 'metaspace_mb': 237}))
        self.assertFalse(low[0])

    def test_no_game_process_fails(self):
        with mock.patch.object(hs, 'game_pid', return_value=None):
            self.assertFalse(hs.read_java_heap(self.ctl, {'max_live_mb': 1})[0])


if __name__ == '__main__':
    unittest.main()
