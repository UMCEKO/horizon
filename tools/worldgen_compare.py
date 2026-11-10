#!/usr/bin/env python3
"""Seed comparison of Horizon's world generation with a real NeoForge server on the same jars (W10 SPEC-1)."""
import argparse
import json
import os
import shlex
import shutil
import signal
import subprocess
import sys
import time
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
TOOLS = ROOT / 'tools'
SOURCE = ROOT / 'mods/horizon/src/java-mods-host/parity/worldgen'
NEOFORGE = '26.3.0.101'
LIMIT = 20
LEVEL = 'minecraft:overworld'
HEIGHTMAPS = ('WORLD_SURFACE', 'OCEAN_FLOOR', 'MOTION_BLOCKING', 'MOTION_BLOCKING_NO_LEAVES')
GATE = os.environ.get('HORIZON_GAME_GATE', '')


# Diff

def keyed(rows, key):
    return {key(r): r for r in rows}


def part(total, found, first, **extra):
    return {'compared': total, 'differences': found, 'rate': (found / total) if total else 0.0, 'first': first[:LIMIT], **extra}


def diff_biomes(forge, horizon):
    """Per point: Horizon's biome against Forge's; a point Horizon did not answer counts."""
    theirs = {tuple(r[:3]): r[3] for r in horizon}
    first = []
    found = 0
    for x, y, z, biome in forge:
        got = theirs.get((x, y, z))
        if got != biome:
            found += 1
            first.append({'at': [x, y, z], 'forge': biome, 'horizon': got})
    return part(len(forge), found, first)


def diff_starts(forge, horizon):
    """Starts as sets keyed by (structure, chunk), then their box and pieces."""
    ours = keyed(forge, lambda s: (s['structure'], tuple(s['chunk'])))
    theirs = keyed(horizon, lambda s: (s['structure'], tuple(s['chunk'])))
    missing = [{'structure': k[0], 'chunk': list(k[1])} for k in ours if k not in theirs]
    extra = [{'structure': k[0], 'chunk': list(k[1])} for k in theirs if k not in ours]
    boxes, pieces = [], []
    for k, s in ours.items():
        h = theirs.get(k)
        if h is None:
            continue
        if s['box'] != h['box']:
            boxes.append({'structure': k[0], 'chunk': list(k[1]), 'forge': s['box'], 'horizon': h['box']})
        if s['pieces'] != h['pieces']:
            pieces.append({'structure': k[0], 'chunk': list(k[1]), 'forge': s['pieces'][:LIMIT], 'horizon': h['pieces'][:LIMIT],
                           'counts': [len(s['pieces']), len(h['pieces'])]})
    found = len(missing) + len(extra) + len(boxes) + len(pieces)
    return part(len(ours), found, missing + extra + boxes + pieces, missing=len(missing), extra=len(extra),
                box_differences=len(boxes), piece_differences=len(pieces),
                missing_first=missing[:LIMIT], extra_first=extra[:LIMIT])


def diff_chunks(forge, horizon):
    """Per chunk: the sections whose hash differs, each heightmap's differing columns, the block entity list."""
    theirs = keyed(horizon, lambda c: (c['x'], c['z']))
    first, found = [], 0
    for c in forge:
        h = theirs.get((c['x'], c['z']))
        if h is None:
            found += 1
            first.append({'chunk': [c['x'], c['z']], 'missing': True})
            continue
        sections = [i for i in range(max(len(c['sections']), len(h['sections'])))
                    if i >= len(c['sections']) or i >= len(h['sections']) or c['sections'][i] != h['sections'][i]]
        maps = {name: sum(1 for a, b in zip(c['heightmaps'][name], h['heightmaps'].get(name, [])) if a != b)
                + abs(len(c['heightmaps'][name]) - len(h['heightmaps'].get(name, [])))
                for name in HEIGHTMAPS if name in c['heightmaps']}
        maps = {k: v for k, v in maps.items() if v}
        entities = c['block_entities'] != h['block_entities']
        if sections or maps or entities:
            found += 1
            entry = {'chunk': [c['x'], c['z']], 'sections': sections, 'heightmap_columns': maps}
            if entities:
                entry['block_entities'] = {'forge': c['block_entities'][:LIMIT], 'horizon': h['block_entities'][:LIMIT]}
            first.append(entry)
    return part(len(forge), found, first)


def diff(forge, horizon):
    """The whole comparison: per dimension the three parts, and the total difference count."""
    result = {'seed': {'forge': forge.get('seed'), 'horizon': horizon.get('seed')}, 'dimensions': {}, 'differences': 0}
    if forge.get('seed') != horizon.get('seed'):
        result['differences'] += 1
    for level, ours in forge.get('dimensions', {}).items():
        theirs = horizon.get('dimensions', {}).get(level)
        if theirs is None:
            result['dimensions'][level] = {'missing_in_horizon': True}
            result['differences'] += 1
            continue
        parts = {'biomes': diff_biomes(ours['biomes'], theirs['biomes']),
                 'starts': diff_starts(ours['starts'], theirs['starts']),
                 'chunks': diff_chunks(ours['chunks'], theirs['chunks'])}
        result['dimensions'][level] = parts
        result['differences'] += sum(p['differences'] for p in parts.values())
    return result


def summary(result):
    lines = [f'seed forge={result["seed"]["forge"]} horizon={result["seed"]["horizon"]}']
    for level, parts in result['dimensions'].items():
        if 'missing_in_horizon' in parts:
            lines.append(f'{level}: Horizon made no answer')
            continue
        for name, p in parts.items():
            lines.append(f'{level} {name}: {p["differences"]} of {p["compared"]} differ ({p["rate"]:.1%})')
    lines.append(f'{result["differences"]} differences')
    return '\n'.join(lines)


# Running the two sides

def java_home(given=None):
    """A JDK 25 (Minecraft 26.3 needs it): --java, JAVA_HOME, then the PATH's."""
    for home in (given, os.environ.get('JAVA_HOME')):
        if home and (Path(home) / 'bin/java').exists():
            return Path(home)
    found = shutil.which('java')
    if found:
        return Path(found).resolve().parent.parent
    sys.exit('no java: pass --java HOME (a JDK 25) or set JAVA_HOME')


def oracle():
    os.environ['HORIZON_NEOFORGE'] = NEOFORGE
    sys.path.insert(0, str(TOOLS))
    import neoforge_oracle
    return neoforge_oracle


def build_dump_jar(home, java, out):
    """The dump mod, compiled against the server's classpath; rebuilt when its sources are newer."""
    jar = out / 'horizon-worldgen-dump.jar'
    sources = sorted((SOURCE / 'dev').rglob('*.java'))
    resources = sorted(p for p in (SOURCE / 'res').rglob('*') if p.is_file())
    if jar.exists() and jar.stat().st_mtime > max(p.stat().st_mtime for p in sources + resources):
        return jar
    classes = out / 'classes'
    shutil.rmtree(classes, ignore_errors=True)
    classes.mkdir(parents=True)
    javac = subprocess.run([str(java / 'bin/javac'), '--release', '25', '-proc:none', '-nowarn', '-cp', oracle().classpath(home),
                            '-d', str(classes), *map(str, sources)], capture_output=True, text=True)
    if javac.returncode:
        sys.exit(f'the dump mod does not compile:\n{javac.stdout}{javac.stderr}')
    with zipfile.ZipFile(jar, 'w', zipfile.ZIP_DEFLATED) as z:
        for p in sorted(classes.rglob('*.class')):
            z.write(p, p.relative_to(classes).as_posix())
        for p in resources:
            z.write(p, p.relative_to(SOURCE / 'res').as_posix())
    return jar


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


def kill_group(proc):
    if proc.poll() is None:
        try:
            os.killpg(proc.pid, signal.SIGKILL)
        except ProcessLookupError:
            pass
        proc.wait()


def run_forge(args, request, out):
    """A NeoForge server on the install's mods and the dump mod; its dump is OUT/forge.json."""
    neo = oracle()
    java = java_home(args.java)
    home = neo.install()
    jar = build_dump_jar(home, java, out)
    run = out / 'forge'
    shutil.rmtree(run, ignore_errors=True)
    (run / 'mods').mkdir(parents=True)
    neo.link(run / 'libraries', home / 'libraries')
    (run / 'eula.txt').write_text('eula=true\n')
    (run / 'server.properties').write_text(
        f'level-seed={args.seed}\nonline-mode=false\nmax-tick-time=-1\nserver-port=0\nview-distance=2\nsimulation-distance=2\n'
        'spawn-monsters=false\nspawn-animals=false\nspawn-protection=0\n')
    for mod in [*sorted(Path(args.install).glob('mods/*.jar')), jar]:
        neo.link(run / 'mods' / mod.name, mod.resolve())
    dump = out / 'forge.json'
    dump.unlink(missing_ok=True)
    (out / 'request.json').write_text(json.dumps(request))
    command = [str(java / 'bin/java'), '-Xmx4G', f'-Dhorizon.wg.request={out / "request.json"}', f'-Dhorizon.wg.out={dump}',
               f'@{neo.args_file(home)}', 'nogui']
    if not acquire_gate('worldgen_compare'):
        sys.exit('the game gate did not open')
    with (run / 'server.log').open('w') as log:
        proc = subprocess.Popen(command, cwd=run, stdin=subprocess.DEVNULL, stdout=log, stderr=subprocess.STDOUT, start_new_session=True,
                                env=dict(os.environ, JAVA_HOME=str(java)))
        try:
            proc.wait(timeout=args.timeout)
        except subprocess.TimeoutExpired:
            sys.exit(f'the Forge server did not finish in {args.timeout} s; see {run / "server.log"}')
        finally:
            kill_group(proc)
    if not dump.exists():
        tail = '\n'.join((run / 'server.log').read_text(errors='replace').splitlines()[-30:])
        sys.exit(f'the Forge server wrote no dump; the end of {run / "server.log"}:\n{tail}')
    return json.loads(dump.read_text())


def game_binary(given):
    if given:
        return Path(given)
    for base in (ROOT / '.local/parity-target', ROOT / 'target'):
        for profile in ('fast', 'debug', 'release'):
            if (base / profile / 'horizon-client').exists():
                return base / profile / 'horizon-client'
    sys.exit('no horizon-client binary: build one or pass --game')


def ctl_json(ctl, socket, env, *words, timeout=900):
    done = subprocess.run([str(ctl), '--socket', str(socket), '--timeout', str(timeout), *words], env=env, capture_output=True, text=True, timeout=timeout)
    try:
        return json.loads(done.stdout)
    except ValueError:
        raise RuntimeError(f'horizon-ctl {" ".join(words)[:80]} answered {done.stdout[-300:]!r} {done.stderr[-300:]!r}')


def run_horizon(args, request, out):
    """Horizon's own answers, from a new world of the seed under scratch folders, through `horizon-ctl worldgen`."""
    game = game_binary(args.game)
    ctl = game.parent / 'horizon-ctl'
    run = out / 'horizon'
    shutil.rmtree(run, ignore_errors=True)
    scratch = {name: run / name for name in ('game', 'home', 'launcher', 'xdg-data', 'xdg-config')}
    for path in scratch.values():
        path.mkdir(parents=True)
    socket = run / 'ctl.sock'
    resources = os.environ.get('HORIZON_TEST_RESOURCES') or str(ROOT / '.local/horizon-client' / '26.3')
    env = dict(os.environ, HORIZON_HOME=str(scratch['home']), HORIZON_LAUNCHER_HOME=str(scratch['launcher']),
               XDG_DATA_HOME=str(scratch['xdg-data']), XDG_CONFIG_HOME=str(scratch['xdg-config']),
               HORIZON_LAUNCHER_NO_SELF_UPDATE='1', HORIZON_NO_AUDIO='1', DISPLAY=os.environ.get('HZ_DISPLAY', ':99'))
    env.pop('WAYLAND_DISPLAY', None)
    if not acquire_gate('worldgen_compare'):
        sys.exit('the game gate did not open')
    result = {}
    try:
        launched = ctl_json(ctl, socket, env, 'launch', '--world', 'wgcompare', '--creative', '--seed', str(args.seed), '--headless',
                            '--wait', '--game', str(game), '--resources', resources, '--game-dir', str(scratch['game']))
        (run / 'launch.json').write_text(json.dumps(launched))
        if 'error' in launched:
            sys.exit(f'launch failed: {launched["error"]}; see {run / "ctl.log"}')
        ctl_json(ctl, socket, env, 'wait', '100')
        for name in ('biomes', 'starts', 'chunks'):
            result[name] = ctl_json(ctl, socket, env, 'worldgen', name, json.dumps(request))
    finally:
        try:
            ctl_json(ctl, socket, env, 'quit', timeout=60)
        except Exception:
            pass
        for pid in subprocess.run(['pgrep', '-f', f'--game-dir {scratch["game"]}'], capture_output=True, text=True).stdout.split():
            try:
                os.kill(int(pid), signal.SIGKILL)
            except ProcessLookupError:
                pass
    for name, value in result.items():
        if isinstance(value, dict) and 'error' in value:
            sys.exit(f'horizon-ctl worldgen {name}: {value["error"]}')
    return {'seed': args.seed, 'dimensions': {LEVEL: result}}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--install', required=True, help='a folder whose mods/*.jar both sides load (Forge; Horizon when it has the Java layer)')
    parser.add_argument('--seed', type=int, required=True)
    parser.add_argument('--out', required=True)
    parser.add_argument('--square', type=int, default=2048, help='biome square, blocks')
    parser.add_argument('--step', type=int, default=4, help='chunks between biome samples')
    parser.add_argument('--starts', type=int, default=64, help='structure start square, chunks')
    parser.add_argument('--chunks', type=int, default=5, help='force-loaded square, chunks')
    parser.add_argument('--skip-forge', action='store_true', help='reuse OUT/forge.json')
    parser.add_argument('--skip-horizon', action='store_true', help='reuse OUT/horizon.json')
    parser.add_argument('--game', help='a horizon-client binary; horizon-ctl is taken from beside it')
    parser.add_argument('--java', help='a JDK 25 home (default JAVA_HOME, then the PATH)')
    parser.add_argument('--timeout', type=int, default=3600, help='seconds for the Forge server')
    args = parser.parse_args()
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    request = {'square': args.square, 'step': args.step, 'starts': args.starts, 'chunks': args.chunks}
    sides = {}
    for name, skip, run in (('forge', args.skip_forge, run_forge), ('horizon', args.skip_horizon, run_horizon)):
        path = out / f'{name}.json'
        if not skip:
            path.write_text(json.dumps(run(args, request, out)))
        sides[name] = json.loads(path.read_text())
    result = diff(sides['forge'], sides['horizon'])
    (out / 'diff.json').write_text(json.dumps(result, indent=1))
    print(summary(result))
    return 1 if result['differences'] else 0


if __name__ == '__main__':
    sys.exit(main())
