#!/usr/bin/env python3
"""Dev driver for horizon-java-host: real mod jars on real NeoForge classes in a JVM embedded in Rust (needs nix-shell -p jdk25 pkg-config)."""
import argparse
import collections
import io
import json
import os
import re
import shlex
import shutil
import struct
import subprocess
import sys
import tempfile
import threading
import time
import zipfile
from pathlib import Path

os.environ.setdefault('HORIZON_NEOFORGE', '26.3.0.99')
sys.path.insert(0, str(Path(__file__).resolve().parent))
import neoforge_oracle as oracle  # noqa: E402

NEOFORGE = oracle.NEOFORGE
SOURCES = oracle.ROOT / 'mods' / 'horizon' / 'src' / 'java-mods-host'
LOCAL = oracle.ROOT / '.local' / 'java-mods'
TARGET = LOCAL / 'target'
HOST_JAR = LOCAL / 'horizon-java-host.jar'
BOOT_JAR = LOCAL / 'horizon-java-boot.jar'


def java_home():
    if os.environ.get('JAVA_HOME'):
        return os.environ['JAVA_HOME']
    settings = subprocess.run(['java', '-XshowSettings:properties', '-version'], capture_output=True, text=True).stderr
    return re.search(r'java\.home = (.+)', settings).group(1).strip()


def build():
    home = oracle.install()
    LOCAL.mkdir(parents=True, exist_ok=True)
    cp = oracle.classpath(home)
    for name, jar, extra in (('host', HOST_JAR, ''), ('boot', BOOT_JAR, str(LOCAL / 'classes' / 'host'))):
        classes = LOCAL / 'classes' / name
        shutil.rmtree(classes, ignore_errors=True)
        classes.mkdir(parents=True)
        sources = [str(p) for p in sorted((SOURCES / name).rglob('*.java'))]
        subprocess.run(['javac', '--release', '25', '-proc:none', '-nowarn', '-cp', f'{cp}:{extra}', '-d', str(classes), *sources], check=True)
        jar_args = ['jar', 'cf', str(jar), '-C', str(classes), '.']
        if (SOURCES / name / 'res').exists():
            jar_args += ['-C', str(SOURCES / name / 'res'), '.']
        subprocess.run(jar_args, check=True)
    env = {**os.environ, 'JAVA_HOME': java_home(), 'CARGO_TARGET_DIR': str(TARGET), 'RUSTC_WRAPPER': ''}
    subprocess.run(['cargo', 'build', '--profile', 'fast', '-q', '-p', 'horizon-java-host'], check=True, env=env, cwd=oracle.ROOT)


def instance(name, jars, boot):
    home = oracle.install()
    run = LOCAL / 'runs' / name
    shutil.rmtree(run, ignore_errors=True)
    (run / 'mods').mkdir(parents=True)
    (run / 'libraries').symlink_to(home / 'libraries')
    (run / 'eula.txt').write_text('eula=true\n')
    (run / 'server.properties').write_text(oracle.SERVER_PROPERTIES)
    for jar in sorted(Path(jars).glob('*.jar')):
        (run / 'mods' / jar.name).symlink_to(jar.resolve())
    if boot:
        (run / 'mods' / BOOT_JAR.name).symlink_to(BOOT_JAR)
    return run


def host(run, opts, ops, facts=None):
    java = java_home()
    env = {**os.environ, 'JAVA_HOME': java, 'LD_LIBRARY_PATH': f'{java}/lib/server:{java}/lib', 'HORIZON_NEOFORGE': NEOFORGE,
           'HORIZON_JAVA_RUN': str(run), 'HORIZON_JAVA_HOST_JAR': str(HOST_JAR), 'HORIZON_JAVA_OPTS': opts}
    if facts:
        env['HORIZON_JAVA_FACTS'] = str(Path(facts).resolve())
    return subprocess.run([str(TARGET / 'fast' / 'horizon-java-host'), *ops], env=env, capture_output=True, text=True)


def stock(run):
    """Stock NeoForge to `Done`, its RSS sampled from outside."""
    shutil.rmtree(run / 'world', ignore_errors=True)
    args = f'libraries/net/neoforged/neoforge/{NEOFORGE}/unix_args.txt'
    began = time.perf_counter()
    p = subprocess.Popen(['java', f'@{args}', 'nogui'], cwd=run, stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True)
    lines = []
    threading.Thread(target=lambda: lines.extend(p.stdout), daemon=True).start()
    done, rss = None, 0
    while done is None and p.poll() is None:
        if any('Done (' in line for line in list(lines)):
            done = time.perf_counter() - began
            rss = int(re.search(r'VmRSS:\s+(\d+)', Path(f'/proc/{p.pid}/status').read_text()).group(1))
        time.sleep(0.05)
    if p.poll() is None:
        p.stdin.write('stop\n')
        p.stdin.flush()
    try:
        p.wait(60)
    except subprocess.TimeoutExpired:
        p.kill()
    return {'total_ms': round(done * 1000, 1) if done else None, 'rss_mb': rss // 1024}


def rss_mb(pid):
    return int(re.search(r'VmRSS:\s+(\d+)', Path(f'/proc/{pid}/status').read_text()).group(1)) // 1024


def play(args):
    """The real game with these jars in its JVM, in scratch folders, driven by horizon-ctl commands; each reply is printed."""
    scratch = Path(args.scratch or tempfile.mkdtemp(prefix='horizon-java-mods-'))
    java = java_home()
    env = {**os.environ, 'JAVA_HOME': java, 'HORIZON_NEOFORGE': NEOFORGE, 'HORIZON_JAVA_HOST_JAR': str(HOST_JAR), 'HORIZON_JAVA_OPTS': args.opts,
           'HORIZON_NO_AUDIO': '1', 'HORIZON_LAUNCHER_NO_SELF_UPDATE': '1',
           'LD_LIBRARY_PATH': f"{java}/lib/server:{java}/lib:/run/opengl-driver/lib:{os.environ.get('LD_LIBRARY_PATH', '')}"}
    if args.jars:
        env['HORIZON_JAVA_RUN'] = str(instance('play', args.jars, True))
    for name in ('HORIZON_HOME', 'HORIZON_LAUNCHER_HOME', 'XDG_DATA_HOME', 'XDG_CONFIG_HOME'):
        env[name] = str(scratch / name)
        Path(env[name]).mkdir(exist_ok=True)
    ctl = [str(oracle.ROOT / 'target' / 'fast' / 'horizon-ctl'), '--socket', str(scratch / 'ctl.sock')]
    launch = [*ctl, 'launch', '--world', 'J', '--offscreen', '--wait', '--resources', str(oracle.ROOT / '.local' / 'horizon-client' / '26.3')]
    for mod in args.dev_mod:
        launch += ['--dev-mod', str(Path(mod).resolve())]
    began = time.perf_counter()
    opened = subprocess.run(launch, env=env, check=True, cwd=scratch, capture_output=True, text=True)
    pid = json.loads(opened.stdout)['launch']['pid']
    print(f'world open after {time.perf_counter() - began:.1f} s, {rss_mb(pid)} MB', flush=True)
    try:
        for line in args.commands:
            reply = subprocess.run([*ctl, *shlex.split(line)], env=env, capture_output=True, text=True, cwd=scratch)
            print(f'> {line}\n{reply.stdout.strip()}{reply.stderr.strip()}', flush=True)
        print(f'{rss_mb(pid)} MB after the commands', flush=True)
    finally:
        subprocess.run([*ctl, 'quit'], env=env, capture_output=True, cwd=scratch)
    print(f'scratch folder: {scratch}')


def references(data):
    """The (class, method) pairs a class file's constant pool names."""
    count = struct.unpack('>H', data[8:10])[0]
    pool, at, k = [None] * count, 10, 1
    while k < count:
        tag = data[at]
        if tag == 1:
            size = struct.unpack('>H', data[at + 1:at + 3])[0]
            pool[k] = data[at + 3:at + 3 + size].decode('utf8', 'replace')
            at += 3 + size
        elif tag in (3, 4):
            at += 5
        elif tag in (5, 6):
            at, k = at + 9, k + 1
        elif tag in (7, 8, 16, 19, 20):
            pool[k] = struct.unpack('>H', data[at + 1:at + 3])[0]
            at += 3
        elif tag == 15:
            at += 4
        else:
            pool[k] = (tag, *struct.unpack('>HH', data[at + 1:at + 5]))
            at += 5
        k += 1
    return {(pool[pool[e[1]]], pool[pool[e[2]][1]]) for e in pool if isinstance(e, tuple) and e[0] in (10, 11)}


def jar_references(archive, found):
    for name in archive.namelist():
        if name.endswith('.class'):
            found |= references(archive.read(name))
        elif name.endswith('.jar'):
            jar_references(zipfile.ZipFile(io.BytesIO(archive.read(name))), found)


def surface(args):
    """Which vanilla methods these jars call, by what the port ledger knows of each: what could be served from Rust next."""
    ledger = json.loads((oracle.ROOT / '.local' / 'port-ledger' / 'ledger.json').read_text())
    methods, classes, fns = ledger['methods'], ledger['classes'], ledger['fns']
    users = {}
    for jar in sorted(Path(args.jars).rglob('*.jar')):
        found = set()
        jar_references(zipfile.ZipFile(jar), found)
        for owner, name in found:
            queue = [owner] if owner.startswith('net/minecraft/') else []
            while queue:
                current = queue.pop(0)
                if f'{current}#{name}' in methods:
                    users.setdefault(f'{current}#{name}', set()).add(jar.name)
                    break
                queue += classes.get(current, {}).get('extends', [])

    def status(key):
        for kind in ('ported', 'mapped', 'named', 'structural', 'linked', 'inferred', 'mentioned', 'skipped', 'absent'):
            if key in ledger[kind]:
                return kind
        return 'trivial' if methods[key].get('accessor') or methods[key].get('size', 99) <= 4 else 'none'

    def homes(key):
        found = set()
        for kind in ('ported', 'mapped', 'named', 'structural', 'linked'):
            entry = ledger[kind].get(key)
            ids = entry if isinstance(entry, list) else [entry['fn']] if entry and entry.get('fn') is not None else []
            for i in ids:
                file = fns[i]['file']
                found.add('game' if '/horizon-client/src/game' in file else 'client' if '/horizon-client' in file else 'library')
        return found

    print(f'{len(users)} vanilla methods called by {args.jars}')
    print(collections.Counter(status(k) for k in users).most_common())
    ported = [k for k in users if status(k) in ('ported', 'mapped', 'named', 'structural', 'linked')]
    print('their Rust ports live in', collections.Counter(next((h for h in ('library', 'client', 'game') if h in homes(k)), 'none') for k in ported).most_common())
    ported.sort(key=lambda k: (-len(users[k]), -methods[k].get('size', 0)))
    for key in ported[:args.top]:
        print(len(users[key]), methods[key].get('size', 0), key.removeprefix('net/minecraft/'), status(key), sorted(homes(key)))


def bench(args):
    cases = [c.split('=', 1) for c in args.case]
    runs = {c[0]: instance(f'bench-{c[0]}', args.jars, True) for c in cases}
    plain = instance('bench-stock', args.jars, False)
    out = Path(args.out)
    out.parent.mkdir(parents=True, exist_ok=True)
    with out.open('a') as f:
        for i in range(args.rounds):
            for name, opts in cases:
                load = float(Path('/proc/loadavg').read_text().split()[0])
                r = host(runs[name], opts, ['shapes'])
                m = re.search(r'total_ms=([\d.]+).*rss_point_kb=(\d+).*hwm_kb=(\d+)', r.stdout)
                row = {'case': name, 'round': i, 'load': load, 'ok': bool(m) and r.returncode == 0}
                if m:
                    row |= {'total_ms': float(m[1]), 'rss_mb': int(m[2]) // 1024, 'peak_mb': int(m[3]) // 1024}
                mark = re.search(r'MARKS (.*)', r.stdout)
                row['marks'] = mark[1] if mark else r.stderr[-400:]
                shapes = re.search(r'mode=.*', r.stdout)
                row['shapes'] = shapes[0][:400] if shapes else ''
                f.write(json.dumps(row) + '\n')
                f.flush()
                print(name, row.get('total_ms'), row.get('rss_mb'), row['shapes'][:150], flush=True)
            if args.stock:
                load = float(Path('/proc/loadavg').read_text().split()[0])
                row = {'case': 'stock', 'round': i, 'load': load, 'ok': True, **stock(plain)}
                f.write(json.dumps(row) + '\n')
                f.flush()
                print('stock', row['total_ms'], row['rss_mb'], flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('build')
    r = sub.add_parser('run')
    r.add_argument('--jars', required=True)
    r.add_argument('--opts', default='')
    r.add_argument('--facts')
    r.add_argument('ops', nargs='*')
    b = sub.add_parser('bench')
    b.add_argument('--jars', required=True)
    b.add_argument('--out', required=True)
    b.add_argument('--rounds', type=int, default=7)
    b.add_argument('--stock', action='store_true')
    b.add_argument('--case', action='append', default=[])
    g = sub.add_parser('play')
    g.add_argument('--jars', help='no JVM without it')
    g.add_argument('--scratch', help='reuse this folder: its world and settings')
    g.add_argument('--opts', default='--enable-native-access=ALL-UNNAMED')
    g.add_argument('--dev-mod', action='append', default=[])
    g.add_argument('commands', nargs='*')
    c = sub.add_parser('surface')
    c.add_argument('--jars', required=True)
    c.add_argument('--top', type=int, default=40)
    args = parser.parse_args()
    if args.command == 'build':
        build()
    elif args.command == 'run':
        p = host(instance('run', args.jars, True), args.opts, args.ops, args.facts)
        print(p.stdout)
        print(p.stderr[-3000:], file=sys.stderr)
        sys.exit(p.returncode)
    elif args.command == 'play':
        play(args)
    elif args.command == 'surface':
        surface(args)
    else:
        bench(args)


if __name__ == '__main__':
    main()
