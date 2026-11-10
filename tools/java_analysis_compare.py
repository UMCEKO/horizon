#!/usr/bin/env python3
"""Dev only: compares the Rust mod analysis (crates/horizon-java-analysis) with the Python it ports, stage by stage.

    python3 tools/java_analysis_compare.py classes|libraries|jars --install DIR [--bin PATH]

Writes only into a temp folder. Prints `N <units>, M differences` and the first 20 differences; exits 1 on any.
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
import lab  # noqa: E402
import mod_parity_explain as ex  # noqa: E402
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


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest='stage', required=True)
    for name, text in (('classes', 'the class-file reader'), ('libraries', 'library jars, class index, SPI names'), ('jars', 'mod archives and their jar facts')):
        stage = sub.add_parser(name, help=text)
        stage.add_argument('--install', required=True)
        stage.add_argument('--bin', help='the horizon-java-analysis binary (default: the fast profile)')
    args = parser.parse_args()
    unit = {'classes': 'classes', 'libraries': 'library classes', 'jars': 'archives'}[args.stage]
    compare = {'classes': compare_classes, 'libraries': compare_libraries, 'jars': compare_jars}[args.stage]
    count, found = compare(args.install, rust_binary(args.bin))
    print(f'{count} {unit}, {len(found)} differences')
    for where, python, rust in found[:SHOWN]:
        print(f'{where}: {str(python)[:200]} vs {str(rust)[:200]}')
    sys.exit(1 if found else 0)


if __name__ == '__main__':
    main()
