#!/usr/bin/env python3
"""Runs a scripted session on Horizon with the Java mods of an install, checks its state, and prints one short JSON result.

    hz_scenario.py run --install DIR --script S.json --out OUT [--game BIN] [--jars DIR] [--world NAME] [--size WxH] [--fresh] [--keep]

The install is only read: its home/ and client config are copied into OUT. Steps (a {"name": ...} entry labels the next step):
  {"wait": TICKS} {"command": "/fill ..."} {"look": [yaw, pitch]} {"lookat": [x, y, z]} {"slot": 0-8} {"use": TICKS} {"attack": TICKS}
  {"key": "key.inventory"} {"close": true} {"tp": [x, y, z]} or {"tp": [x, y, z, yaw, pitch]} {"screenshot": "name"} {"camera": [x, y, z, yaw, pitch]} {"hud": false} {"quit": true}
  {"check": {...}} with one of:
    {"block": [x,y,z], "is": "stone", "state": "oak_stairs[facing=east]"}   the loaded block at a position
    {"looking_at": "stone"}                                                  the block under the crosshair
    {"held": "item"}  {"slot": 0, "item": "stone", "count": 4}               the held item, or a slot's (0-8 hotbar, 9-35 main)
    {"near": [x,y,z], "radius": 2}                                           the player's position
    {"output": "regex", "present": true}                                     the chat lines the last command returned
    {"log": "regex", "present": true}  {"no_exceptions": true, "ignore": "regex"}   the log since the last step started
    {"java_heap": {"max_live_mb": 450, "max_metaspace_mb": 260}}             the game JVM's live heap after a full GC, and its Metaspace
  Every check takes "within" (seconds to keep trying, default 3) and the log checks take "since": "start".
"""
import argparse
import collections
import json
import os
import re
import shlex
import shutil
import signal
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
CAMERA_SETTLE = 200  # ticks for chunks to mesh; horizon-ctl has no renderer-idle signal
GATE = os.environ.get('HORIZON_GAME_GATE', '')
GFX_ENV = os.environ.get('HORIZON_GFX_ENV', '')
KEYS = {'key.inventory': 'e', 'key.jump': 'space', 'key.sneak': 'shift', 'key.sprint': 'ctrl', 'key.drop': 'q', 'key.chat': 't',
        'key.swapOffhand': 'f', 'key.forward': 'w', 'key.back': 's', 'key.left': 'a', 'key.right': 'd'}
HEAD = re.compile(r'(?:^|[\s:\]])((?:[A-Za-z_$][\w$]*\.)+[\w$]*(?:Exception|Error|Throwable))\b')


def flatten(script):
    """[(name, kind, argument)]: a `name` entry labels the step after it."""
    steps, pending = [], None
    for item in script['steps'] if isinstance(script, dict) else script:
        if 'name' in item:
            pending = item['name']
            continue
        kind = next(iter(item))
        steps.append((pending or kind, kind, item[kind]))
        pending = None
    return steps


def ctl_commands(kind, arg):
    """The horizon-ctl lines that do one scenario step; waits are in game ticks."""
    if kind == 'wait':
        return [['wait', str(arg)]]
    if kind == 'command':
        return [['command', arg.lstrip('/')]]
    if kind == 'look':
        return [['look', str(arg[0]), str(arg[1])]]
    if kind == 'lookat':
        return [['look-at', *map(str, arg)]]
    if kind == 'slot':
        return [['select', str(arg + 1)]]
    if kind in ('use', 'attack'):
        button = 'right' if kind == 'use' else 'left'
        return [['mouse', button, 'down'], ['wait', str(arg)], ['mouse', button, 'up']]
    if kind == 'key':
        if arg.startswith('key.hotbar.'):
            return [['key', arg.rsplit('.', 1)[1], 'tap']]
        if arg.startswith('key.') and arg not in KEYS:
            raise ValueError(f'unknown key {arg}')
        return [['key', KEYS.get(arg, arg), 'tap']]
    if kind in ('press', 'release'):
        if arg.startswith('key.') and arg not in KEYS:
            raise ValueError(f'unknown key {arg}')
        return [['key', KEYS.get(arg, arg), 'down' if kind == 'press' else 'up']]
    if kind == 'close':
        return [['close']]
    if kind == 'tp':
        lines = [['tp', *map(str, arg[:3])]]
        return lines + ([['look', str(arg[3]), str(arg[4])]] if len(arg) >= 5 else [])
    if kind == 'camera':
        return ctl_commands('tp', arg) + [['wait', str(CAMERA_SETTLE)]]
    if kind == 'hud':
        return [] if arg else [['key', 'f1', 'tap']]
    if kind == 'quit':
        return [['quit']]
    raise ValueError(f'unknown step {kind}')


# Log reading and exceptions

class Log:
    """Reads a growing log by offset; its text is only ever matched, never returned to the caller's output."""
    def __init__(self, path):
        self.path, self.mark_at = Path(path), 0

    def size(self):
        try:
            return self.path.stat().st_size
        except OSError:
            return 0

    def mark(self):
        self.mark_at = self.size()

    def text(self, since='step'):
        start = 0 if since == 'start' or self.size() < self.mark_at else self.mark_at
        try:
            with open(self.path, 'rb') as f:
                f.seek(start)
                return f.read().decode('utf-8', 'replace')
        except OSError:
            return ''


class Logs:
    """Several growing logs read as one: the game's latest.log and the launcher's ctl.log, where the Java side's stderr lands."""
    def __init__(self, *paths):
        self.logs = [Log(p) for p in paths]
        self.path = self.logs[0].path

    def mark(self):
        for log in self.logs:
            log.mark()

    def text(self, since='step'):
        return '\n'.join(log.text(since) for log in self.logs)


def exception_heads(text, ignore=None):
    """The first line of every stack trace in the text: a line naming an exception class that is not a trace continuation."""
    heads = []
    for line in text.splitlines():
        if line[:1] in ' \t' or line.startswith('Caused by:') or (ignore and re.search(ignore, line)):
            continue
        if HEAD.search(line):
            heads.append(line.strip())
    return heads


def merged_heads(text, limit=5):
    """Heads with their numbers, timestamps and paths blurred, merged and counted, most frequent first."""
    count = collections.Counter()
    first = {}
    for head in exception_heads(text):
        head = re.sub(r'^\[[\d:]+\] \[[^\]]*\]: ', '', head)
        key = re.sub(r'0x[0-9a-f]+|\d+|/[\w./+-]+', 'N', head)[:160]
        count[key] += 1
        first.setdefault(key, head[:200])
    return [first[k] + (f' (x{n})' if n > 1 else '') for k, n in count.most_common(limit)]


def summarize_errors(text):
    """tools/logsum.py's merged errors (known noise left out), or the built-in merge above when it cannot be used."""
    try:
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        import logsum
        errors = logsum.scan(text.splitlines(True))[1]
        rows = [e for e in sorted(errors.values(), key=lambda e: e.lineno) if not logsum.known(e.text())]
        return [logsum.cut(e.text() + (f' (x{e.count})' if e.count > 1 else ''), 200) for e in rows[:5]]
    except Exception:
        return merged_heads(text)


# horizon-ctl

class Ctl:
    def __init__(self, binary, socket, env, cwd, java=None, marker=None):
        self.base, self.env, self.cwd = [str(binary), '--socket', str(socket)], env, cwd
        self.java, self.marker = java, marker

    def __call__(self, *args, timeout=120):
        """(exit code, parsed JSON or the text); a timeout is (None, 'timeout') with the call killed."""
        try:
            r = subprocess.run([*self.base, *args], env=self.env, cwd=self.cwd, capture_output=True, text=True, timeout=timeout)
        except subprocess.TimeoutExpired:
            return None, 'timeout'
        text = (r.stdout + r.stderr).strip()
        try:
            return r.returncode, json.loads(r.stdout)
        except ValueError:
            return r.returncode, text


def stack_item(stack):
    return None if not stack or stack.get('item') in (None, 'minecraft:air') else str(stack['item']).removeprefix('minecraft:')


# Checks

MIB = 1 << 20
SIZE = {'bytes': 1, 'KB': 1 << 10, 'MB': MIB, 'GB': 1 << 30}


def histogram_total(text):
    """Bytes of the `Total` line of a GC.class_histogram, or None."""
    found = re.search(r'^Total\s+\d+\s+(\d+)\s*$', text, re.M)
    return int(found.group(1)) if found else None


def metaspace_used(text):
    """Bytes of the first `Both:` line of VM.metaspace that says `used`, or None."""
    found = re.search(r'^\s*Both:.*?([\d.]+) (bytes|KB|MB|GB) \(\s*[<>]?\s*\d+%\) used', text, re.M)
    return int(float(found.group(1)) * SIZE[found.group(2)]) if found else None


def game_pid(marker):
    """The pid of the game marked by HZ_SCENARIO_MARKER (argv[0] ends in /horizon-client: horizon-ctl's argv names it too)."""
    needle = b'HZ_SCENARIO_MARKER=' + str(marker).encode() + b'\0'
    for proc in Path('/proc').iterdir():
        if proc.name.isdigit():
            try:
                argv0 = (proc / 'cmdline').read_bytes().split(b'\0')[0]
                if argv0.endswith(b'/horizon-client') and needle in (proc / 'environ').read_bytes():
                    return int(proc.name)
            except OSError:
                pass
    return None


def jcmd(java, pid, *command):
    return subprocess.run([f'{java}/bin/jcmd', str(pid), *command], capture_output=True, text=True, timeout=120).stdout


def read_java_heap(ctl, check):
    """(ok, want, got) from the game JVM's forced-full-GC histogram and Metaspace, once."""
    pid = game_pid(ctl.marker)
    want = {k: check[k] for k in ('max_live_mb', 'max_metaspace_mb') if k in check}
    if pid is None:
        return False, want, 'no game process'
    live, meta = histogram_total(jcmd(ctl.java, pid, 'GC.class_histogram')), metaspace_used(jcmd(ctl.java, pid, 'VM.metaspace'))
    if live is None or meta is None:
        return False, want, 'jcmd gave no total or metaspace'
    got = {'live_mb': round(live / MIB), 'metaspace_mb': round(meta / MIB)}
    ok = live <= check.get('max_live_mb', float('inf')) * MIB and meta <= check.get('max_metaspace_mb', float('inf')) * MIB
    return ok, want, got


def read_check(ctl, check, timeout):
    """(ok, want, got) from the game's state, once."""
    if 'block' in check:
        code, data = ctl('block', *map(str, check['block']), timeout=timeout)
        if code != 0 or not isinstance(data, dict):
            return False, check.get('is'), f'ctl: {str(data)[:120]}'
        if 'state' in check:
            return str(data.get('state', '')).removeprefix('minecraft:') == check['state'].removeprefix('minecraft:'), check['state'], data.get('state')
        want = check['is'].removeprefix('minecraft:')
        return str(data.get('block', '')).removeprefix('minecraft:') == want, want, str(data.get('block')).removeprefix('minecraft:')
    if 'looking_at' in check:
        code, data = ctl('looking-at', timeout=timeout)
        target = data.get('target', data) if isinstance(data, dict) else None
        got = target.get('block') if isinstance(target, dict) else None
        got = got.get('block', got.get('name')) if isinstance(got, dict) else got
        want = check['looking_at'].removeprefix('minecraft:')
        return want == str(got).removeprefix('minecraft:'), want, str(got)[:80]
    if 'held' in check or 'slot' in check:
        code, data = ctl('inventory', timeout=timeout)
        if code != 0 or not isinstance(data, dict):
            return False, check.get('held') or check.get('item'), f'ctl: {str(data)[:120]}'
        if 'held' in check:
            want, got = None if check['held'] in (None, 'air') else check['held'].removeprefix('minecraft:'), stack_item(data.get('held'))
            return want == got, want, got
        slot = check['slot']
        stack = (data['hotbar'] + data['main'])[slot]
        want, got = (None if check.get('item') in (None, 'air') else check['item'].removeprefix('minecraft:')), stack_item(stack)
        if 'count' in check:
            return want == got and stack.get('count') == check['count'], [want, check['count']], [got, stack.get('count')]
        return want == got, want, got
    if 'near' in check:
        code, data = ctl('state', timeout=timeout)
        player = data.get('player') if isinstance(data, dict) else None
        if code != 0 or not player:
            return False, check['near'], 'no player'
        got = player['position']
        distance = sum((a - b) ** 2 for a, b in zip(got, check['near'])) ** 0.5
        radius = check.get('radius', 2)
        return distance <= radius, f'{check["near"]} within {radius}', f'{[round(v, 1) for v in got]} ({distance:.1f} away)'
    if 'java_heap' in check:
        return read_java_heap(ctl, check['java_heap'])
    raise ValueError(f'unknown check {sorted(check)}')


def evaluate(check, ctl, log, last_output, timeout, sleep=time.sleep):
    """One check as {"ok", "want", "got"}; state checks keep trying for `within` seconds."""
    if 'output' in check or 'log' in check or 'no_exceptions' in check:
        within = check.get('within', 3 if check.get('present', 'no_exceptions' not in check) else 0)
    else:
        within = check.get('within', 3)
    end = time.monotonic() + within
    while True:
        if 'output' in check:
            text, pattern = '\n'.join(last_output), check['output']
        elif 'log' in check:
            text, pattern = log.text(check.get('since', 'step')), check['log']
        elif 'no_exceptions' in check:
            heads = exception_heads(log.text(check.get('since', 'step')), check.get('ignore'))
            result = (not heads, 'no exceptions', merged_heads('\n'.join(heads)) or 'none')
        else:
            result = read_check(ctl, check, timeout)
        if 'output' in check or 'log' in check:
            present = check.get('present', True)
            found = re.search(pattern, text)
            result = (bool(found) == present, ('' if present else 'no ') + f'match of {pattern}', 'matched' if found else 'no match')
        if result[0] or time.monotonic() >= end:
            return {'ok': bool(result[0]), 'want': result[1], 'got': result[2]}
        sleep(0.25)


# The run

def run_steps(ctl, steps, out, log, deadline, step_timeout=120, sleep=time.sleep):
    """Each step's result; stops at the first failure, and at the whole-run deadline."""
    results, last_output = [], []
    for i, (name, kind, arg) in enumerate(steps):
        started = time.monotonic()
        entry = {'i': i, 'name': name, 'ok': True, 'checks': []}
        remaining = deadline - started
        timeout = min(step_timeout, remaining)
        try:
            if remaining <= 0:
                entry.update(ok=False, error='run timeout')
            elif kind == 'check':
                entry['checks'] = [dict(evaluate(arg, ctl, log, last_output, timeout, sleep), check=next(k for k in ('block', 'looking_at', 'held', 'slot', 'near', 'output', 'log', 'no_exceptions', 'java_heap') if k in arg))]
                entry['ok'] = all(c['ok'] for c in entry['checks'])
            elif kind == 'screenshot':
                path = Path(out) / f'{re.sub(r"[^\w.-]", "_", str(arg) if isinstance(arg, str) else name)}.png'
                code, data = ctl('screenshot', str(path), timeout=timeout)
                if code != 0:
                    entry.update(ok=False, error=f'screenshot: {str(data)[:200]}')
                else:
                    entry['path'] = str(path)
            else:
                log.mark()
                for command in ctl_commands(kind, arg):
                    wait = max(step_timeout, int(command[1]) / 20 * 4 + 30) if command[0] == 'wait' else step_timeout
                    code, data = ctl(*command, timeout=max(0.5, min(wait, deadline - time.monotonic())))
                    if code != 0:
                        entry.update(ok=False, error=('timeout' if code is None else str(data)[:200]) + f' in {command[0]}')
                        break
                    if kind == 'command' and isinstance(data, dict):
                        last_output = [str(line) for line in data.get('chat') or []]
        except Exception as e:
            entry.update(ok=False, error=f'{type(e).__name__}: {e}'[:200])
        entry['ms'] = int((time.monotonic() - started) * 1000)
        results.append(entry)
        if not entry['ok']:
            break
    return results


def result_of(results, log, out, extra=None):
    failed = next(({k: s[k] for k in ('i', 'name', 'error') if k in s} for s in results if not s['ok']), None)
    result = {'ok': failed is None, 'steps': results, 'failed': failed, 'errors': summarize_errors(log.text('start')),
              'log': str(log.path), 'screenshots': [s['path'] for s in results if 'path' in s]}
    result.update(extra or {})
    (Path(out) / 'result.json').write_text(json.dumps(result, indent=1) + '\n')
    return result


def execute(ctl, steps, out, log, timeout=600, step_timeout=120, cleanup=lambda: None, sleep=time.sleep):
    """Runs the steps and always runs cleanup; the result even when a step blows up."""
    try:
        results = run_steps(ctl, steps, out, log, time.monotonic() + timeout, step_timeout, sleep)
    except BaseException as e:
        results = [{'i': -1, 'name': 'run', 'ok': False, 'checks': [], 'error': f'{type(e).__name__}: {e}'[:200], 'ms': 0}]
    finally:
        cleanup()
    return result_of(results, log, out)


# Setting up and tearing down a game

def gfx_env(path=GFX_ENV):
    env = {}
    if path and Path(path).is_file():
        for line in Path(path).read_text().splitlines():
            if '=' in line:
                key, value = line.split('=', 1)
                env[key] = value
    return env


def kill_marked(marker):
    """Kills every process whose environment carries the marker (copied from mod_parity.py, which another agent owns)."""
    needle = b'HZ_SCENARIO_MARKER=' + str(marker).encode() + b'\0'
    for proc in Path('/proc').iterdir():
        if proc.name.isdigit() and int(proc.name) != os.getpid():
            try:
                if needle in (proc / 'environ').read_bytes():
                    os.kill(int(proc.name), signal.SIGKILL)
            except OSError:
                pass


def acquire_gate(owner, wait=10800):
    if not GATE:
        return True
    env = dict(os.environ, GATE_OWNER=owner)
    end = time.monotonic() + wait
    while time.monotonic() < end:
        if subprocess.run(shlex.split(GATE), env=env, capture_output=True).returncode == 0:
            return True
        time.sleep(30)
    return False


def scratch_install(install, out, jars, fresh):
    """OUT/install (the install's libraries and mods linked, host jars and client config copied) and OUT/home; the install itself is never written."""
    install, out = Path(install).resolve(), Path(out)
    scratch = out / 'install'
    scratch.mkdir(parents=True)
    for name in ('libraries', 'mods', 'assets-index'):
        if (install / name).exists():
            (scratch / name).symlink_to((install / name).resolve())
    shutil.copytree(install / 'host', scratch / 'host')
    if jars:
        for jar in Path(jars).glob('horizon-java-*.jar'):
            shutil.copy(jar, scratch / 'host' / jar.name)
    shutil.copy(install / 'install.json', scratch / 'install.json')
    shutil.copytree(install / 'client', scratch / 'client', symlinks=True)
    homes = out / 'home'
    source = install / 'home'
    if source.is_dir() and not fresh:
        shutil.copytree(source, homes, ignore=shutil.ignore_patterns('logs', 'session.lock', '*.lock', 'ctl.sock'))
    for name in ('horizon', 'launcher', 'xdg-data', 'xdg-config'):
        (homes / name).mkdir(parents=True, exist_ok=True)
    return scratch, homes


def run(args):
    out = Path(args.out).resolve()
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    steps = flatten(json.loads(Path(args.script).read_text()))
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import java_mods as jm
    import lab
    game = Path(args.game or ROOT / 'target' / 'fast' / 'horizon-client').resolve()
    if not game.is_file():
        sys.exit(f'{game} is not a file: pass --game')
    try:
        os.nice(10)
    except OSError:
        pass
    scratch, homes = scratch_install(args.install, out, args.jars, args.fresh)
    record = json.loads((scratch / 'install.json').read_text())
    saves = homes / 'horizon' / 'saves'
    world = args.world or next((p.name for p in sorted(saves.glob('*')) if (p / 'level.dat').is_file()), 'scenario')
    extra = {'HORIZON_JAVA_MODS_DIR': str(args.jars)} if args.jars else {}
    base = lab.game_env({**os.environ, **gfx_env()}, TMPDIR=out, HZ_SCENARIO_MARKER=out, HORIZON_HOME=homes / 'horizon', HORIZON_LAUNCHER_HOME=homes / 'launcher',
                        XDG_DATA_HOME=homes / 'xdg-data', XDG_CONFIG_HOME=homes / 'xdg-config', **extra)
    env = jm.launch_env(scratch, record, False, base)
    ctl = Ctl(game.parent / 'horizon-ctl', out / 'ctl.sock', env, out, record['java'], out)
    log = Logs(homes / 'horizon' / 'logs' / 'latest.log', out / 'ctl.log')
    if not acquire_gate(os.environ.get('GATE_OWNER', 'scenario')):
        return result_of([{'i': -1, 'name': 'gate', 'ok': False, 'checks': [], 'error': 'the gate never opened', 'ms': 0}], log, out)
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))
    started, keep, result = time.monotonic(), False, None

    size = [x for v, flag in zip(args.size.split('x'), ('--width', '--height')) for x in (flag, v)] if args.size else []

    def launch():
        new = () if (saves / world / 'level.dat').is_file() else ('--creative', '--seed', '1')
        code, data = ctl('launch', '--world', world, *new, '--offscreen', '--wait', *size, '--resources', record['resources'], '--game', str(game),
                         timeout=min(300, args.timeout))
        (out / 'launch.log').write_text(data if isinstance(data, str) else json.dumps(data))
        return code, data

    def cleanup():
        if not keep:
            ctl('quit', timeout=20)
            time.sleep(3)
            kill_marked(out)

    try:
        code, data = launch()
        if code != 0:
            result = result_of([{'i': -1, 'name': 'launch', 'ok': False, 'checks': [], 'error': ('timeout' if code is None else str(data)[-250:]), 'ms': int((time.monotonic() - started) * 1000)}], log, out)
        else:
            remaining = args.timeout - (time.monotonic() - started)
            result = execute(ctl, steps, out, log, remaining, args.step_timeout, lambda: None)
            keep = args.keep and result['ok']
            if keep:
                result['keep'] = {'socket': str(out / 'ctl.sock')}
                (out / 'result.json').write_text(json.dumps(result, indent=1) + '\n')
    finally:
        cleanup()
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest='command', required=True)
    r = sub.add_parser('run')
    r.add_argument('--install', required=True, help='read only: its home and client config are copied')
    r.add_argument('--script', required=True)
    r.add_argument('--out', required=True)
    r.add_argument('--game', help='a horizon-client binary built with java-mods; horizon-ctl is taken from beside it')
    r.add_argument('--jars', help='the built horizon-java-*.jar (HORIZON_JAVA_MODS_DIR)')
    r.add_argument('--world', help='a world of the copied home (default: its first), made if missing')
    r.add_argument('--size', help='window size WxH')
    r.add_argument('--fresh', action='store_true', help='start with an empty home')
    r.add_argument('--keep', action='store_true', help='on success leave the game running and print its ctl socket')
    r.add_argument('--timeout', type=int, default=600, help='seconds for the whole run, launch included')
    r.add_argument('--step-timeout', type=int, default=120)
    args = parser.parse_args()
    result = run(args)
    print(json.dumps(result))
    sys.exit(0 if result['ok'] else 1)


if __name__ == '__main__':
    main()
