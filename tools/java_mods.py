#!/usr/bin/env python3
"""Dev driver for horizon-java-host: real mod jars on real NeoForge classes in a JVM embedded in Rust (needs nix-shell -p jdk25 pkg-config)."""
import argparse
import json
import os
import re
import shutil
import subprocess
import sys
import threading
import time
from pathlib import Path

os.environ.setdefault('HORIZON_NEOFORGE', '26.3.0.99')
sys.path.insert(0, str(Path(__file__).resolve().parent))
import neoforge_oracle as oracle  # noqa: E402

NEOFORGE = oracle.NEOFORGE
CRATE = oracle.ROOT / 'crates' / 'horizon-java-host'
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
        sources = [str(p) for p in sorted((CRATE / 'java' / name).rglob('*.java'))]
        subprocess.run(['javac', '--release', '25', '-proc:none', '-nowarn', '-cp', f'{cp}:{extra}', '-d', str(classes), *sources], check=True)
        jar_args = ['jar', 'cf', str(jar), '-C', str(classes), '.']
        if (CRATE / 'java' / name / 'res').exists():
            jar_args += ['-C', str(CRATE / 'java' / name / 'res'), '.']
        subprocess.run(jar_args, check=True)
    env = {**os.environ, 'JAVA_HOME': java_home(), 'CARGO_TARGET_DIR': str(TARGET), 'RUSTC_WRAPPER': ''}
    subprocess.run(['cargo', 'build', '--release', '-q', '--manifest-path', str(CRATE / 'Cargo.toml')], check=True, env=env)


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
    return subprocess.run([str(TARGET / 'release' / 'horizon-java-host'), *ops], env=env, capture_output=True, text=True)


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
    args = parser.parse_args()
    if args.command == 'build':
        build()
    elif args.command == 'run':
        p = host(instance('run', args.jars, True), args.opts, args.ops, args.facts)
        print(p.stdout)
        print(p.stderr[-3000:], file=sys.stderr)
        sys.exit(p.returncode)
    else:
        bench(args)


if __name__ == '__main__':
    main()
