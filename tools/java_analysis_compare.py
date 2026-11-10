#!/usr/bin/env python3
"""Dev only: compares the Rust mod analysis (crates/horizon-java-analysis) with the Python it ports, stage by stage.

    python3 tools/java_analysis_compare.py classes --install DIR [--bin PATH]

Writes only into a temp folder. Prints `N classes, M differences` and the first 20 differences; exits 1 on any.
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


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    sub = parser.add_subparsers(dest='stage', required=True)
    classes = sub.add_parser('classes', help='the class-file reader')
    classes.add_argument('--install', required=True)
    classes.add_argument('--bin', help='the horizon-java-analysis binary (default: the fast profile)')
    args = parser.parse_args()
    count, found = compare_classes(args.install, rust_binary(args.bin))
    print(f'{count} classes, {len(found)} differences')
    for where, python, rust in found[:SHOWN]:
        print(f'{where}: {str(python)[:200]} vs {str(rust)[:200]}')
    sys.exit(1 if found else 0)


if __name__ == '__main__':
    main()
