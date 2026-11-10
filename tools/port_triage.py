#!/usr/bin/env python3
"""Which unported vanilla methods to port, and why: signals per method from the pack's mods, the explain index, a run and the harnesses.

    port_triage.py --install DIR [--run DIR] --out OUT [--ledger ledger.json] [--cache DIR]
    port_triage.py --write-verdicts DECISIONS.tsv [--date YYYY-MM-DD]

Triages the ledger's `absent` methods (verdict `missing` in tools/port_map, class rows expanded). The ledger's own status
`missing` is a different set: methods nobody has decided yet (--unanswered adds them). OUT gets triage.md and triage.json.
Mod jars (jar-in-jar too) are read with javap only: calls to vanilla methods, overrides by name, mixin targets (tools/mod_parity.py).
The suggestion is a rule of thumb, stated in triage.md; the user decides. --write-verdicts turns decisions (method, decision, reason)
into tools/port_map/triage-<date>.tsv. A decision is p1, p2 or p3 (stays `missing`, note says why), skip:<reason>, or deferred (not written).
"""
import argparse
import collections
import concurrent.futures
import datetime
import gzip
import hashlib
import io
import json
import math
import os
import re
import shutil
import subprocess
import sys
import tomllib
import zipfile
from pathlib import Path

try:
    from . import mod_parity as mp, mod_parity_explain as ex
except ImportError:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import mod_parity as mp  # noqa: E402
    import mod_parity_explain as ex  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
MAP_DIR = ROOT / 'tools' / 'port_map'
LEDGER_JSON = Path(os.environ.get('PORT_LEDGER_OUT') or ROOT / '.local' / 'port-ledger') / 'ledger.json'
DEFAULT_CACHE = ROOT / '.local' / 'port-triage'
VANILLA = 'net/minecraft/'
CLASSES = ('P2', 'P1-candidate', 'deferred')
GAMEPLAY = ('world/', 'server/level/', 'server/players/', 'server/commands/', 'commands/', 'network/protocol/', 'network/syncher/', 'advancements/', 'stats/')
READ_ONLY = re.compile(r'^(get|is|has|can|should|test|matches|to|create|codec|equals|hash|find|check|lambda|stream|describe|name|type)', re.I)
SKIP_REASONS = ('java', 'data', 'ui', 'debug', 'service', 'server', 'other', 'java-serves')
DECISIONS = ('p1', 'p2', 'p3', 'deferred')

HEADER = re.compile(r'^(?:[a-z@]+ )*(?:class|interface|enum) ([^\s<{]+)(?:<.*?>)?(?: extends ([^{]+?))?(?: implements ([^{]+?))?\s*\{$')


# Mod jars

def nested_jars(jar, cache, label=None):
    """[(label, path)] of a jar and of every jar nested in META-INF/jarjar, extracted under cache."""
    label = label or Path(jar).stem
    found = [(label, Path(jar))]
    with zipfile.ZipFile(jar) as z:
        for name in sorted(z.namelist()):
            if name.startswith('META-INF/jarjar/') and name.endswith('.jar'):
                data = z.read(name)
                path = Path(cache) / 'nested' / (hashlib.sha256(data).hexdigest()[:20] + '.jar')
                if not path.is_file():
                    path.parent.mkdir(parents=True, exist_ok=True)
                    path.write_bytes(data)
                found += nested_jars(path, cache, f'{label}>{Path(name).stem}')
    return found


def mod_id(jar, fallback):
    """The first mod id of a jar's neoforge.mods.toml, else the label."""
    try:
        with zipfile.ZipFile(jar) as z:
            toml = tomllib.loads(z.read('META-INF/neoforge.mods.toml').decode())
        ids = [m.get('modId') for m in toml.get('mods', []) if m.get('modId')]
        return ids[0] if ids else fallback
    except (KeyError, ValueError, zipfile.BadZipFile):
        return fallback


def mod_jars(install, cache):
    """[(mod, jar)]: each mods/*.jar by its mod id, nested jars by `parent>nested` (they belong to that mod)."""
    out = []
    for jar in sorted((Path(install) / 'mods').glob('*.jar')):
        parts = nested_jars(jar, cache)
        mod = mod_id(jar, jar.stem)
        out += [(mod if i == 0 else f'{mod}{label[len(jar.stem):]}', path) for i, (label, path) in enumerate(parts)]
    return out


def parse_declarations(lines):
    """{class path: {'supers': [paths], 'methods': [names]}} from `javap -p -s` output."""
    out = {}
    current = None
    for line in lines:
        line = line.rstrip('\n')
        if line and not line[0].isspace():
            m = HEADER.match(line)
            current = None
            if m:
                name = m.group(1).replace('.', '/')
                supers = [s.strip().replace('.', '/') for part in (m.group(2), m.group(3)) if part for s in re.split(r',\s*(?![^<]*>)', part)]
                current = out.setdefault(name, {'supers': [re.sub(r'<.*', '', s) for s in supers], 'methods': []})
            continue
        if current is not None and ex.METHOD_LINE.match(line):
            head = line.split('(', 1)[0].split()
            name = head[-1] if head else ''
            if name and '.' not in name and not name.startswith(('lambda$', 'access$')) and name != '<clinit>':
                current['methods'].append(name)
    return out


def declarations(javap, jar, cache):
    """The class declarations of a jar, cached under its content hash."""
    cache = Path(cache)
    path = cache / f'decl-{ex.digest([jar])}.json.gz'
    if path.is_file():
        return json.loads(gzip.decompress(path.read_bytes()))
    names = ex.jar_classes(jar)
    out = {}

    def batch(chunk):
        done = subprocess.run(['nice', javap, '-p', '-s', '-cp', str(jar), *chunk], capture_output=True, text=True, errors='replace')
        return parse_declarations(done.stdout.splitlines())

    with concurrent.futures.ThreadPoolExecutor(6) as pool:
        for part in pool.map(batch, [names[i:i + 400] for i in range(0, len(names), 400)]):
            out.update(part)
    cache.mkdir(parents=True, exist_ok=True)
    path.write_bytes(gzip.compress(json.dumps(out).encode()))
    return out


# The ledger side

class Vanilla:
    """The ledger's classes and methods: where a name resolves to, and which ones are triaged."""

    def __init__(self, ledger, targets):
        self.classes = ledger['classes']
        self.methods = ledger['methods']
        self.targets = targets
        self._order = {}

    def ancestors(self, cls):
        """The class, then its supers breadth first."""
        if cls not in self._order:
            seen, queue = [], [cls]
            while queue:
                c = queue.pop(0)
                if c in seen:
                    continue
                seen.append(c)
                queue += self.classes.get(c, {}).get('extends', [])
            self._order[cls] = seen
        return self._order[cls]

    def declarer(self, cls, name):
        """The key of the nearest vanilla class at or above cls that declares name, or None."""
        for c in self.ancestors(cls):
            k = f'{c}#{name}'
            if k in self.methods:
                return k
        return None

    def target(self, cls, name):
        k = self.declarer(cls, name)
        return k if k in self.targets else None


def display(k):
    """`a/b/Outer$Inner#m` as the verdict map spells it: `Outer.Inner.m`."""
    cls, name = k.split('#')
    return '.'.join(cls.rsplit('/', 1)[-1].split('$')) + '.' + name


def package_of(k):
    path = k.split('#')[0]
    path = path[len(VANILLA):] if path.startswith(VANILLA) else path
    return path.rsplit('/', 1)[0] if '/' in path else ''


def targets_of(ledger, include_unanswered=False):
    """{key: {'status', 'note', 'src'}}: the absent methods, and with include_unanswered the ledger's undecided ones."""
    out = {k: {'status': 'absent', 'note': v.get('note', ''), 'src': v.get('src', '')} for k, v in ledger['absent'].items() if k in ledger['methods']}
    if include_unanswered:
        decided = [ledger.get(d, {}) for d in ('ported', 'mapped', 'skipped', 'absent', 'mentioned', 'named', 'linked', 'structural', 'inferred')]
        for k, m in ledger['methods'].items():
            if any(k in d for d in decided) or m.get('accessor') or m.get('abstract') or m.get('size', 9) <= 3 or m['name'] == '<init>':
                continue
            out[k] = {'status': 'unanswered', 'note': '', 'src': ''}
    return out


# Signals

def calls_signal(vanilla, mod_indexes):
    """{key: {mod: distinct calling methods}} for calls into a target, resolved up to the declaring class."""
    found = collections.defaultdict(lambda: collections.defaultdict(set))
    for mod, index in mod_indexes:
        for target, callers in index['calls'].items():
            if not target.startswith(VANILLA):
                continue
            owner, name = target.rsplit(':', 1)[0].rsplit('.', 1)
            k = vanilla.target(owner, name)
            if k:
                found[k][mod] |= set(callers)
    return {k: {m: len(c) for m, c in mods.items()} for k, mods in found.items()}


def overrides_signal(vanilla, mod_decls):
    """{key: {mod: [mod classes]}}: a mod class under a vanilla class that redeclares a target's name."""
    every = {}
    for mod, decls in mod_decls:
        for cls, d in decls.items():
            every[cls] = (mod, d)

    def vanilla_roots(cls, seen=()):
        if cls in seen:
            return []
        if cls in every:
            return [r for s in every[cls][1]['supers'] for r in vanilla_roots(s, seen + (cls,))]
        return [cls] if cls.startswith(VANILLA) else []

    found = collections.defaultdict(lambda: collections.defaultdict(set))
    for cls, (mod, d) in every.items():
        roots = set(vanilla_roots(cls))
        for name in set(d['methods']):
            if name == '<init>' or name == cls.rsplit('/', 1)[-1].split('$')[-1]:
                continue
            for root in roots:
                k = vanilla.target(root, name)
                if k:
                    found[k][mod].add(cls.replace('/', '.'))
    return {k: {m: sorted(c) for m, c in mods.items()} for k, mods in found.items()}


def mixins_signal(vanilla, entries):
    """{key: {mod: ['Mixin.handler (kind)']}} from mixins.json entries."""
    found = collections.defaultdict(lambda: collections.defaultdict(set))
    for e in entries:
        for cls in e.get('target_class') or []:
            for name in ex.target_methods(e):
                if name.startswith('<'):
                    continue
                k = vanilla.target(cls.replace('.', '/'), name)
                if k:
                    found[k][e['mod']].add(f'{e["mixin"].rsplit(".", 1)[-1]}.{e["handler"]} ({e["kind"]})')
    return {k: {m: sorted(c) for m, c in mods.items()} for k, mods in found.items()}


def event_key(vanilla, method):
    """Ledger key of a vanilla method the explain index names (`a/B$1.lambda$m$0:()V` is `a/B#m`), or None."""
    cls, name = ex.split_method(method)
    parts = [p for p in cls.split('$') if not p.isdigit()]
    cls = '$'.join(parts)
    m = re.match(r'lambda\$(.+?)\$\d+$', name)
    k = f'{cls}#{m.group(1) if m else name}'
    return k if k in vanilla.methods else None


def is_event(cls):
    return any('Event' in part for part in cls.rsplit('/', 1)[-1].split('$'))


def own_alias_index(index):
    """The explain index with alias hops limited to NeoForge extension interfaces (a name-only alias of `init` or `tick` reaches every vanilla caller)."""
    calls = dict(index['calls'])
    aliases = {}
    for target in calls:
        if target.startswith(VANILLA):
            aliases.setdefault(target.split('.', 1)[1], []).append(target)
    for constructors in index['news'].values():
        for m in constructors:
            if '/extensions/' in m and m not in calls:
                calls[m] = [c for t in aliases.get(m.split('.', 1)[1], []) for c in index['calls'][t]]
    return {**index, 'calls': calls, 'aliases': {}}


def events_signal(vanilla, index):
    """{key: [event]}: NeoForge events a vanilla method posts, through the hooks (ex.callers_of)."""
    index = own_alias_index(index)
    found = collections.defaultdict(set)
    for event in index['news']:
        if not event.startswith('net/neoforged/') or not is_event(event):
            continue
        dotted = event.replace('/', '.')
        for c in ex.callers_of(index, dotted, limit=10 ** 6)['callers']:
            k = event_key(vanilla, c['method'])
            if k:
                found[k].add(ex.short_name(dotted))
    return {k: sorted(v) for k, v in found.items()}


def hotness(run, java, cache):
    """({key-less 'cls.name': samples}, note): inclusive execution samples of vanilla frames in the run's real-client recording."""
    if not run:
        return {}, 'no --run given'
    run = Path(run)
    recordings = sorted((run / 'forge').glob('*.jfr')) or sorted(run.glob('*.jfr'))
    if not recordings:
        return {}, f'{run} has no .jfr recording'
    rec = recordings[0]
    stamp = hashlib.sha256(f'{rec.name}{rec.stat().st_size}{rec.stat().st_mtime_ns}'.encode()).hexdigest()[:16]
    path = Path(cache) / f'hot-{stamp}.json'
    if path.is_file():
        saved = json.loads(path.read_text())
        return saved['counts'], saved['note']
    jfr = f'{java}/bin/jfr' if java and Path(f'{java}/bin/jfr').is_file() else shutil.which('jfr')
    counts = collections.Counter()
    events = 0
    printer = subprocess.Popen([jfr, 'print', '--events', 'jdk.ExecutionSample', '--stack-depth', '64', str(rec)], stdout=subprocess.PIPE, text=True, errors='replace')
    try:
        for e in mp.jfr_events(printer.stdout):
            events += 1
            counts.update({f for f in (hot_frame(x) for x in e['frames']) if f})
    finally:
        printer.kill()
        printer.wait()
    note = f'{events} execution samples in {rec.name}' if events else f'{rec.name} has no execution samples'
    Path(cache).mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps({'counts': counts, 'note': note}))
    return dict(counts), note


def hot_frame(frame):
    """`net.minecraft.a.B$C.name(args) line: 3` as `net/minecraft/a/B$C#name`, or None."""
    m = re.match(r'(net\.minecraft\.[\w.$]+)\.([\w$<>]+)\(', frame)
    return f'{m.group(1).replace(".", "/")}#{m.group(2)}' if m else None


def hot_signal(vanilla, counts):
    out = {}
    for frame, n in counts.items():
        cls, name = frame.split('#')
        k = vanilla.target(cls, name)
        if k:
            out[k] = out.get(k, 0) + n
    return out


def class_harnesses(ledger):
    """{class path: sorted harness names} from the ledger's per-method harness coverage."""
    out = collections.defaultdict(set)
    for k, names in ledger.get('harness', {}).items():
        out[k.split('#')[0]] |= set(names)
    return {c: sorted(v) for c, v in out.items()}


def parity_harnesses(files, cache, run=None):
    """{file: [harness]} from `tools/parity.py --changed-files --list`, one call per file (about 15 s each), cached."""
    path = Path(cache) / 'parity-files.json'
    saved = json.loads(path.read_text()) if path.is_file() else {}
    run = run or (lambda f: subprocess.run([sys.executable, str(ROOT / 'tools' / 'parity.py'), '--changed-files', f, '--list'], capture_output=True, text=True, cwd=ROOT).stdout)
    for f in files:
        if f not in saved:
            tmp = Path(cache) / 'parity-one.txt'
            tmp.write_text(f + '\n')
            saved[f] = [x for x in run(str(tmp)).splitlines() if re.fullmatch(r'[a-z0-9_#]+', x)]
    Path(cache).mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(saved))
    return {f: saved[f] for f in files}


# The decision

def mutator_like(vanilla, k):
    m = vanilla.methods[k]
    return not m.get('abstract') and not m.get('accessor') and m.get('size', 0) > 3 and not READ_ONLY.match(m['name']) and (bool(m.get('touches')) or m.get('size', 0) >= 8)


def gameplay(k):
    return (package_of(k) + '/').startswith(GAMEPLAY)


def weight(signals, k, vanilla):
    mods = lambda d: len(d)  # noqa: E731
    w = 10 * mods(signals['calls']) + 10 * mods(signals['overrides']) + 12 * mods(signals['mixins']) + 8 * len(signals['events'])
    w += 2 * min(len(signals['harnesses']), 5) + math.log10(1 + vanilla.methods[k].get('size', 0)) + (2 if gameplay(k) else 0) + (3 * math.log10(1 + signals['hot']) if signals['hot'] else 0)
    if signals['client_side'] and not (signals['overrides'] or signals['mixins'] or signals['events']):
        w *= 0.1
    return round(w, 2)


def classify(signals, vanilla, k):
    """(class, reason) by the stated rule: P2 on any mod reference or event; P1-candidate on harnesses or a state-changing gameplay method."""
    parts = []
    if signals['calls']:
        parts.append('called by ' + ', '.join(f'{m} ({n})' for m, n in sorted(signals['calls'].items())))
    if signals['overrides']:
        parts.append('overridden by ' + ', '.join(sorted(signals['overrides'])))
    if signals['mixins']:
        parts.append('mixin target of ' + ', '.join(sorted(signals['mixins'])))
    if signals['events']:
        parts.append('posts ' + ', '.join(signals['events'][:3]) + (f' +{len(signals["events"]) - 3}' if len(signals['events']) > 3 else ''))
    if parts:
        if signals['client_side'] and not (signals['overrides'] or signals['mixins'] or signals['events']):
            parts.append('client class: Java runs its own copy, confirm java-serves')
        if signals['hot']:
            parts.append(f'{signals["hot"]} real-client samples')
        return 'P2', '; '.join(parts)
    if signals['harnesses'] and mutator_like(vanilla, k):
        return 'P1-candidate', f'{len(signals["harnesses"])} harnesses cover its class ({", ".join(signals["harnesses"][:3])})'
    if gameplay(k) and mutator_like(vanilla, k):
        return 'P1-candidate', f'gameplay package {package_of(k)}, changes state'
    return 'deferred', 'no evidence'


def triage(vanilla, targets, signals_by, hot_note, harness):
    rows = []
    for k, t in targets.items():
        cls = k.split('#')[0]
        s = {'calls': signals_by['calls'].get(k, {}), 'overrides': signals_by['overrides'].get(k, {}), 'mixins': signals_by['mixins'].get(k, {}),
             'events': signals_by['events'].get(k, []), 'hot': signals_by['hot'].get(k, 0), 'harnesses': harness.get(cls, []),
             'client_side': cls.startswith(VANILLA + 'client/')}
        verdict, reason = classify(s, vanilla, k)
        rows.append({'method': display(k), 'key': k, 'package': package_of(k) or '(root)', 'status': t['status'], 'class': verdict, 'weight': weight(s, k, vanilla),
                     'reason': reason, 'ledger_note': t['note'], 'signals': s})
    rows.sort(key=lambda r: (CLASSES.index(r['class']), -r['weight'], r['method']))
    return rows


# Output

def counts_of(rows):
    by_class = collections.Counter(r['class'] for r in rows)
    by_package = collections.defaultdict(collections.Counter)
    for r in rows:
        by_package[r['package']][r['class']] += 1
    return {c: by_class.get(c, 0) for c in CLASSES}, {p: {c: v.get(c, 0) for c in CLASSES} for p, v in sorted(by_package.items(), key=lambda kv: -sum(kv[1].values()))}


def signal_text(s):
    bits = []
    for name, label in (('calls', 'calls'), ('overrides', 'overrides'), ('mixins', 'mixins')):
        if s[name]:
            bits.append(f'{label}: ' + ', '.join(f'{m}' + (f'×{v}' if isinstance(v, int) else f'×{len(v)}') for m, v in sorted(s[name].items())))
    if s['events']:
        bits.append('events: ' + ', '.join(s['events'][:3]))
    if s['hot']:
        bits.append(f'hot: {s["hot"]}')
    if s['harnesses']:
        bits.append(f'harnesses: {len(s["harnesses"])}')
    return '; '.join(bits) or 'none'


def markdown(result):
    lines = ['# Port triage', '', f'Methods triaged: {len(result["methods"])} ({result["set"]}). Install: `{result["install"]}`. Hotness: {result["hotness_note"]}.', '',
             'Rule of thumb (the user confirms): **P2** if a pack mod calls, overrides or mixes into the method, or it posts a NeoForge event; **P1-candidate** if a harness covers its class or it sits in a gameplay package (' + ', '.join(GAMEPLAY) + '), and it looks state-changing (not a getter-like name; touches fields or is 8+ instructions); **deferred** otherwise. '
             'Client-package methods with only calls rank low (Java runs its own copy). Overrides match by name (the ledger has no descriptors).', '',
             '## Counts', '', '| class | methods |', '|---|---|']
    lines += [f'| {c} | {n} |' for c, n in result['counts'].items()]
    lines += ['', '## Per package', '', '| package | P2 | P1-candidate | deferred |', '|---|---|---|---|']
    lines += [f'| {p} | {v["P2"]} | {v["P1-candidate"]} | {v["deferred"]} |' for p, v in list(result['packages'].items())[:40]]
    if len(result['packages']) > 40:
        lines.append(f'| ... {len(result["packages"]) - 40} more packages in triage.json | | | |')
    for c in CLASSES[:2]:
        rows = [r for r in result['methods'] if r['class'] == c]
        lines += ['', f'## {c} ({len(rows)})', '', '| method | weight | signals | reason |', '|---|---|---|---|']
        lines += [f'| `{r["method"]}` | {r["weight"]} | {signal_text(r["signals"])} | {r["reason"]} |' for r in rows[:300]]
        if len(rows) > 300:
            lines.append(f'| ... {len(rows) - 300} more in triage.json | | | |')
    lines += ['', f'## deferred ({result["counts"]["deferred"]})', '', 'No evidence; listed in triage.json.']
    return '\n'.join(lines) + '\n'


# Verdicts

def read_decisions(path):
    out = []
    for n, line in enumerate(Path(path).read_text().splitlines(), 1):
        if not line.strip() or line.lstrip().startswith('#'):
            continue
        parts = [p.strip() for p in line.split('\t')] + ['', '']
        method, decision, reason = parts[:3]
        d = decision.lower()
        if d not in DECISIONS and not (d.startswith('skip:') and d[5:] in SKIP_REASONS):
            sys.exit(f'{path}:{n}: decision {decision!r} is not one of {", ".join(DECISIONS)}, skip:<{"|".join(SKIP_REASONS)}>')
        if d != 'deferred' and not reason:
            sys.exit(f'{path}:{n}: a decision needs a reason')
        out.append((method, d, reason))
    return out


def verdict_lines(decisions):
    """port_map rows: p1/p2/p3 stay `missing` with the priority in the note; skip:<reason> goes out as given."""
    lines = []
    for method, d, reason in decisions:
        method = display(method) if '#' in method else method
        if d == 'deferred':
            continue
        if d.startswith('skip:'):
            lines.append(f'{method}\t{d}\t\t{reason}')
        else:
            lines.append(f'{method}\tmissing\t\t{d.upper()}: {reason}')
    return lines


def ledger_reads_extra_files(source=None):
    """True if port_ledger.py globs every *.tsv of tools/port_map (so a new file is read)."""
    text = (source or ROOT / 'tools' / 'port_ledger.py').read_text()
    return 'MAP_DIR.glob("*.tsv")' in text or "MAP_DIR.glob('*.tsv')" in text


def write_verdicts(decisions_path, date=None, map_dir=MAP_DIR, ledger_source=None):
    if not ledger_reads_extra_files(ledger_source):
        sys.exit('port_ledger.py does not read extra TSV files in tools/port_map: not writing')
    lines = verdict_lines(read_decisions(decisions_path))
    out = Path(map_dir) / f'triage-{date or datetime.date.today().isoformat()}.tsv'
    with out.open('a') as f:
        f.write('\n'.join(lines) + '\n' if lines else '')
    return out, len(lines)


# The run

def build(install, run=None, cache=None, ledger_path=LEDGER_JSON, unanswered=False, parity=False, ledger=None):
    install, cache = Path(install), Path(cache or DEFAULT_CACHE)
    cache.mkdir(parents=True, exist_ok=True)
    ledger = ledger or json.loads(Path(ledger_path).read_text())
    targets = targets_of(ledger, unanswered)
    vanilla = Vanilla(ledger, set(targets))
    record = install / 'install.json'
    java = json.loads(record.read_text()).get('java') if record.is_file() else os.environ.get('JAVA_HOME')
    javap = f'{java}/bin/javap' if java and Path(f'{java}/bin/javap').is_file() else shutil.which('javap')
    jars = mod_jars(install, cache)
    mod_indexes = [(mod, ex.load_index(javap, [jar], cache)) for mod, jar in jars]
    mod_decls = [(mod, declarations(javap, jar, cache)) for mod, jar in jars]
    mixin_file = Path(run) / 'mixins.json' if run else None
    if mixin_file and mixin_file.is_file():
        entries = json.loads(mixin_file.read_text())
    else:
        entries = mp.scan_install(install, cache / 'mixins')
    index = ex.load_index(javap, ex.patched_jars(install / 'libraries'), cache)
    counts, hot_note = hotness(run, java, cache)
    harness = class_harnesses(ledger)
    if parity:
        files = {f for c in {k.split('#')[0] for k in targets} for f in ledger.get('homes', {}).get(c, [])}
        by_file = parity_harnesses(sorted(files), cache)
        for c in {k.split('#')[0] for k in targets}:
            extra = sorted({h for f in ledger.get('homes', {}).get(c, []) for h in by_file.get(f, [])})
            harness[c] = sorted(set(harness.get(c, [])) | set(extra))
    signals_by = {'calls': calls_signal(vanilla, mod_indexes), 'overrides': overrides_signal(vanilla, mod_decls), 'mixins': mixins_signal(vanilla, entries),
                  'events': events_signal(vanilla, index), 'hot': hot_signal(vanilla, counts)}
    rows = triage(vanilla, targets, signals_by, hot_note, harness)
    by_class, by_package = counts_of(rows)
    return {'install': str(install), 'run': str(run) if run else None, 'set': 'absent plus unanswered' if unanswered else 'absent: verdict missing in tools/port_map',
            'mods': sorted({m.split('>')[0] for m, _ in jars}), 'hotness_note': hot_note, 'counts': by_class, 'packages': by_package, 'methods': rows}


def main():
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    p.add_argument('--install', help='an install whose mods/ and libraries/ to scan (read only)')
    p.add_argument('--run', help='a comparison run folder; its forge/*.jfr gives hotness if it has execution samples')
    p.add_argument('--out', help='where triage.md and triage.json go')
    p.add_argument('--ledger', default=str(LEDGER_JSON), help='the ledger.json of the last `port_ledger.py build`')
    p.add_argument('--cache', default=str(DEFAULT_CACHE))
    p.add_argument('--unanswered', action='store_true', help='also triage the ledger status `missing` (no verdict yet)')
    p.add_argument('--parity', action='store_true', help='add harnesses from tools/parity.py for the classes\' home files (about 15 s per file)')
    p.add_argument('--write-verdicts', metavar='FILE', help='a TSV of method, decision, reason; appends tools/port_map/triage-<date>.tsv')
    p.add_argument('--date', help='date for the verdict file name (default: today)')
    args = p.parse_args()
    if args.write_verdicts:
        out, n = write_verdicts(args.write_verdicts, args.date)
        print(f'{n} rows appended to {out}')
        return
    if not (args.install and args.out):
        p.error('--install and --out are required')
    result = build(args.install, args.run, args.cache, args.ledger, args.unanswered, args.parity)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    (out / 'triage.json').write_text(json.dumps(result, indent=1) + '\n')
    (out / 'triage.md').write_text(markdown(result))
    print(out / 'triage.md', result['counts'])


if __name__ == '__main__':
    main()
