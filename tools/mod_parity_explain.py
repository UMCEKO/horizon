#!/usr/bin/env python3
"""Where to fix each gap of a mod parity run: the vanilla callers that post a missing event, and Horizon's place for them.

    mod_parity_explain.py --run DIR [--install DIR] [--cache DIR]    (also: mod_parity.py explain --run DIR)

Reads DIR/report.json, DIR/mixins.json and DIR/horizon/merged.txt, indexes the install's patched jars with `javap -c -s -p`
(new <Event> and calls into net/minecraft and net/neoforged; signatures and bytecode only), asks tools/port_ledger.py
where the vanilla methods are ported, and greps the Java host for sites that already fire the event.
It writes the "Where to fix" section of DIR/report.md and the `where_to_fix` key of DIR/report.json.
"""
import argparse
import collections
import concurrent.futures
import gzip
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
HOST = ROOT / 'mods' / 'horizon' / 'src' / 'java-mods-host'
LEDGER = ROOT / 'tools' / 'port_ledger.py'
SECTION = '## Where to fix'
KEPT_PREFIXES = ('net/minecraft/', 'net/neoforged/')
HOST_SKIP = ('testmod', 'unit', 'parity')
TOP_MISSING, TOP_SHORT, MAX_CALLERS = 30, 15, 4


# The bytecode index

CLASS_LINE = re.compile(r'^(?:[a-z@]+ )*(?:class|interface|enum) ([^\s<{]+)')
METHOD_LINE = re.compile(r'^  (?:[^\s].*\(.*\)(?: throws [^;]+)?|static \{\});$')
CODE_LINE = re.compile(r'^\s+\d+: (new|invoke(?:static|virtual|special|interface))\s+#\d+(?:, +\d+)?\s+// (?:class|Method|InterfaceMethod) (\S+)$')


def parse_javap(lines):
    """Yields ('new', class, caller) and ('call', target, caller); a caller or target is `a/b/C.name:desc`."""
    cls = name = current = None
    for line in lines:
        line = line.rstrip('\n')
        if line and not line[0].isspace():
            m = CLASS_LINE.match(line)
            if m and line.endswith('{'):
                cls = m.group(1).replace('.', '/')
            current = None
            continue
        if METHOD_LINE.match(line):
            head = line.split('(', 1)[0].split()
            name = head[-1] if head else ''
            name = '<clinit>' if line.strip() == 'static {};' else '<init>' if name.replace('.', '/') == cls else name
            current = None
            continue
        stripped = line.strip()
        if stripped.startswith('descriptor: ') and name is not None:
            current = f'{cls}.{name}:{stripped[12:]}'
            continue
        m = CODE_LINE.match(line)
        if not m or current is None:
            continue
        op, ref = m.groups()
        if op == 'new':
            if ref.startswith('net/neoforged/'):
                yield ('new', ref, current)
        elif ref.startswith(KEPT_PREFIXES):
            target, desc = ref.rsplit(':', 1)
            owner, called = target.rsplit('.', 1)
            if called == '"<init>"':
                continue
            yield ('call', f'{owner}.{called.strip(chr(34))}:{desc}', current)


def parse_javap_text(text):
    return list(parse_javap(text.splitlines()))


def jar_classes(jar):
    with zipfile.ZipFile(jar) as z:
        return [n[:-6].replace('/', '.') for n in z.namelist() if n.endswith('.class') and not n.endswith('module-info.class') and '/' in n]


def javap_batch(javap, jar, names):
    proc = subprocess.run(['nice', javap, '-c', '-s', '-p', '-cp', str(jar), *names], capture_output=True, text=True, errors='replace')
    news, calls = collections.defaultdict(set), collections.defaultdict(set)
    for kind, target, caller in parse_javap(proc.stdout.splitlines()):
        (news if kind == 'new' else calls)[target].add(caller)
    return news, calls


def index_jars(javap, jars, workers=6, batch=400):
    news, calls = collections.defaultdict(set), collections.defaultdict(set)
    with concurrent.futures.ThreadPoolExecutor(workers) as pool:
        futures = []
        for jar in jars:
            names = jar_classes(jar)
            futures += [pool.submit(javap_batch, javap, jar, names[i:i + batch]) for i in range(0, len(names), batch)]
        for f in futures:
            n, c = f.result()
            for k, v in n.items():
                news[k] |= v
            for k, v in c.items():
                calls[k] |= v
    return {'news': {k: sorted(v) for k, v in news.items()}, 'calls': {k: sorted(v) for k, v in calls.items()}}


def digest(paths):
    h = hashlib.sha256()
    for p in paths:
        with open(p, 'rb') as f:
            for block in iter(lambda: f.read(1 << 20), b''):
                h.update(block)
    return h.hexdigest()[:20]


def load_index(javap, jars, cache):
    """The index of the jars, cached under their content hash."""
    cache = Path(cache)
    cache.mkdir(parents=True, exist_ok=True)
    path = cache / f'index-{digest(jars)}.json.gz'
    if path.is_file():
        return json.loads(gzip.decompress(path.read_bytes()))
    index = index_jars(javap, jars)
    path.write_bytes(gzip.compress(json.dumps(index).encode()))
    return index


def patched_jars(libraries):
    """The jars NeoForge patches and builds: client, server, the universal jar and the loader."""
    libs = Path(libraries) / 'net' / 'neoforged'
    patterns = ['minecraft-client-patched/*/*.jar', 'minecraft-server-patched/*/*.jar', 'neoforge/*/neoforge-*-universal.jar', 'fancymodloader/loader/*/loader-*.jar']
    found = [sorted(libs.glob(p)) for p in patterns]
    missing = [p for p, f in zip(patterns, found) if not f]
    if missing[:3] and len(missing) > 1:
        sys.exit(f'{libs} has no {", ".join(missing)}')
    return [f[-1] for f in found if f]


# From an event to the vanilla methods that post it

def dotted(name):
    return name.replace('/', '.')


def split_method(method):
    """`a/b/C.name:desc` to (`a/b/C`, `name`)."""
    head = method.rsplit(':', 1)[0] if ':(' in method else method
    cls, name = head.rsplit('.', 1)
    return cls, name


def is_vanilla(method):
    return split_method(method)[0].startswith('net/minecraft/')


def readable(method):
    cls, name = split_method(method)
    m = re.match(r'lambda\$(.+?)\$\d+$', name)
    return f'{dotted(cls)}.{m.group(1) if m else name}' + (' (lambda)' if m else '')


def ledger_name(method):
    """`net/minecraft/a/Outer$Inner.lambda$m$0:()V` to `Outer.Inner.m`; anonymous classes fold into their outer class."""
    cls, name = split_method(method)
    parts = [p for p in cls.rsplit('/', 1)[-1].split('$') if not p.isdigit()]
    m = re.match(r'lambda\$(.+?)\$\d+$', name)
    return f'{".".join(parts)}.{m.group(1) if m else name}'


def simple_hook(method):
    cls, name = split_method(method)
    return f'{cls.rsplit("/", 1)[-1].replace("$", ".")}.{name}'


def alias_callers(index, method):
    """Callers of a NeoForge extension method that vanilla reaches through the class it extends (same name and descriptor)."""
    if 'aliases' not in index:
        aliases = collections.defaultdict(list)
        for target in index['calls']:
            if target.startswith('net/minecraft/'):
                aliases[target.split('.', 1)[1]].append(target)
        index['aliases'] = aliases
    suffix = method.split('.', 1)[1]
    return [c for target in index['aliases'].get(suffix, []) for c in index['calls'][target]]


def callers_of(index, event, depth=3, limit=MAX_CALLERS):
    """{'constructors': [method], 'callers': [{method, via, up}]}: vanilla methods that reach `new event`, through NeoForge's hooks."""
    constructors = index['news'].get(event.replace('.', '/'), [])
    found, seen = {}, set()
    frontier = [(c, ()) for c in constructors]
    for _ in range(depth + 1):
        following = []
        for method, via in frontier:
            if method in seen:
                continue
            seen.add(method)
            if is_vanilla(method):
                found.setdefault(method, via)
                continue
            for caller in index['calls'].get(method) or alias_callers(index, method):
                following.append((caller, via + (simple_hook(method),)))
        frontier = following
    ranked = sorted(found.items(), key=lambda kv: (len(kv[1]), kv[0]))[:limit]
    callers = []
    for method, via in ranked:
        up = sorted({readable(c) for c in index['calls'].get(method, []) if is_vanilla(c) and c != method})[:3]
        callers.append({'method': method, 'via': list(via), 'up': up})
    return {'constructors': [readable(c) for c in constructors], 'callers': callers, 'total_callers': len(found)}


# Horizon's side: the Java host and the port ledger

def host_lines(root=HOST):
    lines = []
    for path in sorted(Path(root).rglob('*.java')):
        if any(part in HOST_SKIP for part in path.relative_to(root).parts):
            continue
        for n, text in enumerate(path.read_text(errors='replace').splitlines(), 1):
            lines.append((path, n, text))
    return lines


def event_pattern(event):
    simple = event.rsplit('.', 1)[-1]
    parts = simple.split('$')
    return re.compile(r'\b' + r'\.'.join(map(re.escape, parts[-2:])) + r'\b')


def host_sites(lines, event, hooks, citations, root=ROOT):
    """Java host lines that fire the event, name it, call its hooks, or cite the vanilla methods that post it."""
    pattern = event_pattern(event)
    simple = pattern.pattern
    fires = re.compile(r'new\s+(?:[\w.]+\.)?' + simple.replace(r'\b', ''))
    hook = re.compile(r'\b(?:' + '|'.join(re.escape(h) for h in hooks) + r')\b') if hooks else None
    cite = re.compile('|'.join(r'\b' + re.escape(c) + r'\b' for c in citations)) if citations else None
    result = {'fires': [], 'hooks': [], 'mentions': [], 'near_miss': []}
    for path, n, text in lines:
        if text.lstrip().startswith('import '):
            continue
        place = f'{path.relative_to(root)}:{n}'
        if fires.search(text):
            result['fires'].append(place)
        elif hook and hook.search(text):
            result['hooks'].append(place)
        elif pattern.search(text):
            result['mentions'].append(place)
        elif cite and cite.search(text):
            result['near_miss'].append(place)
    return {k: v[:3] for k, v in result.items()}


PLACE = re.compile(r'([\w./+-]+\.rs):(\d+)')


def parse_ledger(text):
    """{'status', 'port', 'home'} from `port_ledger.py --json where`: the first Rust place of a method, or the class home."""
    try:
        records = json.loads(text)
    except ValueError:
        return None
    for r in records:
        if 'vanilla' in r:
            for key in ('ported_in', 'map', 'mentioned_in', 'same_name_in'):
                for entry in r.get(key, []):
                    m = PLACE.search(entry)
                    if m:
                        return {'status': r.get('status', ''), 'port': f'{m.group(1)}:{m.group(2)}', 'how': key, 'vanilla': r['vanilla']}
            return {'status': r.get('status', ''), 'port': None, 'home': (r.get('class_home') or [None])[0], 'vanilla': r['vanilla']}
        if 'class' in r:
            return {'status': 'class', 'port': None, 'home': (r.get('home') or [None])[0], 'vanilla': r['class']}
    return None


class Ledger:
    def __init__(self, cache=None, run=None):
        self.cache = Path(cache) / 'ledger-answers.json' if cache else None
        stamp = LEDGER.parent.parent / '.local' / 'port-ledger' / 'ledger.json'
        self.stamp = f'{stamp.stat().st_mtime_ns}' if stamp.is_file() else 'none'
        self.run = run or self.ask
        self.answers = {}
        if self.cache and self.cache.is_file():
            saved = json.loads(self.cache.read_text())
            if saved.get('stamp') == self.stamp:
                self.answers = saved['answers']

    def ask(self, query):
        done = subprocess.run([sys.executable, str(LEDGER), '--json', 'where', query], capture_output=True, text=True, cwd=ROOT)
        return done.stdout

    def where(self, query):
        if query not in self.answers:
            self.answers[query] = parse_ledger(self.run(query))
        return self.answers[query]

    def save(self):
        if self.cache:
            self.cache.write_text(json.dumps({'stamp': self.stamp, 'answers': self.answers}, indent=1))


def port_of(ledger, method_query, class_query=None):
    """The ledger's answer for `Class.method`, else for the class (its home file)."""
    answer = ledger.where(method_query)
    if answer and answer.get('port'):
        return answer
    fallback = ledger.where(class_query or method_query.rsplit('.', 1)[0])
    return fallback if fallback and fallback.get('port') else answer or fallback


# The entries

def explain_event(index, lines, ledger, event, **info):
    found = callers_of(index, event)
    hooks = sorted({v.rsplit('.', 1)[-1] for c in found['callers'] for v in c['via']} | {c.rsplit('.', 1)[-1] for c in found['constructors'] if 'Hooks' in c})
    citations = sorted({ledger_name(c['method']) for c in found['callers']})
    sites = host_sites(lines, event, hooks, citations)
    callers = []
    for c in found['callers']:
        port = port_of(ledger, ledger_name(c['method']), ledger_name(c['method']).rsplit('.', 1)[0])
        callers.append({'caller': readable(c['method']), 'via': c['via'], 'up': c['up'], 'vanilla_port': port and port.get('port'),
                        'port_status': port and port.get('status'), 'class_home': port and port.get('home')})
    return {'event': event, **info, 'constructors': found['constructors'], 'callers': callers, 'more_callers': max(found['total_callers'] - len(callers), 0),
            'fired_by_horizon_at': sites['fires'] or sites['hooks'], 'mentioned_at': sites['mentions'], 'cited_at': sites['near_miss']}


def parse_merged(path):
    """{target class: {mixin class: {merged names}}} from a run's merged.txt."""
    merged = collections.defaultdict(lambda: collections.defaultdict(set))
    if path and Path(path).is_file():
        for line in Path(path).read_text().splitlines():
            parts = line.split('\t')
            if len(parts) == 4:
                merged[parts[0]][parts[3].replace('/', '.')].add(parts[1])
    return merged


def target_methods(entry):
    """Method names of a mixin target: `Lx/Y;item(..)V` and `accept(..)Z` both give a name."""
    names = []
    for t in entry.get('target_method') or []:
        name = re.sub(r'^L[^;]+;', '', t).split('(', 1)[0]
        if name and name not in names:
            names.append(name)
    return names


def explain_handler(entry, merged, ledger):
    classes = entry.get('target_class') or []
    mixins = {c: merged.get(c, {}) for c in classes}
    if any(n == entry['handler'] or n.endswith('$' + entry['handler']) for m in mixins.values() for mixin, names in m.items() if mixin == entry['mixin'] for n in names):
        java = 'the target is loaded in Java and the handler merged, but it never ran'
    elif any(entry['mixin'] in m for m in mixins.values()):
        java = 'the target is loaded in Java and the mixin applied, but this handler was not merged'
    elif any(mixins.values()):
        java = 'the target is loaded in Java, but this mixin never applied'
    elif merged:
        java = 'Java never loads or transforms the target'
    else:
        java = 'unknown'
    port = None
    for cls in classes:
        simple = '.'.join(cls.rsplit('.', 1)[-1].split('$'))
        for name in target_methods(entry) or ['']:
            answer = port_of(ledger, f'{simple}.{name}' if name else simple, simple)
            if answer and (answer.get('port') or not port):
                port = answer
            if port and port.get('port'):
                break
        if port and port.get('port'):
            break
    return {'mod': entry['mod'], 'mixin': entry['mixin'], 'handler': entry['handler'], 'kind': entry['kind'], 'target_class': classes,
            'target_method': target_methods(entry), 'forge_calls': entry.get('forge'), 'java_side': java,
            'vanilla_port': port and port.get('port'), 'port_status': port and port.get('status'), 'class_home': port and port.get('home')}


def short_events(events):
    """Classes Horizon fires fewer times than the real client, but at least once, by the size of the deficit."""
    totals = collections.defaultdict(lambda: [0, 0, set()])
    for m in events['count_mismatches']:
        t = totals[m['event']]
        t[0] += m['forge']
        t[1] += m['horizon']
        t[2].add(m['side'])
    missing = {m['event'] for m in events['missing']}
    rows = [(e, f, h, sorted(s)) for e, (f, h, s) in totals.items() if 0 < h < f and e not in missing]
    return sorted(rows, key=lambda r: r[1] - r[2], reverse=True)


def explain(run, install=None, cache=None, javap=None, ledger=None, index=None):
    run = Path(run)
    report = json.loads((run / 'report.json').read_text())
    cache = cache or ROOT / '.local' / 'mod-parity-index'
    if index is None:
        install = Path(install) if install else run / 'horizon' / 'install'
        record = install / 'install.json'
        java = json.loads(record.read_text()).get('java') if record.is_file() else None
        javap = javap or (f'{java}/bin/javap' if java and Path(f'{java}/bin/javap').is_file() else shutil.which('javap'))
        index = load_index(javap, patched_jars(install / 'libraries'), cache)
    ledger = ledger or Ledger(cache)
    lines = host_lines()
    events = report['events']
    missing = sorted(events['missing'], key=lambda m: -m['count'])
    short = short_events(events)
    with concurrent.futures.ThreadPoolExecutor(4) as pool:
        miss = list(pool.map(lambda m: explain_event(index, lines, ledger, m['event'], count=m['count'], sides=m['sides'], first_step=m['first_step']), missing[:TOP_MISSING]))
        shorts = list(pool.map(lambda r: explain_event(index, lines, ledger, r[0], forge=r[1], horizon=r[2], sides=r[3]), short[:TOP_SHORT]))
        merged = parse_merged(run / 'horizon' / 'merged.txt')
        dead = [r for r in report['mixins'] if r['state'] == 'dead in Horizon' and not r['access']]
        handlers = list(pool.map(lambda r: explain_handler(r, merged, ledger), dead))
    ledger.save()
    return {'missing_events': miss, 'missing_total': len(missing), 'short_events': shorts, 'dead_handlers': handlers}


# The report section

def short_name(event):
    return event.replace('net.neoforged.neoforge.', '').replace('net.neoforged.', '')


def horizon_place(entry):
    if entry['fired_by_horizon_at']:
        return f'fired by Horizon at {entry["fired_by_horizon_at"][0]}' + (' (not reached in this run)' if 'count' in entry else ' (fewer times)')
    if entry['mentioned_at']:
        return f'named but not fired, {entry["mentioned_at"][0]}'
    if entry['cited_at']:
        return f'caller cited at {entry["cited_at"][0]}, event not fired'
    return 'no firing site in the Java host'


def port_text(entry):
    for c in entry['callers']:
        if c['vanilla_port']:
            return f'`{c["caller"]}` ported at {c["vanilla_port"]} ({c["port_status"]})'
    homes = [c for c in entry['callers'] if c['class_home']]
    return f'no method port; class home {homes[0]["class_home"]}' if homes else 'no vanilla port found'


def event_line(entry):
    head = f'`{short_name(entry["event"])}` ({",".join(entry["sides"])}, ' + (f'{entry["count"]}' if 'count' in entry else f'real {entry["forge"]}, Horizon {entry["horizon"]}') + ')'
    if not entry['callers']:
        what = f'constructed in {", ".join(entry["constructors"][:2])}, no vanilla caller found' if entry['constructors'] else 'no constructor in the indexed jars'
        return f'- {head} → {what} → {horizon_place(entry)}'
    shown = []
    for c in entry['callers'][:3]:
        via = f' via {c["via"][-1]}' if c['via'] else ''
        up = f' (from {c["up"][0]})' if c['up'] else ''
        shown.append(f'`{c["caller"]}`{via}{up}')
    more = len(entry['callers']) - 3 + entry['more_callers']
    return f'- {head} → {"; ".join(shown)}{f" +{more} more" if more > 0 else ""} → {horizon_place(entry)}; {port_text(entry)}'


def handler_line(h):
    target = f'{h["target_class"][0] if h["target_class"] else "?"}.{",".join(h["target_method"]) or "*"}'
    where = f'native owns `{h["vanilla_port"]}` ({h["port_status"]}); Java\'s version never runs' if h['vanilla_port'] else (
        f'no Rust port in the ledger (class home {h["class_home"]})' if h['class_home'] else 'no Rust port found in the ledger')
    return f'- {h["mod"]} `{h["mixin"].rsplit(".", 1)[-1]}.{h["handler"]}` ({h["kind"]}, real {h["forge_calls"]} calls) → `{target}` → {h["java_side"]}; {where}'


def section_markdown(where):
    lines = [SECTION, '', 'Each line: the gap → the vanilla method that posts it (and its caller) → where Horizon handles it today.', '',
             f'### Missing events (top {len(where["missing_events"])} of {where["missing_total"]} by count)', '']
    lines += [event_line(e) for e in where['missing_events']] or ['none']
    lines += ['', f'### Short-count events (top {len(where["short_events"])} by deficit)', '']
    lines += [event_line(e) for e in where['short_events']] or ['none']
    lines += ['', f'### Dead mixin handlers ({len(where["dead_handlers"])})', '']
    lines += [handler_line(h) for h in where['dead_handlers']] or ['none']
    unresolved = [e for e in where['missing_events'] + where['short_events'] if not e['callers']]
    lines += ['', f'### No vanilla caller found ({len(unresolved)})', '']
    lines += [f'- `{short_name(e["event"])}`: ' + (f'constructed in {", ".join(e["constructors"][:3])}' if e['constructors'] else 'no `new` of it in the patched jars, NeoForge or the loader') for e in unresolved] or ['none']
    return '\n'.join(lines) + '\n'


def insert_section(markdown, section):
    """Puts the section before the first detail section, replacing an older one."""
    markdown = re.sub(r'\n?' + re.escape(SECTION) + r'\n.*?(?=\n## |\Z)', '', markdown, flags=re.S)
    anchor = markdown.find('\n## Missing events')
    anchor = len(markdown) if anchor < 0 else anchor + 1
    return markdown[:anchor] + section + '\n' + markdown[anchor:]


def write_explanation(run, where):
    run = Path(run)
    report = json.loads((run / 'report.json').read_text())
    report['where_to_fix'] = where
    (run / 'report.json').write_text(json.dumps(report, indent=1) + '\n')
    (run / 'report.md').write_text(insert_section((run / 'report.md').read_text(), section_markdown(where)))


def explain_command(args):
    where = explain(args.run, args.install, args.cache)
    write_explanation(args.run, where)
    print(Path(args.run) / 'report.md')


def add_arguments(parser):
    parser.add_argument('--run', required=True, help='a finished run folder (report.json, mixins.json, forge/, horizon/)')
    parser.add_argument('--install', help='the install whose patched jars to index (default: the run\'s horizon/install)')
    parser.add_argument('--cache', help='where the index is cached, keyed by the jar hashes')


if __name__ == '__main__':
    p = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    add_arguments(p)
    explain_command(p.parse_args())
