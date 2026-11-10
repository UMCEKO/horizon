#!/usr/bin/env python3
"""Mod coverage: which events and mixin handlers run on a real NeoForge client and on Horizon, step by step through one scripted session.

    mod_parity.py mixins --install DIR --out OUT         the static list of every mixin handler of the install's mods
    mod_parity.py compare --forge F --horizon H --mixins M --out OUT
    mod_parity.py explain --run DIR    adds "Where to fix" to a finished run's report: vanilla callers of the gaps and Horizon's place for each
    mod_parity.py run --install DIR --scenario basic --out OUT   Parts 1 to 4: real client, then Horizon, then the comparison
"""
import argparse
import collections
import datetime
import gzip
import io
import json
import os
import re
import shlex
import shutil
import signal
import subprocess
import sys
import tempfile
import time
import tomllib
import zipfile
from pathlib import Path

os.environ.setdefault('HORIZON_NEOFORGE', '26.3.0.101')
ROOT = Path(__file__).resolve().parent.parent
PARITY = ROOT / 'mods' / 'horizon' / 'src' / 'java-mods-host' / 'parity'
SCENARIOS = Path(__file__).resolve().parent / 'parity_scenarios'
TOLERANCE = 0.2
SLACK = 2
SIDES = ('client', 'server', 'other')
ACCESS = ('accessor', 'invoker')
_CLIENT_EVENT = 'net.neoforged.neoforge.client.event.'
FRAME_EVENTS = frozenset([_CLIENT_EVENT + 'ViewportEvent$' + n for n in ('ComputeCameraAngles', 'ComputeFov', 'ComputeFogColor', 'RenderFog')]
                         + [_CLIENT_EVENT + 'CalculatePlayerTurnEvent', _CLIENT_EVENT + 'CalculateDetachedCameraDistanceEvent'])


# Part 1: the static mixin list

def lenient_json(text):
    try:
        return json.loads(text)
    except ValueError:
        return json.loads(re.sub(r'^\s*//.*$', '', text, flags=re.M))


def jar_mixin_classes(archive, owner=None, label='', warnings=None):
    """Every (mod, config, side, class name, class bytes) of the mixin configs a jar declares, and of the jars nested in it."""
    warnings = [] if warnings is None else warnings
    found = []
    names = set(archive.namelist())
    toml = tomllib.loads(archive.read('META-INF/neoforge.mods.toml').decode()) if 'META-INF/neoforge.mods.toml' in names else {}
    mods = [m.get('modId') for m in toml.get('mods', []) if m.get('modId')]
    mod = mods[0] if mods else owner or label
    configs = [m['config'] for m in toml.get('mixins', []) if 'config' in m]
    if 'META-INF/MANIFEST.MF' in names:
        for line in archive.read('META-INF/MANIFEST.MF').decode().replace('\r\n ', '').splitlines():
            if line.startswith('MixinConfigs:'):
                configs += [c.strip() for c in line.split(':', 1)[1].split(',') if c.strip()]
    for config in dict.fromkeys(configs):
        if config not in names:
            warnings.append(f'{mod}: config {config} is not in the jar')
            continue
        data = lenient_json(archive.read(config).decode())
        if data.get('plugin'):
            warnings.append(f'{mod}: {config} has a config plugin ({data["plugin"]}) that may drop or add mixins at runtime')
        package = data.get('package', '')
        for key, side in (('mixins', 'common'), ('client', 'client'), ('server', 'server')):
            for entry in data.get(key, []):
                name = f'{package}.{entry}' if package else entry
                path = name.replace('.', '/') + '.class'
                if path in names:
                    found.append((mod, config, side, name, archive.read(path)))
                else:
                    warnings.append(f'{mod}: {config} lists {name}, which is not in the jar')
    for name in sorted(names):
        if name.startswith('META-INF/jarjar/') and name.endswith('.jar'):
            with zipfile.ZipFile(io.BytesIO(archive.read(name))) as nested:
                found += jar_mixin_classes(nested, mod, name, warnings)
    return found


def asm_jars(install):
    """The newest ASM and ASM tree jars among the install's libraries."""
    jars = []
    for name in ('asm', 'asm-tree'):
        found = sorted((Path(install) / 'libraries' / 'org' / 'ow2' / 'asm' / name).glob(f'*/{name}-*.jar'))
        if not found:
            sys.exit(f'{install} has no {name} jar in its libraries')
        jars.append(found[-1])
    return jars


def java_of(install):
    record = Path(install) / 'install.json'
    if record.is_file():
        return json.loads(record.read_text())['java']
    return os.environ.get('JAVA_HOME') or sys.exit('no JDK: the install has no install.json and JAVA_HOME is unset')


def compile_scanner(java, classpath, out):
    classes = Path(out) / 'scanner'
    shutil.rmtree(classes, ignore_errors=True)
    classes.mkdir(parents=True)
    sources = sorted((PARITY / 'scan').rglob('MixinScanner.java'))
    subprocess.run([f'{java}/bin/javac', '--release', '25', '-proc:none', '-nowarn', '-cp', ':'.join(map(str, classpath)), '-d', str(classes), *map(str, sources)], check=True)
    return classes


def parse_scan(text):
    entries = []
    for line in text.splitlines():
        mod, config, side, mixin, handler, kind, targets, methods, at = line.split('\t')
        entries.append({'mod': mod, 'config': config, 'mixin': mixin, 'handler': handler, 'kind': kind,
                        'target_class': targets.split(',') if targets else [], 'target_method': methods.split(',') if methods else [],
                        'at': at.split('|') if at else [], 'side': side, 'access': kind in ACCESS})
    return entries


def scan_install(install, out):
    """The mixin handlers of every mod of the install, each as one entry (mixins.json)."""
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    java = java_of(install)
    warnings = []
    found = []
    for jar in sorted((Path(install) / 'mods').glob('*.jar')):
        with zipfile.ZipFile(jar) as archive:
            found += jar_mixin_classes(archive, None, jar.stem, warnings)
    classpath = asm_jars(install)
    scanner = compile_scanner(java, classpath, out)
    with tempfile.TemporaryDirectory(dir=out) as work:
        lines = []
        for i, (mod, config, side, name, data) in enumerate(found):
            path = Path(work) / f'{i}.class'
            path.write_bytes(data)
            lines.append(f'{mod}\t{config}\t{side}\t{name}\t{path}')
        manifest = Path(work) / 'manifest.tsv'
        manifest.write_text('\n'.join(lines) + '\n')
        scan = subprocess.run([f'{java}/bin/java', '-cp', ':'.join(map(str, [scanner, *classpath])), 'dev.horizonlab.javamods.parity.MixinScanner', str(manifest)],
                              check=True, capture_output=True, text=True)
    entries = parse_scan(scan.stdout)
    (out / 'mixins.json').write_text(json.dumps(entries, indent=1) + '\n')
    (out / 'mixins.md').write_text(mixins_markdown(entries, len(found), warnings))
    return entries


def mixins_markdown(entries, classes, warnings):
    by_mod = collections.defaultdict(collections.Counter)
    for e in entries:
        by_mod[e['mod']]['access' if e['access'] else 'behaviour'] += 1
        by_mod[e['mod']][e['kind']] += 1
    kinds = sorted({e['kind'] for e in entries})
    lines = [f'# Mixin handlers: {len(entries)} in {classes} mixin classes', '', '| mod | behaviour | access | ' + ' | '.join(kinds) + ' |',
             '|---|---|---|' + '---|' * len(kinds)]
    for mod, c in sorted(by_mod.items()):
        lines.append(f'| {mod} | {c["behaviour"]} | {c["access"]} | ' + ' | '.join(str(c[k]) for k in kinds) + ' |')
    targets = collections.Counter(t for e in entries if not e['access'] for t in e['target_class'])
    lines += ['', '## Top target classes (behaviour handlers)', '', '| class | handlers |', '|---|---|']
    lines += [f'| {t} | {n} |' for t, n in targets.most_common(25)]
    if warnings:
        lines += ['', '## Notes', ''] + [f'- {w}' for w in warnings]
    return '\n'.join(lines) + '\n'


# Part 2: reading a JFR recording printed as text

def local_epoch(text):
    """Seconds since the epoch of `jfr print`'s `HH:MM:SS.mmm (YYYY-MM-DD)` (local time)."""
    m = re.match(r'(\d+):(\d+):(\d+)\.(\d+) \((\d+)-(\d+)-(\d+)\)', text.strip())
    h, mi, s, frac, y, mo, d = m.groups()
    moment = datetime.datetime(int(y), int(mo), int(d), int(h), int(mi), int(s), int((frac + '000000')[:6]))
    return moment.timestamp()


def jfr_events(lines):
    """Each event of `jfr print --stack-depth N` output as a dict: type, time, thread, method, invocations, frames."""
    event = None
    in_stack = False
    for raw in lines:
        line = raw.rstrip('\n')
        if event is None:
            if line.startswith('jdk.') and line.endswith('{'):
                event = {'type': line[:-1].strip(), 'frames': [], 'time': None, 'thread': None, 'method': None, 'invocations': None}
                in_stack = False
            continue
        if line == '}':
            yield event
            event = None
            continue
        stripped = line.strip()
        if in_stack:
            if stripped == ']':
                in_stack = False
            elif stripped != '...':
                event['frames'].append(stripped)
        elif stripped.startswith('startTime = '):
            event['time'] = local_epoch(stripped[12:])
        elif stripped.startswith('method = '):
            event['method'] = stripped[9:]
        elif stripped.startswith('eventThread = '):
            m = re.match(r'eventThread = "(.*)" \(', stripped)
            event['thread'] = m.group(1) if m else stripped[14:]
        elif stripped.startswith('invocations = '):
            event['invocations'] = int(stripped[14:])
        elif stripped.startswith('stackTrace = ['):
            in_stack = True


def event_class(frames):
    """The most derived Event subclass constructor on top of the stack."""
    leaf = None
    for frame in frames:
        m = re.match(r'(\S+?)\(', frame)
        name = m.group(1) if m else frame
        if not name.endswith('.<init>'):
            break
        cls = name[:-7]
        if 'Event' in cls.rsplit('.', 1)[-1] or leaf is None:
            leaf = cls
    return leaf


def side_of(thread):
    return {'main': 'client', 'Render thread': 'client', 'Server thread': 'server'}.get(thread, 'other')


def timing_target(method):
    """(class, method name) of a MethodTiming `method` such as `net.minecraft.Foo.handler$zbj000$bar(int)`."""
    m = re.match(r'^(.*)\.([^.(]+)\(', method)
    return (m.group(1), m.group(2)) if m else (method, '')


def original_name(merged):
    """The handler name Mixin renamed: `handler$zbj000$onHurt` was `onHurt`."""
    m = re.match(r'^[A-Za-z]+\$[a-z][a-z0-9]{2}\d{3}\$(.+)$', merged)
    return m.group(1) if m else merged


def read_steps(path):
    """[(index label, name, epoch seconds)] from steps.txt: `step <i> <name> <nanoTime> <epoch nanos>`."""
    steps = []
    for line in Path(path).read_text().splitlines():
        parts = line.split()
        if len(parts) >= 5 and parts[0] == 'step':
            steps.append((parts[1], ' '.join(parts[2:-2]), int(parts[-1]) / 1e9))
    return steps


def read_merged(path):
    """{(target class, merged method name): mixin class} from the processor's log."""
    merged = {}
    if path and Path(path).is_file():
        for line in Path(path).read_text().splitlines():
            cls, name, _, mixin = line.split('\t')
            merged[(cls, name)] = mixin.replace('/', '.')
    return merged


def step_of(times, t):
    """-1 before the first step starts, else the index of the step running at t (the last one is `end`)."""
    index = -1
    for i, start in enumerate(times):
        if t >= start:
            index = i
    return index


def aggregate(events, steps, merged):
    """Per step (index 0 is everything before the scenario's first step): event counts per side, and mixin handler deltas."""
    names = ['pre'] + [f'{i} {name}' for i, name, _ in steps if name != 'end']
    times = [t for _, _, t in steps]
    n = len(names)
    counts = [{s: collections.Counter() for s in SIDES} for _ in range(n)]
    firsts = [{s: {} for s in SIDES} for _ in range(n)]
    snapshots = collections.defaultdict(dict)
    for e in events:
        if e['type'] == 'jdk.MethodTrace':
            cls = event_class(e['frames'])
            if cls is None:
                continue
            step = min(step_of(times, e['time']) + 1, n - 1)
            side = side_of(e['thread'])
            counts[step][side][cls] += 1
            firsts[step][side].setdefault(cls, e['time'])
        elif e['type'] == 'jdk.MethodTiming':
            at, key = step_of(times, e['time']), timing_target(e['method'])
            snapshots[at][key] = max(snapshots[at].get(key, 0), e['invocations'])
    handlers = [collections.Counter() for _ in range(n)]
    unmapped = collections.Counter()
    carried = {}
    for boundary in range(-1, len(times)):
        current = dict(carried)
        for key, value in snapshots.get(boundary, {}).items():
            current[key] = max(current.get(key, 0), value)
        target = min(max(boundary, 0), n - 1)
        for key, value in current.items():
            delta = value - carried.get(key, 0)
            if delta <= 0:
                continue
            mixin = merged.get(key)
            if mixin is None:
                unmapped[f'{key[0]}.{key[1]}'] += delta
            else:
                handlers[target][f'{mixin}#{original_name(key[1])}'] += delta
        carried = current
    return {'steps': names,
            'events': [{s: {'counts': dict(counts[i][s]), 'order': sorted(firsts[i][s], key=firsts[i][s].get)} for s in SIDES} for i in range(n)],
            'handlers': [dict(h) for h in handlers], 'unmapped': dict(unmapped)}


def load_run(path, java=None):
    """A run's aggregate: its aggregate.json, or built from the folder's steps.txt, merged.txt and recording."""
    path = Path(path)
    if path.is_file():
        return json.loads(path.read_text())
    cached = path / 'aggregate.json'
    if cached.is_file():
        return json.loads(cached.read_text())
    return build_aggregate(path, java)


def build_aggregate(folder, java=None, recording=None):
    folder = Path(folder)
    java = java or os.environ.get('JAVA_HOME')
    recording = recording or next(folder.glob('*.jfr'))
    printer = subprocess.Popen([f'{java}/bin/jfr', 'print', '--events', 'jdk.MethodTrace,jdk.MethodTiming', '--stack-depth', '24', str(recording)],
                               stdout=subprocess.PIPE, text=True, errors='replace')
    try:
        result = aggregate(jfr_events(printer.stdout), read_steps(folder / 'steps.txt'), read_merged(folder / 'merged.txt'))
    finally:
        printer.kill()
        printer.wait()
    (folder / 'aggregate.json').write_text(json.dumps(result, indent=1) + '\n')
    return result


# Part 4: the comparison

def mismatch(forge, horizon):
    return abs(horizon - forge) > TOLERANCE * forge + SLACK


def frame_listened_of(install):
    """The frame events some mod listens to, from the install's replaces.json; all of them (strict) without that file."""
    path = Path(install) / 'analysis' / 'replaces.json' if install else None
    if not path or not path.is_file():
        return set(FRAME_EVENTS)
    events = json.loads(path.read_text()).get('events', {})
    return {key.replace('/', '.') for key, value in events.items() if value.get('changes')} & FRAME_EVENTS


def compare_events(forge, horizon, frame_listened=None):
    frame_listened = set(FRAME_EVENTS if frame_listened is None else frame_listened)
    steps = forge['steps']
    totals = {side: [collections.Counter(), collections.Counter()] for side in SIDES}
    first_seen = {}
    counts, orders = [], []
    for i, name in enumerate(steps):
        for side in SIDES:
            f = forge['events'][i][side] if i < len(forge['events']) else {'counts': {}, 'order': []}
            h = horizon['events'][i][side] if i < len(horizon['events']) else {'counts': {}, 'order': []}
            for cls, n in f['counts'].items():
                totals[side][0][cls] += n
                first_seen.setdefault((side, cls), name)
            for cls, n in h['counts'].items():
                totals[side][1][cls] += n
            for cls in sorted(set(f['counts']) | set(h['counts'])):
                a, b = f['counts'].get(cls, 0), h['counts'].get(cls, 0)
                if cls not in FRAME_EVENTS and mismatch(a, b):
                    counts.append({'step': name, 'side': side, 'event': cls, 'forge': a, 'horizon': b})
            common = [c for c in f['order'] if c in h['counts']]
            other = [c for c in h['order'] if c in f['counts']]
            if common != other:
                orders.append({'step': name, 'side': side, 'forge': common, 'horizon': other})
    forge_all, horizon_all = collections.Counter(), collections.Counter()
    for side in SIDES:
        forge_all.update(totals[side][0])
        horizon_all.update(totals[side][1])
    sides = collections.defaultdict(set)
    for (side, cls) in first_seen:
        sides[cls].add(side)
    first = {}
    for (side, cls), step in first_seen.items():
        first.setdefault(cls, step)
    missing = [{'event': c, 'count': n, 'sides': sorted(sides[c]), 'first_step': first[c]} for c, n in forge_all.items()
               if c not in horizon_all and (c not in FRAME_EVENTS or c in frame_listened)]
    frame_unlistened = [{'event': c, 'count': n} for c, n in sorted(forge_all.items()) if c in FRAME_EVENTS and c not in horizon_all and c not in frame_listened]
    frame_present = sorted(c for c in FRAME_EVENTS if c in forge_all and c in horizon_all)
    extra = [{'event': c, 'count': n} for c, n in horizon_all.items() if c not in forge_all]
    missing.sort(key=lambda m: -m['count'])
    extra.sort(key=lambda m: -m['count'])
    return {'missing': missing, 'extra': extra, 'count_mismatches': counts, 'order_differences': orders,
            'frame_unlistened': frame_unlistened, 'frame_present': frame_present}


def resolve_handler(known, mixin, name):
    """Mixin may prefix a handler's name (`ae2$onHurt` for `onHurt`): the entry's name is the longest suffix after a `$` that the mixin declares."""
    while name not in known.get(mixin, ()) and '$' in name:
        name = name.split('$', 1)[1]
    return name


def handler_counts(run, known):
    total = collections.Counter()
    for step in run['handlers']:
        for key, n in step.items():
            mixin, name = key.split('#', 1)
            total[f'{mixin}#{resolve_handler(known, mixin, name)}'] += n
    return total


def compare_mixins(forge, horizon, mixins):
    known = collections.defaultdict(set)
    for e in mixins:
        known[e['mixin']].add(e['handler'])
    f, h = handler_counts(forge, known), handler_counts(horizon, known)
    rows = []
    for entry in mixins:
        key = f'{entry["mixin"]}#{entry["handler"]}'
        a, b = f.get(key, 0), h.get(key, 0)
        state = 'both' if a and b else 'dead in Horizon' if a else 'Horizon only' if b else 'not exercised'
        rows.append({'mod': entry['mod'], 'mixin': entry['mixin'], 'handler': entry['handler'], 'kind': entry['kind'], 'access': entry['access'],
                     'target_class': entry['target_class'], 'target_method': entry['target_method'], 'side': entry['side'],
                     'forge': a, 'horizon': b, 'state': state})
    names = {f'{e["mixin"]}#{e["handler"]}' for e in mixins}
    other = {'forge': sum(n for k, n in f.items() if k not in names), 'horizon': sum(n for k, n in h.items() if k not in names)}
    return rows, other


def compare(forge, horizon, mixins, frame_listened=None):
    rows, other = compare_mixins(forge, horizon, mixins)
    return {'events': compare_events(forge, horizon, frame_listened), 'mixins': rows, 'other_merged_calls': other, 'steps': forge['steps']}


def table(headers, rows):
    lines = ['| ' + ' | '.join(headers) + ' |', '|' + '---|' * len(headers)]
    return lines + ['| ' + ' | '.join(str(c) for c in row) + ' |' for row in rows]


def report_markdown(result, forge, horizon):
    ev, rows = result['events'], result['mixins']
    behaviour = [r for r in rows if not r['access']]
    states = collections.Counter(r['state'] for r in behaviour)
    lines = ['# Mod parity report', '',
             *table(['', 'count'], [['event classes missing in Horizon', len(ev['missing'])], ['event classes extra in Horizon', len(ev['extra'])],
                                    ['event count mismatches (step, side, class)', len(ev['count_mismatches'])], ['event order differences (step, side)', len(ev['order_differences'])],
                                    ['mixin handlers: both', states['both']], ['mixin handlers: dead in Horizon', states['dead in Horizon']],
                                    ['mixin handlers: Horizon only', states['Horizon only']], ['mixin handlers: not exercised', states['not exercised']]]),
             '', f'Steps: {", ".join(result["steps"])}', '', '## Missing events (fired on the real client, never in Horizon)', '']
    lines += table(['event', 'side', 'count', 'first step'], [[m['event'], ','.join(m['sides']), m['count'], m['first_step']] for m in ev['missing']]) or ['none']
    frame = [f'{c.rsplit(".", 1)[-1]} {"present" if c in ev["frame_present"] else "not fired (no listener)" if any(u["event"] == c for u in ev["frame_unlistened"]) else "missing (listened)"}'
             for c in sorted(FRAME_EVENTS) if c in ev['frame_present'] or any(m['event'] == c for m in ev['missing']) or any(u['event'] == c for u in ev['frame_unlistened'])]
    lines += ['', f'Frame events (by presence, HUD rule): {", ".join(frame) or "none in the run"}']
    lines += ['', '## Extra events (Horizon only)', ''] + table(['event', 'count'], [[m['event'], m['count']] for m in ev['extra']])
    lines += ['', '## Count mismatches', ''] + table(['step', 'side', 'event', 'forge', 'horizon'],
                                                     [[m['step'], m['side'], m['event'], m['forge'], m['horizon']] for m in ev['count_mismatches']])
    lines += ['', '## Order differences (first occurrence within a step)', '']
    for o in ev['order_differences']:
        lines += [f'- {o["step"]} / {o["side"]}', f'  - forge: {", ".join(o["forge"])}', f'  - horizon: {", ".join(o["horizon"])}']
    lines += ['', '## Mixin handlers by target class', '']
    by_target = collections.defaultdict(list)
    for r in behaviour:
        for t in r['target_class'] or ['?']:
            by_target[t].append(r)
    for target in sorted(by_target, key=lambda t: (-sum(r['state'] == 'dead in Horizon' for r in by_target[t]), t)):
        group = by_target[target]
        dead = sum(r['state'] == 'dead in Horizon' for r in group)
        lines += ['', f'### {target} ({len(group)} handlers, {dead} dead in Horizon)', '']
        lines += table(['mod', 'mixin', 'handler', 'kind', 'method', 'forge', 'horizon', 'state'],
                       [[r['mod'], r['mixin'].rsplit('.', 1)[-1], r['handler'], r['kind'], ','.join(r['target_method']), r['forge'], r['horizon'], r['state']]
                        for r in sorted(group, key=lambda r: (r['state'], r['mixin'], r['handler']))])
    lines += ['', f'Merged methods outside the mods\' mixin list (NeoForge, Mixin extras): forge {result["other_merged_calls"]["forge"]} calls, horizon {result["other_merged_calls"]["horizon"]} calls', '']
    return '\n'.join(lines) + '\n'


def write_report(result, forge, horizon, out):
    out = Path(out)
    out.mkdir(parents=True, exist_ok=True)
    (out / 'report.json').write_text(json.dumps(result, indent=1) + '\n')
    (out / 'report.md').write_text(report_markdown(result, forge, horizon))


def explain_module():
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import mod_parity_explain
    return mod_parity_explain


def explain_run(out, install=None, cache=None):
    """Adds the "Where to fix" section to a finished run's report."""
    explain = explain_module()
    explain.write_explanation(out, explain.explain(out, install, cache))


def compare_command(args):
    forge, horizon = load_run(args.forge, args.java), load_run(args.horizon, args.java)
    mixins = json.loads(Path(args.mixins).read_text())
    write_report(compare(forge, horizon, mixins, frame_listened_of(getattr(args, 'install', None))), forge, horizon, args.out)
    print(Path(args.out) / 'report.md')


# Part 3: scenarios and drivers

def load_scenario(name):
    path = Path(name)
    if not path.is_file():
        path = SCENARIOS / f'{name}.json'
    return path, json.loads(path.read_text())


def flatten(scenario):
    """[(name, kind, argument)]: a `name` entry labels the step after it."""
    steps, pending = [], None
    for item in scenario:
        if 'name' in item:
            pending = item['name']
            continue
        kind = next(iter(item))
        steps.append((pending or kind, kind, item[kind]))
        pending = None
    return steps


KEYS = {'key.inventory': 'e', 'key.jump': 'space', 'key.sprint': 'ctrl', 'key.forward': 'w', 'key.togglePerspective': 'f5'}


def ctl_commands(kind, arg):
    """The horizon-ctl lines that do one scenario step; waits are in game ticks."""
    if kind == 'wait':
        return [['wait', str(arg)]]
    if kind == 'command':
        return [['command', arg.lstrip('/')]]
    if kind == 'look':
        return [['look', str(arg[0]), str(arg[1])]]
    if kind == 'slot':
        return [['select', str(arg + 1)]]
    if kind in ('use', 'attack'):
        button = 'right' if kind == 'use' else 'left'
        return [['mouse', button, 'down'], ['wait', str(arg)], ['mouse', button, 'up']]
    if kind == 'key':
        return [['key', KEYS[arg], 'tap']]
    if kind in ('press', 'release'):
        return [['key', KEYS[arg], 'down' if kind == 'press' else 'up']]
    if kind == 'close':
        return [['close']]
    raise ValueError(f'unknown step {kind}')


def acquire_gate(owner):
    """Waits until the command in HORIZON_GAME_GATE exits 0; no gating when it is unset."""
    gate = os.environ.get('HORIZON_GAME_GATE')
    if not gate:
        return
    env = dict(os.environ, GATE_OWNER=owner)
    for _ in range(360):
        if subprocess.run(shlex.split(gate), env=env).returncode == 0:
            return
        time.sleep(30)
    sys.exit('the gate never opened')


def kill_marked(marker):
    """Kills every process whose environment carries the marker (a scratch path unique to this run)."""
    needle = b'PARITY_MARKER=' + marker.encode() + b'\0'
    for proc in Path('/proc').iterdir():
        if proc.name.isdigit() and int(proc.name) != os.getpid():
            try:
                if needle in (proc / 'environ').read_bytes():
                    os.kill(int(proc.name), signal.SIGKILL)
            except (OSError, ProcessLookupError):
                pass


def marked_pids(marker):
    needle = b'PARITY_MARKER=' + marker.encode() + b'\0'
    found = []
    for proc in Path('/proc').iterdir():
        if proc.name.isdigit():
            try:
                if needle in (proc / 'environ').read_bytes() and b'horizon-client' in (proc / 'cmdline').read_bytes():
                    found.append(int(proc.name))
            except OSError:
                pass
    return found


def gfx_env(path=None):
    """KEY=VALUE lines of the file named by HORIZON_GFX_ENV (display and GPU variables for the games), if any."""
    env = {}
    path = path or os.environ.get('HORIZON_GFX_ENV')
    if path and Path(path).is_file():
        for line in Path(path).read_text().splitlines():
            if '=' in line:
                key, value = line.split('=', 1)
                env[key] = value
    return env


def jfr_options(settings, recording, merged, extra=()):
    """The JVM options that record events and handler counts the same way on both sides."""
    return [f'-XX:StartFlightRecording:settings={settings},name=parity,maxsize=2g,filename={recording},dumponexit=true',
            '-XX:FlightRecorderOptions:stackdepth=24', f'-Dhorizon.parity.merged={merged}', *extra]


def build_parity_jars(jars, install):
    """Builds horizon-java-parity.jar (the processor) and horizon-java-parity-mod.jar (the real client's driver) into `jars`."""
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import neoforge_oracle as oracle
    jars = Path(jars)
    jars.mkdir(parents=True, exist_ok=True)
    classpath = ':'.join(j for j in oracle.client_classpath(Path(install)).split(':') if 'installertools' not in j)
    for name, jar in (('lib', 'horizon-java-parity.jar'), ('mod', 'horizon-java-parity-mod.jar')):
        classes = jars / 'classes' / f'parity-{name}'
        shutil.rmtree(classes, ignore_errors=True)
        classes.mkdir(parents=True)
        sources = [str(p) for p in sorted((PARITY / name).rglob('*.java'))]
        subprocess.run(['javac', '--release', '25', '-proc:none', '-nowarn', '-cp', classpath, '-d', str(classes), *sources], check=True)
        manifest = jars / f'parity-{name}.manifest'
        manifest.write_text('FMLModType: LIBRARY\n' if name == 'lib' else 'Manifest-Version: 1.0\n')
        subprocess.run(['jar', 'cfm', str(jars / jar), str(manifest), '-C', str(classes), '.', '-C', str(PARITY / name / 'res'), '.'], check=True)
    return jars / 'horizon-java-parity.jar', jars / 'horizon-java-parity-mod.jar'


def copy_world(source, saves, name):
    shutil.rmtree(Path(saves) / name, ignore_errors=True)
    Path(saves).mkdir(parents=True, exist_ok=True)
    shutil.copytree(source, Path(saves) / name, ignore=shutil.ignore_patterns('session.lock'))
    level = Path(saves) / name / 'level.dat'
    level.write_bytes(allow_commands(level.read_bytes()))


def allow_commands(data):
    """level.dat (gzipped NBT) with its allowCommands byte set, so the scenario's commands run."""
    raw = bytearray(gzip.decompress(data))
    at = raw.find(b'\x01\x00\rallowCommands')
    if at >= 0:
        raw[at + 2 + 1 + 13] = 1
    return gzip.compress(bytes(raw))


def run_real(install, scenario_file, out, world, parity_jars, java, reference, heap='3G', timeout=1500, size=(1280, 720), extra_jvm=()):
    """One real NeoForge client session of the scenario; returns when it quits. Needs an X display (Xvfb) so no window opens on the desktop."""
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import neoforge_oracle as oracle
    install, out = Path(install), Path(out)
    shutil.rmtree(out, ignore_errors=True)
    game = out / 'game'
    (game / 'mods').mkdir(parents=True)
    (game / 'saves').mkdir()
    for jar in sorted((install / 'mods').glob('*.jar')):
        (game / 'mods' / jar.name).symlink_to(jar.resolve())
    for jar in parity_jars:
        (game / 'mods' / jar.name).symlink_to(jar.resolve())
    shutil.copytree(install / 'client' / 'config', game / 'config')
    (game / 'config' / 'fml.toml').write_text('earlyWindowControl = false\n')
    ref = Path(reference)
    shutil.copy(ref / 'game' / 'options.txt', game / 'options.txt')
    copy_world(world, game / 'saves', 'W')
    jvm = ['-Dfml.earlyWindowControl=false', f'-Dhorizon.parity.scenario={scenario_file}', f'-Dhorizon.parity.steps={out / "steps.txt"}',
           f'-Dhorizon.parity.jfr={out / "forge.jfr"}', *jfr_options(PARITY / 'parity.jfc', out / 'forge.jfr', out / 'merged.txt'), *extra_jvm]
    _, cmd = oracle.client_command(f'{java}/bin/java', heap, game, ref / 'natives', ref / 'assets', jvm=jvm,
                                   game=['--width', str(size[0]), '--height', str(size[1]), '--quickPlaySingleplayer', 'W'])
    env = {**os.environ, **gfx_env(), 'ALSOFT_DRIVERS': 'null', 'HORIZON_NEOFORGE': oracle.NEOFORGE, 'JAVA_HOME': java, 'PARITY_MARKER': str(out)}
    acquire_gate('parity')
    log = open(out / 'game.log', 'w')
    display = subprocess.Popen(['Xvfb', ':97', '-screen', '0', f'{size[0]}x{size[1]}x24', '-nolisten', 'tcp'], stderr=subprocess.DEVNULL, stdout=subprocess.DEVNULL, env=env)
    process = None
    try:
        time.sleep(2)
        env.update(DISPLAY=':97', SDL_VIDEODRIVER='x11', LIBGL_ALWAYS_SOFTWARE='1', __GLX_VENDOR_LIBRARY_NAME='mesa')
        env.pop('WAYLAND_DISPLAY', None)
        process = subprocess.Popen(['nice', '-n', '10', *map(str, cmd)], cwd=game, env=env, stdout=log, stderr=subprocess.STDOUT, start_new_session=True)
        started = time.time()
        while process.poll() is None and time.time() - started < timeout:
            time.sleep(2)
    finally:
        if process is not None and process.poll() is None:
            os.killpg(process.pid, signal.SIGKILL)
        kill_marked(str(out))
        display.kill()
        display.wait()
        log.close()
    return process.returncode if process else None


class Ctl:
    def __init__(self, ctl, socket, env, cwd):
        self.base, self.env, self.cwd = [str(ctl), '--socket', str(socket)], env, cwd

    def __call__(self, *args, timeout=300):
        r = subprocess.run([*self.base, *args], env=self.env, cwd=self.cwd, capture_output=True, text=True, timeout=timeout)
        return r.returncode, (r.stdout + r.stderr).strip()


def stamp(out, index, name):
    now = time.time_ns()
    with open(out / 'steps.txt', 'a') as f:
        f.write(f'step {index} {name} {time.monotonic_ns()} {now}\n')


def run_horizon(install, scenario, out, world, parity_jar, host_jars, game_dir, java, timeout=1500):
    """One Horizon session of the scenario through horizon-ctl, in a scratch copy of the install."""
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import java_mods as jm
    install, out = Path(install), Path(out)
    shutil.rmtree(out, ignore_errors=True)
    scratch = out / 'install'
    (scratch / 'host').mkdir(parents=True)
    (scratch / 'libraries').symlink_to((install / 'libraries').resolve())
    (scratch / 'mods').mkdir()
    for jar in sorted((install / 'mods').glob('*.jar')):
        (scratch / 'mods' / jar.name).symlink_to(jar.resolve())
    for name in ('horizon-java-host.jar', 'horizon-java-boot.jar', 'horizon-java-plugin.jar'):
        shutil.copy(Path(host_jars) / name, scratch / 'host' / name)
    shutil.copy(parity_jar, scratch / 'host' / parity_jar.name)
    run = scratch / 'client'
    (run / 'mods').mkdir(parents=True)
    for jar in [*sorted((scratch / 'mods').glob('*.jar')), *sorted((scratch / 'host').glob('*.jar'))]:
        if jar.name != 'horizon-java-host.jar':
            (run / 'mods' / jar.name).symlink_to(jar)
    (run / 'libraries').symlink_to(Path('..') / 'libraries')
    shutil.copytree(install / 'client' / 'config', run / 'config')
    args = (install / 'client' / 'horizon-java.args').read_text().splitlines()
    args += [f'jvm {o}' for o in jfr_options(PARITY / 'parity.jfc', out / 'horizon.jfr', out / 'merged.txt')]
    (run / 'horizon-java.args').write_text('\n'.join(args) + '\n')
    record = json.loads((install / 'install.json').read_text())
    shutil.copy(install / 'install.json', scratch / 'install.json')
    homes = out / 'home'
    for name in ('horizon', 'launcher', 'xdg-data', 'xdg-config'):
        (homes / name).mkdir(parents=True)
    copy_world(world, homes / 'horizon' / 'saves', 'W')
    import lab
    base = lab.game_env({**os.environ, **gfx_env()}, TMPDIR=out, PARITY_MARKER=out, HORIZON_HOME=homes / 'horizon', HORIZON_LAUNCHER_HOME=homes / 'launcher',
                        XDG_DATA_HOME=homes / 'xdg-data', XDG_CONFIG_HOME=homes / 'xdg-config')
    env = jm.launch_env(scratch, record, False, base)
    ctl = Ctl(Path(game_dir) / 'horizon-ctl', out / 'ctl.sock', env, out)
    steps = flatten(scenario)
    acquire_gate('parity')
    try:
        code, text = ctl('launch', '--world', 'W', '--offscreen', '--wait', '--resources', record['resources'], timeout=timeout)
        (out / 'launch.log').write_text(text)
        if code != 0:
            return {'ok': False, 'error': text[-2000:]}
        pids = marked_pids(str(out))
        errors = []

        def rotate():
            for pid in pids:
                r = subprocess.run([f'{java}/bin/jcmd', str(pid), 'JFR.dump', 'name=parity', f'filename={out / "rotate.jfr"}'], capture_output=True, text=True)
                if r.returncode != 0:
                    errors.append(r.stdout + r.stderr)
            (out / 'rotate.jfr').unlink(missing_ok=True)

        for index, (name, kind, arg) in enumerate(steps):
            stamp(out, index, name)
            rotate()
            for command in ctl_commands(kind, arg):
                code, text = ctl(*command)
                if code != 0:
                    errors.append(f'step {index} {command}: {text}')
        stamp(out, len(steps), 'end')
        for pid in pids:
            subprocess.run([f'{java}/bin/jcmd', str(pid), 'JFR.dump', 'name=parity', f'filename={out / "horizon.jfr"}'], capture_output=True, text=True)
        ctl('quit')
        time.sleep(5)
        (out / 'driver-errors.txt').write_text('\n'.join(errors))
        return {'ok': True, 'errors': errors}
    finally:
        kill_marked(str(out))


def run_command(args):
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    install = Path(args.install).resolve()
    java = java_of(install)
    entries = scan_install(install, out)
    (out / 'mixins.json').write_text(json.dumps(entries, indent=1) + '\n')
    scenario_file, scenario = load_scenario(args.scenario)
    jars = Path(args.jars or os.environ.get('HORIZON_JAVA_MODS_DIR') or out / 'jars')
    lib, mod = build_parity_jars(jars, install)
    if not args.skip_real and not args.reference:
        sys.exit('--reference is required for the real client run')
    if not args.skip_horizon and not args.game_dir:
        sys.exit('--game-dir is required for the Horizon run')
    world = Path(args.world)
    if not args.skip_real:
        run_real(install, scenario_file, out / 'forge', world, [lib, mod], java, args.reference)
    forge = build_aggregate(out / 'forge', java, out / 'forge' / 'forge.jfr') if (out / 'forge' / 'forge.jfr').is_file() else None
    if not args.skip_horizon:
        result = run_horizon(install, scenario, out / 'horizon', world, lib, args.host_jars or install / 'host', args.game_dir, java)
        print('horizon:', result)
    horizon = build_aggregate(out / 'horizon', java, out / 'horizon' / 'horizon.jfr') if (out / 'horizon' / 'horizon.jfr').is_file() else None
    if forge and horizon:
        write_report(compare(forge, horizon, entries, frame_listened_of(install)), forge, horizon, out)
        explain_run(out, install)
        print(out / 'report.md')


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest='command', required=True)
    m = sub.add_parser('mixins')
    m.add_argument('--install', required=True)
    m.add_argument('--out', required=True)
    c = sub.add_parser('compare')
    c.add_argument('--forge', required=True)
    c.add_argument('--horizon', required=True)
    c.add_argument('--mixins', required=True)
    c.add_argument('--out', required=True)
    c.add_argument('--install', help='the install whose analysis/replaces.json says which frame events a mod listens to')
    c.add_argument('--java', default=os.environ.get('JAVA_HOME'))
    e = sub.add_parser('explain', help='regenerate the "Where to fix" section of a finished run')
    explain_module().add_arguments(e)
    r = sub.add_parser('run')
    r.add_argument('--install', required=True)
    r.add_argument('--scenario', default='basic')
    r.add_argument('--out', required=True)
    r.add_argument('--world', required=True, help='the world folder both games start from')
    r.add_argument('--reference', help='a real client\'s folder with natives/, assets/ and game/options.txt (needed unless --skip-real)')
    r.add_argument('--jars', help='where the parity jars are built (default HORIZON_JAVA_MODS_DIR)')
    r.add_argument('--host-jars', help='the built horizon-java-{host,boot,plugin}.jar (default: the install\'s)')
    r.add_argument('--game-dir', help='the folder with the built horizon-ctl and horizon-client (needed unless --skip-horizon)')
    r.add_argument('--skip-real', action='store_true')
    r.add_argument('--skip-horizon', action='store_true')
    args = parser.parse_args()
    if args.command == 'mixins':
        scan_install(args.install, args.out)
    elif args.command == 'compare':
        compare_command(args)
    elif args.command == 'explain':
        explain_run(args.run, args.install, args.cache)
        print(Path(args.run) / 'report.md')
    else:
        run_command(args)


if __name__ == '__main__':
    main()
