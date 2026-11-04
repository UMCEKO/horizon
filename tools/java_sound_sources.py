#!/usr/bin/env python3
"""Which vanilla sounds the Java client makes that native also makes, checked against boot/res/sound_sources.tsv.

    python3 tools/java_sound_sources.py [--jar client.jar] [--candidates OUT]

mods/horizon/src/java-mods-host/devtools/dev/horizonlab/javamods/devtools/Graph.java walks the vanilla client jar's call graph from `Minecraft#tick` and `Camera#tick`, cutting what
Horizon's mixins cancel, to every method that reaches a sound sink. Each candidate is the first caller that is not sound plumbing
(the rule `SoundOwners` applies to a stack at run time). A candidate is classified when the table has a row for it, or when the port
ledger says native does not own it (Java stays its owner). It is unclassified when native owns it and the table has no row: a new
duplicate sound, which tests/test_sound_sources.py fails on.
"""
import argparse
import re
import subprocess
import sys
import tempfile
from pathlib import Path

try:
    from . import mod_replaces as mr
except ImportError:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import mod_replaces as mr  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
JAR = ROOT / '.local' / 'minecraft' / '26.3' / 'jars' / 'client.jar'
GRAPH = ROOT / 'mods' / 'horizon' / 'src' / 'java-mods-host' / 'devtools' / 'dev' / 'horizonlab' / 'javamods' / 'devtools' / 'Graph.java'
TABLE = ROOT / 'mods' / 'horizon' / 'src' / 'java-mods-host' / 'boot' / 'res' / 'sound_sources.tsv'
CLOSURE = ROOT / 'mods' / 'horizon' / 'src' / 'java-mods-host' / 'boot' / 'res' / 'animate_closure.tsv'
OUT = ROOT / '.local' / 'sound-sources-candidates.tsv'

ENTRIES = ['net/minecraft/client/Minecraft#tick', 'net/minecraft/client/Camera#tick']
# What Horizon's mixins cancel in the Java client.
CUT = ['net/minecraft/client/Minecraft#handleKeybinds', 'net/minecraft/client/sounds/MusicManager#tick', 'net/minecraft/client/renderer/GameRenderer#tick',
       'net/minecraft/client/Minecraft#renderFrame', 'net/minecraft/client/player/LocalPlayer#move', 'net/minecraft/client/particle/ParticleEngine#createParticle',
       'net/minecraft/client/Minecraft#pauseGame', 'net/minecraft/client/Minecraft#setScreen', 'net/minecraft/network/Connection#tick']
LEVEL = 'net/minecraft/client/multiplayer/ClientLevel'
SINKS = ['net/minecraft/client/sounds/SoundManager#play', 'net/minecraft/client/sounds/SoundManager#playDelayed', 'net/minecraft/client/sounds/SoundManager#queueTickingSound',
         *(f'{LEVEL}#{m}' for m in ('playSound', 'playLocalSound', 'playSeededSound', 'playPlayerSound')),
         *(f'net/minecraft/world/level/Level#{m}' for m in ('playSound', 'playLocalSound', 'playSeededSound', 'playPlayerSound')), 'net/minecraft/world/entity/Entity#playSound']
SOUND_CLASSES = ('net/minecraft/client/sounds/SoundManager', 'net/minecraft/client/sounds/SoundEngine')
ENTITY = 'net/minecraft/world/entity/Entity'
PLUMBED = (LEVEL, 'net/minecraft/world/level/Level', ENTITY)


def source_method(name):
    match = re.fullmatch(r'lambda\$(.+)\$\d+', name)
    return match.group(1) if match else name


def plumbing(owner, name):
    """The same rule as `SoundOwners.plumbing`: the engine and the `play*` entry points, with the lambdas and mixin handlers inside them."""
    name = source_method(name)
    if owner in SOUND_CLASSES:
        return True
    if owner in PLUMBED:
        return name.startswith(('playSound', 'handler$')) or (owner != ENTITY and name in ('playLocalSound', 'playSeededSound', 'playPlayerSound'))
    return owner == 'net/minecraft/client/player/LocalPlayer' and name in ('playSound', 'playNotifySound')


def read_table(path=TABLE, closure=CLOSURE):
    """[(class, method or *, owner)] of the checked-in table, and the animate closure's block and fluid rows as native's."""
    rows = []
    for line in path.read_text().splitlines():
        if line and not line.startswith('#'):
            cls, method, owner, *_ = line.split('\t')
            rows.append((cls, method, owner))
    for line in closure.read_text().splitlines():
        role, cls, method, *_ = line.split('\t')
        if role in ('block', 'fluid'):
            rows.append((cls, method, 'native'))
    return rows


def table_owner(rows, owner, method):
    """'native' or 'java' from the first row naming this class (or its outer class) and method, else None."""
    for cls in dict.fromkeys((owner, owner.split('$')[0])):
        for row_cls, row_method, row_owner in rows:
            if row_cls == cls and row_method in ('*', method):
                return row_owner
    return None


def graph(jar=JAR):
    classes = Path(tempfile.mkdtemp(prefix='sound-graph-'))
    subprocess.run(['javac', '--release', '25', '-d', str(classes), str(GRAPH)], check=True)
    run = subprocess.run(['java', '-Xss64m', '-cp', str(classes), 'Graph', str(jar), ','.join(CUT), ','.join(SINKS), ','.join(ENTRIES)], check=True, capture_output=True, text=True)
    return run.stdout.splitlines()


def candidates(lines):
    """{(class, method): sink} of the first non-plumbing caller of each sink, over paths that stay in the client's code."""
    found = {}
    for line in lines:
        sink, *path = line.split(' <- ')
        if any(frame.startswith('net/minecraft/server/') for frame in path):
            continue
        for frame in path:
            owner, name = frame.split('#')
            if not plumbing(owner, name):
                found.setdefault((owner, source_method(name)), sink)
                break
    return found


def classify(found, rows, native):
    out = []
    for (owner, method), sink in sorted(found.items()):
        info = native.info(f'{owner}#{method}')
        listed = table_owner(rows, owner, method)
        out.append((owner, method, info['status'], listed or ('native-unlisted' if info['owned'] else 'java'), sink))
    return out


def run(jar=JAR, ledger=None):
    native = mr.Native(ledger or __import__('json').load(open(mr.LEDGER_JSON)))
    return classify(candidates(graph(jar)), read_table(), native)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument('--jar', default=str(JAR))
    parser.add_argument('--candidates', default=str(OUT))
    args = parser.parse_args()
    result = run(Path(args.jar))
    out = Path(args.candidates)
    out.parent.mkdir(parents=True, exist_ok=True)
    out.write_text(''.join('\t'.join(row) + '\n' for row in result))
    missing = [row for row in result if row[3] == 'native-unlisted']
    for row in missing:
        print(f'unclassified: {row[0]}#{row[1]} ({row[2]}) reaches {row[4]}')
    print(f'{out}: {len(result)} candidates, {len(missing)} unclassified')
    return 1 if missing else 0


if __name__ == '__main__':
    sys.exit(main())
