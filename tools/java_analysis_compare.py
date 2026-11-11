#!/usr/bin/env python3
"""Dev only: compares the Rust mod analysis (crates/horizon-java-analysis) with the Python it ports, stage by stage.

    python3 tools/java_analysis_compare.py classes|libraries|jars|mixins|knowledge|replaces|entity-guards|entity-facts|all --install DIR [--bin PATH]

Writes only into a temp folder. Prints `N <units>, M differences` and the first 20 differences (one line per stage for `all`); exits 1 on any.
"""
import argparse
import json
import struct
import subprocess
import sys
import tempfile
import zipfile
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import java_knowledge  # noqa: E402
import lab  # noqa: E402
import mod_entity_facts as ef  # noqa: E402
import mod_parity_explain as ex  # noqa: E402
import mod_parity as mp  # noqa: E402
import mod_replaces as mr  # noqa: E402

SHOWN = 20


def rust_binary(override):
    path = Path(override) if override else lab.cargo_target() / 'fast' / 'horizon-java-analysis'
    if not path.is_file():
        sys.exit(f'{path} is missing: cargo build --profile fast -p horizon-java-analysis')
    return path


def nested_jars(jar, into):
    """The jars under META-INF/jarjar/ of a jar, and theirs, extracted into a folder."""
    found = []
    with zipfile.ZipFile(jar) as z:
        for name in sorted(z.namelist()):
            if name.startswith('META-INF/jarjar/') and name.endswith('.jar'):
                out = Path(into) / f'{len(found)}-{Path(name).name}'
                out.write_bytes(z.read(name))
                found.append(out)
    for inner in list(found):
        found += nested_jars(inner, into)
    return found


def jars_of(install, scratch):
    install = Path(install)
    top = sorted((install / 'mods').glob('*.jar')) + [ex.patched_jars(install / 'libraries')[0]]
    jars = []
    for jar in top:
        jars.append(jar)
        jars += nested_jars(jar, scratch)
    return jars


def python_dump(jar):
    """Python's parse of every class of a jar, in the shape of the Rust dump."""
    out = []
    with zipfile.ZipFile(jar) as z:
        for entry in z.namelist():
            if not entry.endswith('.class') or entry.startswith('META-INF/') or entry == 'module-info.class':
                continue
            try:
                info = mr.parse_class(z.read(entry), members=True, annotations=True, code=True)
            except (ValueError, IndexError, struct.error):
                out.append({'entry': entry, 'error': True})
                continue
            methods = [{k: v for k, v in m.items() if k != 'code'} for m in info['methods']]
            info = {k: v for k, v in info.items() if k not in ('pool', 'methods')}
            out.append({**info, 'methods': methods, 'entry': entry})
    return json.loads(json.dumps(out))


def rust_dump(binary, jar):
    run = subprocess.run([str(binary), 'dump-classes', str(jar)], capture_output=True, text=True, check=True)
    return [json.loads(line) for line in run.stdout.splitlines()]


def differences(path, python, rust):
    """Yields (path, python value, rust value) for the first difference under each mismatching path."""
    if isinstance(python, dict) and isinstance(rust, dict):
        for key in sorted(set(python) | set(rust)):
            if key not in python or key not in rust:
                yield f'{path}/{key}', python.get(key, '<missing>'), rust.get(key, '<missing>')
            else:
                yield from differences(f'{path}/{key}', python[key], rust[key])
    elif isinstance(python, list) and isinstance(rust, list) and len(python) == len(rust):
        for i, (a, b) in enumerate(zip(python, rust)):
            yield from differences(f'{path}[{i}]', a, b)
    elif python != rust:
        yield path, python, rust


def compare_classes(install, binary):
    classes, found = 0, []
    with tempfile.TemporaryDirectory(prefix='java-analysis-compare-') as scratch:
        for jar in jars_of(install, scratch):
            python, rust = python_dump(jar), rust_dump(binary, jar)
            label = jar.name
            if len(python) != len(rust):
                found.append((f'{label}: class count', len(python), len(rust)))
            for a, b in zip(python, rust):
                classes += 1
                if a.get('entry') != b.get('entry'):
                    found.append((f'{label}: entry order', a.get('entry'), b.get('entry')))
                    break
                found += [(f'{label}!{a["entry"]}: {p}', x, y) for p, x, y in differences('', a, b)]
    return classes, found


def rust_json(binary, command, *args):
    run = subprocess.run([str(binary), command, *map(str, args)], capture_output=True, text=True, check=True)
    return json.loads(run.stdout)


def compare_libraries(install, binary):
    """The library jar list, the class index (and the cache both sides share) and the SPI names."""
    libs = Path(install) / 'libraries'
    jars = mr.library_jars(libs)
    python = json.loads(json.dumps({'jars': [str(j) for j in jars], 'index': mr.build_library_index(jars), 'spi': sorted(mr.spi_names(jars))}))
    found = []
    with tempfile.TemporaryDirectory(prefix='java-analysis-compare-') as cache:
        mr.load_library_index(jars, cache)
        written = sorted(p.name for p in Path(cache).iterdir())
        rust = rust_json(binary, 'dump-libraries', '--install', install, '--cache', cache)
        if sorted(p.name for p in Path(cache).iterdir()) != written:
            found.append(('cache file name', written, sorted(p.name for p in Path(cache).iterdir())))
        fresh = rust_json(binary, 'dump-libraries', '--install', install)
    for label, got in (('cached', rust), ('built', fresh)):
        found += [(f'{label}: {p}', x, y) for p, x, y in differences('', python, got)]
    return len(python['index']), found


def compare_jars(install, binary):
    """Per archive of install/mods: mod id, label and jar facts."""
    jars = mr.library_jars(Path(install) / 'libraries')
    spis = mr.spi_names(jars)
    python = []
    for jar in sorted((Path(install) / 'mods').glob('*.jar')):
        for mod, label, archive, toml in mr.walk(zipfile.ZipFile(jar), None, jar.stem):
            data, at_lines, opaque, plugins = mr.jar_facts(archive, toml, spis)
            python.append({'mod': mod, 'label': label, 'facts': {'data': data, 'at_lines': at_lines, 'opaque': opaque, 'plugins': plugins}})
    python = json.loads(json.dumps(python))
    rust = rust_json(binary, 'dump-jars', '--install', install)
    found = []
    if len(python) != len(rust):
        found.append(('archive count', len(python), len(rust)))
    for i, (a, b) in enumerate(zip(python, rust)):
        found += [(f'{a["label"]}: {p}', x, y) for p, x, y in differences('', a, b)]
    return len(python), found


def compare_mixins(install, binary):
    """mixins.json as parsed JSON and mixins.md as text, from mod_parity.scan_install and the Rust command."""
    found = []
    with tempfile.TemporaryDirectory(prefix='java-analysis-compare-') as scratch:
        python_out, rust_out = Path(scratch) / 'python', Path(scratch) / 'rust'
        python = mp.scan_install(install, python_out)
        subprocess.run([str(binary), 'mixins', '--install', str(install), '--out', str(rust_out)], check=True)
        rust = json.loads((rust_out / 'mixins.json').read_text())
        if len(python) != len(rust):
            found.append(('entry count', len(python), len(rust)))
        found += [(f'mixins.json: {p}', x, y) for p, x, y in differences('', json.loads(json.dumps(python)), rust)]
        for name in ('mixins.json', 'mixins.md'):
            a, b = (python_out / name).read_text(), (rust_out / name).read_text()
            if a != b:
                found.append((f'{name} text', len(a), len(b)))
    return len(python), found


MECHANISMS = ('mixin', 'override', 'implement')


def knowledge_jar(scratch):
    """A jar holding the analysis/ resources `java_knowledge.write` makes, as the boot jar does."""
    folder = Path(scratch) / 'analysis'
    java_knowledge.write(folder)
    jar = Path(scratch) / 'knowledge.jar'
    with zipfile.ZipFile(jar, 'w') as z:
        for path in sorted(folder.iterdir()):
            z.write(path, f'analysis/{path.name}')
    return jar


def compare_knowledge(install, binary):
    """Native.info, verdict_of and route of every `methods` key of the Python replaces.json, then the folded lambdas' enclosing methods."""
    ledger = json.loads(mr.LEDGER_JSON.read_text())
    verdicts = mr.read_verdicts()
    native = mr.Native(ledger)
    found = []
    with tempfile.TemporaryDirectory(prefix='java-analysis-compare-') as scratch:
        out = Path(scratch) / 'out'
        out.mkdir()
        result = mr.analyse(install, out, ledger, Path(scratch) / 'cache', log=lambda *_: None, verdicts=verdicts)
        methods = result['methods']
        keys = sorted(methods)
        rust = rust_json(binary, 'dump-knowledge', '--boot-jar', knowledge_jar(scratch), *keys)
        for key in keys:
            info = native.info(key)
            expected = {**info, 'verdict': mr.verdict_of(verdicts, key), 'routes': {m: mr.route(m, info, key, verdicts) for m in MECHANISMS}}
            found += [(f'{key}: {p}', x, y) for p, x, y in differences('', json.loads(json.dumps(expected)), rust.get(key))]
        folds = {}
        for key, entry in methods.items():
            for change in entry['changes']:
                if change.get('folded'):
                    folds[f'{key.partition("#")[0]}#{change["folded"]}'] = [key.partition('#')[2], change['desc']]
        folded = rust_json(binary, 'dump-folds', '--install', install, *sorted(folds))
        for lambda_key, expected in sorted(folds.items()):
            found += [(f'{lambda_key}: {p}', x, y) for p, x, y in differences('', expected, folded.get(lambda_key))]
        return len(keys) + len(folds), found


UNCLASSIFIED = 'entity methods without a state group'


def run_both(install, binary, scratch):
    """mod_replaces.run and the Rust `replaces` command, each into its own folder under scratch; both may stop on unclassified entity methods."""
    python_out, rust_out, cache = Path(scratch) / 'python', Path(scratch) / 'rust', Path(scratch) / 'cache'
    args = argparse.Namespace(install=str(install), out=str(python_out), ledger=str(mr.LEDGER_JSON), cache=str(cache), mixins=None, routes_out=None)
    try:
        mr.run(args)
    except SystemExit as e:
        if not str(e.code).startswith(UNCLASSIFIED):
            raise
    run = subprocess.run([str(binary), 'replaces', '--install', str(install), '--boot-jar', str(knowledge_jar(scratch)), '--out', str(rust_out), '--cache', str(cache)],
                         capture_output=True, text=True)
    if run.returncode and UNCLASSIFIED not in run.stderr:
        sys.exit(f'horizon-java-analysis replaces failed ({run.returncode}): {run.stderr[-2000:]}')
    return python_out, rust_out


def compare_replaces(install, binary):
    """replaces.json (without `seconds` and `install`) and mixins/ from mod_replaces.run and the Rust command, each into its own temp folder."""
    found = []
    with tempfile.TemporaryDirectory(prefix='java-analysis-compare-') as scratch:
        python_out, rust_out = run_both(install, binary, scratch)
        python, rust = (json.loads((out / 'replaces.json').read_text()) for out in (python_out, rust_out))
        for doc in (python, rust):
            doc.pop('seconds'), doc.pop('install')
        found += [(f'replaces.json: {p}', x, y) for p, x, y in differences('', python, rust)]
        for name in ('mixins.json', 'mixins.md'):
            a, b = (python_out / 'mixins' / name).read_text(), (rust_out / 'mixins' / name).read_text()
            if a != b:
                found.append((f'mixins/{name} text', len(a), len(b)))
    return len(python['methods']) + len(python['events']), found


def compare_entity_guards(install, binary):
    """The entity type ids and the guards of every entity-event listener of the mods, keyed `mod|class|method|event`."""
    jars = mr.library_jars(Path(install) / 'libraries')
    hier = mr.Hierarchy(mr.load_library_index(jars, None))
    hier.jars = jars
    archives, classes = [], []
    for jar in sorted((Path(install) / 'mods').glob('*.jar')):
        for mod, _, archive, _ in mr.walk(zipfile.ZipFile(jar), None, jar.stem):
            archives.append((mod, archive))
            for entry in archive.namelist():
                if not entry.endswith('.class') or entry.startswith('META-INF/') or entry == 'module-info.class':
                    continue
                data = archive.read(entry)
                try:
                    head = mr.parse_class(data, members=False)
                except (ValueError, IndexError, struct.error):
                    continue
                hier.add(head['name'], head['super'], head['ifaces'])
                marked = b'addListener' in data or b'addGenericListener' in data or b'SubscribeEvent' in data
                classes.append((len(archives) - 1, entry, head['name'], marked))
    by_name = {name: (ai, entry) for ai, entry, name, _ in classes}
    cache = {}

    def infos(name):
        if name not in cache:
            ref = by_name.get(name)
            try:
                cache[name] = mr.parse_class(archives[ref[0]][1].read(ref[1]), annotations=True, code=True) if ref else None
            except (ValueError, IndexError, struct.error):
                cache[name] = None
        return cache[name]

    ids = ef.type_ids(hier)
    listeners = {}
    for ai, _, name, marked in classes:
        info = infos(name) if marked else None
        if info is None:
            continue
        for event, _, method, _, _, _, impl in mr.class_listeners(info, hier):
            if event is None or not (event.startswith(ef.EVENT_PREFIXES) or hier.is_a(event, ef.ENTITY_EVENT)):
                continue
            listeners[f'{archives[ai][0]}|{name.replace("/", ".")}|{method}|{event}'] = ef.listener_guards(infos, impl, ids)
    python = json.loads(json.dumps({'type_ids': ids, 'listeners': listeners}))
    rust = rust_json(binary, 'dump-entity-guards', '--install', install)
    found = [(f'entity-guards: {p}', x, y) for p, x, y in differences('', python, rust)]
    return len(listeners) + len(ids), found


def compare_entity_facts(install, binary):
    """entity-facts.json from mod_replaces.run and the Rust `replaces` command."""
    with tempfile.TemporaryDirectory(prefix='java-analysis-compare-') as scratch:
        python_out, rust_out = run_both(install, binary, scratch)
        python, rust = (json.loads((out / 'entity-facts.json').read_text()) for out in (python_out, rust_out))
    found = [(f'entity-facts.json: {p}', x, y) for p, x, y in differences('', python, rust)]
    return len(python['events']) + len(python['writes']) + len(python['routes']), found


STAGES = {
    'classes': (compare_classes, 'classes', 'the class-file reader'),
    'libraries': (compare_libraries, 'library classes', 'library jars, class index, SPI names'),
    'jars': (compare_jars, 'archives', 'mod archives and their jar facts'),
    'mixins': (compare_mixins, 'mixin handlers', 'the mixin handler list'),
    'knowledge': (compare_knowledge, 'keys', 'the port ledger rows, verdicts, routes and folded lambdas'),
    'replaces': (compare_replaces, 'methods and events', 'replaces.json: changes, listeners, routing, summary'),
    'entity-guards': (compare_entity_guards, 'type ids and guarded listeners', 'entity type ids and listener guards'),
    'entity-facts': (compare_entity_facts, 'events, writers and routed classes', 'entity-facts.json: events, reads, writes, hooks, routes, caps'),
}


def report(stage, install, binary, detail):
    """Runs one stage and prints its line; True when it found differences."""
    compare, unit, _ = STAGES[stage]
    count, found = compare(install, binary)
    print(f'{count} {unit}, {len(found)} differences' if detail else f'{stage}: {count} {unit}, {len(found)} differences')
    for where, python, rust in found[:SHOWN if detail else 3]:
        print(f'{"" if detail else "  "}{where}: {str(python)[:200]} vs {str(rust)[:200]}')
    return bool(found)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest='stage', required=True)
    for name, text in [(n, t) for n, (_, _, t) in STAGES.items()] + [('all', 'every stage, one line each')]:
        stage = sub.add_parser(name, help=text)
        stage.add_argument('--install', required=True)
        stage.add_argument('--bin', help='the horizon-java-analysis binary (default: the fast profile)')
    args = parser.parse_args()
    binary = rust_binary(args.bin)
    if args.stage == 'all':
        failed = [stage for stage in STAGES if report(stage, args.install, binary, False)]
    else:
        failed = report(args.stage, args.install, binary, True)
    sys.exit(1 if failed else 0)


if __name__ == '__main__':
    main()
