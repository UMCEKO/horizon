#!/usr/bin/env python3
"""Writes mods/horizon/src/java-mods-host/boot/res/animate_closure.tsv: the vanilla methods a patch of which changes `ClientLevel.animateTick`.

    python3 tools/animate_closure.py [--jar client.jar] [--out FILE]

Columns: role, class (internal name), method, descriptor, and for `block` and `fluid` rows the classes whose states the row serves.
  loop   a patch changes the whole loop: Java runs vanilla's loop and native's is off (`ClientLevel`'s loop methods, their effect sinks, the vanilla methods the loop calls)
  block  a patch changes the states of the block classes in the last column: every class below `Block` declaring `animateTick`, and the methods only those call
  fluid  the same for classes below `Fluid`
The rows are read from the jar's class files (mods/horizon/src/java-mods-host/devtools/dev/horizonlab/javamods/devtools/Closure.java), so a version bump stays right.
"""
import argparse
import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
JAR = ROOT / '.local' / 'minecraft' / '26.3' / 'jars' / 'client.jar'
TSV = ROOT / 'mods' / 'horizon' / 'src' / 'java-mods-host' / 'boot' / 'res' / 'animate_closure.tsv'
SOURCE = ROOT / 'mods' / 'horizon' / 'src' / 'java-mods-host' / 'devtools' / 'dev' / 'horizonlab' / 'javamods' / 'devtools' / 'Closure.java'


def generate(jar=JAR):
    classes = Path(tempfile.mkdtemp(prefix='animate-closure-'))
    subprocess.run(['javac', '--release', '25', '-d', str(classes), str(SOURCE)], check=True)
    out = subprocess.run(['java', '-Xss16m', '-cp', str(classes), 'Closure', str(jar)], check=True, capture_output=True, text=True).stdout
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
