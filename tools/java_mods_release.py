"""A release-shaped Linux build with Java mods: the two release folders and archives, checked the way the release job checks its builds (docs/java-mods.md)."""
import atexit
import hashlib
import json
import os
import subprocess
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
GATE = Path('/home/umceko/hz-poc/agents/gate.py')
BUILD_IMAGE = 'horizon-release-build:ubuntu22'
SMOKE_IMAGE = 'horizon-release-smoke:ubuntu22'
VERSION = '0.0.0-java-mods-release'
CLIENT_DIR = 'horizon-linux-x86_64'
SERVER_DIR = 'horizon-server-linux-x86_64'
CLIENT_PROGRAMS = ('horizon-client', 'horizon-ctl', 'horizon-prepare', 'horizon-server')
SERVER_PROGRAMS = ('horizon-engine', 'horizon-server', 'horizon-prepare')
PORT = '25599'
BUILD_DOCKERFILE = f'''FROM ubuntu:22.04
ENV DEBIAN_FRONTEND=noninteractive CARGO_HOME=/usr/local/cargo RUSTUP_HOME=/usr/local/rustup PATH=/usr/local/cargo/bin:$PATH
RUN apt-get update && apt-get install -y --no-install-recommends curl ca-certificates build-essential pkg-config git python3 libasound2-dev libudev-dev binutils \\
 && curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable \\
 && rustup component add llvm-tools && chmod -R a+rwX /usr/local/cargo /usr/local/rustup
'''
SMOKE_DOCKERFILE = '''FROM ubuntu:22.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates libasound2 libudev1 libvulkan1 mesa-vulkan-drivers libxkbcommon0 libx11-6 libxcb1 libwayland-client0 libgl1 libegl1 \\
 && rm -rf /var/lib/apt/lists/*
'''

_containers = []


def rewrite_args(text, java, host):
    """The lines of an install's `horizon-java.args` with its `java` and `host` lines replaced; every other line kept, in order."""
    out = []
    for line in text.splitlines():
        head = line.split(' ', 1)[0]
        if head == 'java':
            out.append(f'java {java}')
        elif head == 'host':
            out.append(f'host {host}')
        else:
            out.append(line)
    return '\n'.join(out) + '\n'


def sha256sums(files):
    """The text `sha256sum` writes for `files`: `<64 hex>  <name>` per line."""
    return ''.join(f'{hashlib.sha256(Path(f).read_bytes()).hexdigest()}  {Path(f).name}\n' for f in files)


def step(name, results=None):
    class Timed:
        def __enter__(self):
            self.began = time.monotonic()
            print(f'==> {name}', flush=True)

        def __exit__(self, kind, value, trace):
            seconds = time.monotonic() - self.began
            print(f'<== {name}: {"ok" if kind is None else "FAILED"} in {seconds:.0f}s', flush=True)
            if results is not None:
                results[name] = kind is None
    return Timed()


def run(command, **kwargs):
    print('$', ' '.join(map(str, command)), flush=True)
    return subprocess.run(command, check=True, **kwargs)


def stop_containers():
    for name in _containers:
        subprocess.run(['docker', 'rm', '-f', name], capture_output=True)
    _containers.clear()


def docker_args(name, mounts, env=None, user=True, interactive=False):
    _containers.append(name)
    command = ['docker', 'run', '--rm', '--name', name]
    if interactive:
        command.append('-i')
    if user:
        command += ['--user', f'{os.getuid()}:{os.getgid()}']
    for source, target, mode in mounts:
        command += ['-v', f'{source}:{target}:{mode}']
    for key, value in (env or {}).items():
        command += ['-e', f'{key}={value}']
    return command


def build_image(tag, dockerfile):
    run(['docker', 'build', '-q', '-t', tag, '-'], input=dockerfile.encode())


def wait_gate(owner):
    while True:
        done = subprocess.run([sys.executable, str(GATE)], env={**os.environ, 'GATE_OWNER': owner}, capture_output=True, text=True)
        if done.returncode == 0:
            return
        time.sleep(20)


def scratch_env(home):
    names = {'HORIZON_HOME': 'home', 'HORIZON_LAUNCHER_HOME': 'launcher', 'XDG_DATA_HOME': 'xdg-data', 'XDG_CONFIG_HOME': 'xdg-config'}
    env = {}
    for name, folder in names.items():
        (home / folder).mkdir(parents=True, exist_ok=True)
        env[name] = f'/work/{folder}'
    env.update(HORIZON_LAUNCHER_NO_SELF_UPDATE='1', HORIZON_NO_AUDIO='1')
    return env


def host_build(root, jars):
    env = {**os.environ, 'HORIZON_JAVA_MODS_DIR': str(jars)}
    tool = [sys.executable, str(root / 'tools' / 'java_mods.py')]
    run([sys.executable, str(root / 'tools' / 'port_ledger.py'), 'build'], cwd=root, env=env)
    run([*tool, 'build'], cwd=root, env=env)


def host_exporters(root):
    run(['cargo', 'build', '--profile', 'fast', '-p', 'horizon-prepare'], cwd=root)
    prepare = root / 'target' / 'fast' / 'horizon-prepare'
    bundle = subprocess.run([str(prepare), 'compile-exporters', '--root', str(root)], check=True, cwd=root, capture_output=True, text=True).stdout.strip().splitlines()[-1]
    target = root / 'exporters'
    run(['rm', '-rf', str(target)])
    target.mkdir()
    for name in ('server', 'client', 'manifest.json'):
        run(['cp', '-R', str(Path(bundle) / name), str(target)])
    stray = [p for p in target.rglob('*') if p.is_file() and p != target / 'manifest.json'
             and not (p.suffix == '.class' and p.relative_to(target).parts[0] in ('server', 'client') and 'dev/horizonlab/parity' in p.as_posix())]
    if stray:
        sys.exit(f'not one of our classes: {stray[:5]}')
    run([sys.executable, str(root / 'tools' / 'check_prepare_exporters.py'), '--binary', str(prepare)], cwd=root)


BUILD_SCRIPT = f'''set -euo pipefail
git config --global --add safe.directory /src
cd /src
cargo build --release --locked -p horizon-official --bin horizon-official --features horizon-official/java-mods -p horizon-client-lib --bin horizon-ctl -p horizon-prepare --bin horizon-prepare -p horizon-server --bin horizon-server
cp -f /target/release/horizon-official /target/release/horizon-client
cargo run --locked -p xtask -- package /target/release/horizon-client /out/{CLIENT_DIR} --java-mods /jars
cargo build --release --locked -p horizon-client --no-default-features --features java-mods --bin horizon-client
cargo run --locked -p xtask -- package-server /target/release/horizon-client /out/{SERVER_DIR} --java-mods /jars
cd /out
rm -f {CLIENT_DIR}.tar.gz {SERVER_DIR}.tar.gz
tar czf {CLIENT_DIR}.tar.gz {CLIENT_DIR}
tar czf {SERVER_DIR}.tar.gz {SERVER_DIR}
'''

CHECK_SCRIPT = f'''set -euo pipefail
cd /src
client=/out/{CLIENT_DIR}; server=/out/{SERVER_DIR}
for b in {' '.join(CLIENT_PROGRAMS)}; do python3 tools/check_imports.py "$client/$b"; done
python3 tools/check_imports.py --server {' '.join(f'"$server/{p}"' for p in SERVER_PROGRAMS)}
python3 tools/check_cpu_baseline.py {' '.join(f'"$client/{p}"' for p in CLIENT_PROGRAMS)}
export HORIZON_HOME=$(mktemp -d) HORIZON_LAUNCHER_HOME=$(mktemp -d) XDG_DATA_HOME=$(mktemp -d) XDG_CONFIG_HOME=$(mktemp -d) HORIZON_LAUNCHER_NO_SELF_UPDATE=1 HORIZON_NO_AUDIO=1
"$client/horizon-client" --help | grep -q -- '--resources'
"$client/horizon-ctl" --help > /dev/null
"$client/horizon-prepare" --help | grep -q -- '--minecraft-dir'
for b in horizon-prepare horizon-server; do test "$("$client/$b" --version)" = "$b {VERSION}"; done
"$server/horizon-engine" --help | grep -q -- '--resources'
test "$("$server/horizon-server" --version)" = "horizon-server {VERSION}"
echo "check: libjvm links $(readelf -d "$client/horizon-client" "$server/horizon-engine" | grep -c libjvm || true)"
test "$(readelf -d "$client/horizon-client" "$server/horizon-engine" | grep -c libjvm || true)" = 0
for p in "$client/horizon-client" "$server/horizon-engine"; do test "$(grep -c horizon-java-mods-support "$p")" -ge 1; done
for d in "$client" "$server"; do for j in horizon-java-host horizon-java-boot horizon-java-plugin; do test -f "$d/java-mods/$j.jar"; done; done
'''


def container_build(root, out, jars, git_common, results):
    registry = Path.home() / '.cargo' / 'registry'
    registry.mkdir(parents=True, exist_ok=True)
    mounts = [(root, '/src', 'rw'), (out / 'target', '/target', 'rw'), (registry, '/usr/local/cargo/registry', 'rw'), (jars, '/jars', 'ro'), (out, '/out', 'rw')]
    if git_common.resolve() != (root / '.git').resolve():
        mounts.append((git_common, str(git_common), 'ro'))
    env = {'CARGO_TARGET_DIR': '/target', 'HORIZON_VERSION': VERSION, 'HOME': '/tmp'}
    with step('container: build and package both folders', results):
        run([*docker_args('hz-release-build', mounts, env), BUILD_IMAGE, 'bash', '-c', BUILD_SCRIPT])
        stop_containers()
    with step('container: imports, CPU baseline, starts, no libjvm, support mark, host jars', results):
        run([*docker_args('hz-release-check', mounts, env), BUILD_IMAGE, 'bash', '-c', CHECK_SCRIPT])
        stop_containers()


def write_smoke_report_check(path):
    report = json.loads(Path(path).read_text())
    if report.get('error') is not None or report.get('frames', 0) < 300 or not report.get('chunks') or not report.get('meshed_sections') or not report.get('player'):
        sys.exit(f'the smoke run did not draw a world: {report}')
    return report


def server_run(name, mounts, command, expect, folder, env, timeout=900):
    """Starts a dedicated server in a container, waits for each line in `expect`, sends `stop`, wants exit code 0."""
    folder.mkdir(parents=True, exist_ok=True)
    (folder / 'eula.txt').write_text('eula=true\n')
    proc = subprocess.Popen([*docker_args(name, mounts, env, interactive=True), SMOKE_IMAGE, *command], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.STDOUT, text=True, cwd=folder)
    seen, deadline = [], time.monotonic() + timeout
    pending = list(expect)
    log = (folder / 'server.log').open('w')
    for line in proc.stdout:
        log.write(line)
        seen.append(line)
        pending = [e for e in pending if e not in line]
        if not pending or time.monotonic() > deadline:
            break
    if pending:
        proc.kill()
        sys.exit(f'{name}: never logged {pending}; see {folder / "server.log"}')
    proc.stdin.write('stop\n')
    proc.stdin.flush()
    for line in proc.stdout:
        log.write(line)
    code = proc.wait(timeout=300)
    log.close()
    if code != 0:
        sys.exit(f'{name}: exit code {code}')
    return ''.join(seen)


def make_run_folder(install, kind, folder, packaged):
    """A run folder of `install`'s `kind` ('client' or 'server') using the packaged boot and plugin jars and host jar (paths as the container sees them)."""
    mods = folder / 'mods'
    mods.mkdir(parents=True, exist_ok=True)
    for old in mods.iterdir():
        old.unlink()
    for jar in sorted((install / 'mods').glob('*.jar')):
        (mods / jar.name).symlink_to(jar)
    for name in ('horizon-java-boot.jar', 'horizon-java-plugin.jar'):
        (mods / name).symlink_to(f'{packaged}/java-mods/{name}')
    library = folder / 'libraries'
    if library.is_symlink() or library.exists():
        library.unlink()
    library.symlink_to(install / 'libraries')
    jdk = next((install / 'jdk').glob('jdk-*'))
    text = (install / kind / 'horizon-java.args').read_text()
    (folder / 'horizon-java.args').write_text(rewrite_args(text, jdk, f'{packaged}/java-mods/horizon-java-host.jar'))


def smoke(root, out, resources, install, results):
    mounts_base = [(out, '/out', 'ro'), (resources, '/res', 'ro'), (install, str(install), 'ro')]
    smoke_dir = out / 'smoke'
    run(['rm', '-rf', str(smoke_dir)])
    with step('smoke without Java: the game', results):
        wait_gate('w15-release')
        home = smoke_dir / 'plain-client'
        env = scratch_env(home)
        home.mkdir(parents=True, exist_ok=True)
        run([*docker_args('hz-release-smoke-client', [(home, '/work', 'rw'), *mounts_base], env), '-w', '/work', SMOKE_IMAGE, f'/out/{CLIENT_DIR}/horizon-client', '--resources', '/res', '--offscreen',
             '--world', 'smoke', '--seed', '1', '--render-distance', '4', '--world-frames', '300', '--timeout-seconds', '600', '--smoke-report', '/work/smoke.json'])
        stop_containers()
        write_smoke_report_check(home / 'smoke.json')
    with step('smoke without Java: the server', results):
        wait_gate('w15-release')
        home = smoke_dir / 'plain-server'
        env = scratch_env(home)
        server_run('hz-release-smoke-server', [(home, '/work', 'rw'), *mounts_base], [f'/out/{SERVER_DIR}/horizon-engine', '--resources', '/res', '--dedicated', '--nogui', '--port', PORT], ['Done ('], home, {**env, 'HOME': '/work'}, 600)
        stop_containers()
    packaged = f'/out/{CLIENT_DIR}'
    with step('smoke with Java mods: the server', results):
        wait_gate('w15-release')
        runs = smoke_dir / 'server-run'
        make_run_folder(install, 'server', runs, packaged)
        home = smoke_dir / 'java-server'
        env = scratch_env(home)
        server_run('hz-release-java-server', [(home, '/work', 'rw'), (runs, '/runs/server', 'rw'), *mounts_base], [f'/out/{SERVER_DIR}/horizon-engine', '--resources', '/res', '--java-mods', '/runs/server', '--dedicated', '--nogui', '--port', PORT],
                   ['Horizon: routes:', 'Java mods ready', 'Done ('], home, {**env, 'HOME': '/work'})
        stop_containers()
    with step('smoke with Java mods: the game', results):
        wait_gate('w15-release')
        runs = smoke_dir / 'client-run'
        make_run_folder(install, 'client', runs, packaged)
        home = smoke_dir / 'java-client'
        home.mkdir(parents=True, exist_ok=True)
        env = scratch_env(home)
        run([*docker_args('hz-release-java-client', [(home, '/work', 'rw'), (runs, '/runs/client', 'rw'), *mounts_base], env), '-w', '/work', SMOKE_IMAGE, f'/out/{CLIENT_DIR}/horizon-client', '--resources', '/res', '--java-mods', '/runs/client',
             '--offscreen', '--world', 'smoke', '--seed', '1', '--render-distance', '4', '--world-frames', '300', '--timeout-seconds', '600', '--smoke-report', '/work/smoke.json'])
        stop_containers()
        write_smoke_report_check(home / 'smoke.json')


def release(args, neoforge):
    root = ROOT
    out = Path(args.out).resolve()
    atexit.register(stop_containers)
    results = {}
    with step('clean checkout'):
        if subprocess.run(['git', 'status', '--porcelain'], cwd=root, capture_output=True, text=True, check=True).stdout.strip():
            sys.exit('the checkout is not clean: commit or stash first')
        commit = subprocess.run(['git', 'rev-parse', 'HEAD'], cwd=root, capture_output=True, text=True, check=True).stdout.strip()
    jars = Path(os.environ.get('HORIZON_JAVA_MODS_DIR') or root / '.local' / 'java-mods')
    with step('host: Java side and knowledge'):
        host_build(root, jars)
    with step('host: exporters'):
        host_exporters(root)
    out.mkdir(parents=True, exist_ok=True)
    (out / 'target').mkdir(exist_ok=True)
    git_common = Path(subprocess.run(['git', 'rev-parse', '--path-format=absolute', '--git-common-dir'], cwd=root, capture_output=True, text=True, check=True).stdout.strip())
    try:
        build_image(BUILD_IMAGE, BUILD_DOCKERFILE)
        container_build(root, out, jars, git_common, results)
        if not args.no_smoke:
            build_image(SMOKE_IMAGE, SMOKE_DOCKERFILE)
            smoke(root, out, (root / '.local' / 'horizon-client' / '26.3').resolve(), Path(args.install).resolve(), results)
    finally:
        stop_containers()
        run(['rm', '-rf', str(root / 'exporters')])
    with step('checksums and release.json'):
        archives = [out / f'{CLIENT_DIR}.tar.gz', out / f'{SERVER_DIR}.tar.gz']
        (out / 'SHA256SUMS').write_text(sha256sums(archives))
        record = {'commit': commit, 'neoforge': neoforge, 'features': ['java-mods'], 'checks': results}
        (out / 'release.json').write_text(json.dumps(record, indent=2) + '\n')
        print(json.dumps(record, indent=2))
        if not args.keep_target:
            run(['rm', '-rf', str(out / 'target')])
