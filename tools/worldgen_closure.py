#!/usr/bin/env python3
"""Writes mods/horizon/src/java-mods-host/boot/res/worldgen_closure.tsv: per worldgen seam, the vanilla methods a patch of which changes it (W10 SPEC-5).

    python3 tools/worldgen_closure.py [--jar server-game.jar] [--out FILE]

Columns: seam, type (the registered type of `carver`, `feature`, `start` and `place` rows, else empty), class (internal name), method, descriptor.
At world start Java keeps the rows whose method a loaded mod patches (`Routing.patched`) and exports them as `routed`.
The rows are read from the jar's class files (mods/horizon/src/java-mods-host/devtools/dev/horizonlab/javamods/devtools/WorldgenClosure.java), so a version bump stays right.
"""
import argparse
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
JAR = ROOT / '.local' / 'minecraft' / '26.3' / 'jars' / 'server-game.jar'
TSV = ROOT / 'mods' / 'horizon' / 'src' / 'java-mods-host' / 'boot' / 'res' / 'worldgen_closure.tsv'
SOURCE = ROOT / 'mods' / 'horizon' / 'src' / 'java-mods-host' / 'devtools' / 'dev' / 'horizonlab' / 'javamods' / 'devtools' / 'WorldgenClosure.java'


def generate(jar=JAR):
    classes = Path(tempfile.mkdtemp(prefix='worldgen-closure-'))
    subprocess.run(['javac', '--release', '25', '-d', str(classes), str(SOURCE)], check=True)
    out = subprocess.run(['java', '-Xss16m', '-cp', str(classes), 'WorldgenClosure', str(jar)], check=True, capture_output=True, text=True).stdout
    return ''.join(sorted(line + '\n' for line in set(out.splitlines())))


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--jar', default=str(JAR))
    parser.add_argument('--out', default=str(TSV))
    args = parser.parse_args()
    Path(args.out).write_text(generate(args.jar))
    print(f'{args.out}: {len(Path(args.out).read_text().splitlines())} rows')


if __name__ == '__main__':
    sys.exit(main())
