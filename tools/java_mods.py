#!/usr/bin/env python3
"""Dev driver for horizon-java-host: real mod jars on real NeoForge classes in a JVM embedded in Rust (needs nix-shell -p jdk25 pkg-config)."""
import argparse
import collections
import concurrent.futures
import fcntl
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
import urllib.parse
import urllib.request
import zipfile
from pathlib import Path

os.environ.setdefault('HORIZON_NEOFORGE', '26.3.0.99')
sys.path.insert(0, str(Path(__file__).resolve().parent))
import lab  # noqa: E402
import neoforge_oracle as oracle  # noqa: E402

NEOFORGE = oracle.NEOFORGE
SOURCES = oracle.ROOT / 'mods' / 'horizon' / 'src' / 'java-mods-host'
LOCAL = oracle.ROOT / '.local' / 'java-mods'
TARGET = LOCAL / 'target'
HOST_JAR = LOCAL / 'horizon-java-host.jar'
BOOT_JAR = LOCAL / 'horizon-java-boot.jar'
PLUGIN_JAR = LOCAL / 'horizon-java-plugin.jar'
TEST_JAR = LOCAL / 'horizon-java-test.jar'
LIBRARIES = LOCAL / 'library-jars'
# A game's JVM boots the client side, as a NeoForge client does; a dedicated Horizon's would boot the server side.
DIST = os.environ.get('HORIZON_JAVA_DIST', 'client')


def java_home():
    if os.environ.get('JAVA_HOME'):
        return os.environ['JAVA_HOME']
    settings = subprocess.run(['java', '-XshowSettings:properties', '-version'], capture_output=True, text=True).stderr
    return re.search(r'java\.home = (.+)', settings).group(1).strip()


def build(java_only=False):
    LOCAL.mkdir(parents=True, exist_ok=True)
    # Against the client, which holds the server's classes too.
    cp = ':'.join(jar for jar in oracle.client_classpath(oracle.client()[0]).split(':') if 'installertools' not in jar)
    host_classes = str(LOCAL / 'classes' / 'host')
    for name, jar, extra in (('host', HOST_JAR, ''), ('plugin', PLUGIN_JAR, host_classes), ('boot', BOOT_JAR, host_classes), ('testmod', TEST_JAR, '')):
        classes = LOCAL / 'classes' / name
        shutil.rmtree(classes, ignore_errors=True)
        classes.mkdir(parents=True)
        sources = [str(p) for p in sorted((SOURCES / name).rglob('*.java'))]
        subprocess.run(['javac', '--release', '25', '-proc:none', '-nowarn', '-cp', f'{cp}:{extra}', '-d', str(classes), *sources], check=True)
        jar_args = ['jar', 'cf', str(jar), '-C', str(classes), '.']
        if name == 'plugin':
            manifest = LOCAL / 'plugin.manifest'
            manifest.write_text('FMLModType: LIBRARY\n')
            jar_args = ['jar', 'cfm', str(jar), str(manifest), '-C', str(classes), '.']
        if (SOURCES / name / 'res').exists():
            jar_args += ['-C', str(SOURCES / name / 'res'), '.']
        subprocess.run(jar_args, check=True)
    if java_only:
        return
    env = {**os.environ, 'JAVA_HOME': java_home(), 'CARGO_TARGET_DIR': str(TARGET), 'RUSTC_WRAPPER': ''}
    subprocess.run(['cargo', 'build', '--profile', 'fast', '-q', '-p', 'horizon-java-host'], check=True, env=env, cwd=oracle.ROOT)


def write_args(run, home):
    """What the host starts the JVM with, in absolute paths, as NeoForge's launcher arguments would be."""
    lines = []
    if DIST == 'client':
        _, neo, _, jars = oracle.client()
        lines.append('dist client')
        lines.append('cp ' + ':'.join(map(str, jars)))
        jvm = [a.replace('${library_directory}', str(home / 'libraries')) for a in neo['arguments']['jvm']]
        game = neo['arguments']['game']
    else:
        args = (home / 'libraries' / 'net/neoforged/neoforge' / NEOFORGE / 'unix_args.txt').read_text().splitlines()
        libs = str(home / 'libraries')
        at = args.index('-classpath')
        lines.append('cp ' + ':'.join(f'{home}/{entry}' for entry in args[at + 1].strip().split(':')))
        jvm, game = [], []
        for line in args[:at] + args[at + 2:]:
            head, _, rest = line.strip().partition(' ')
            if head in ('--add-opens', '--add-exports'):
                jvm += [head, rest]
            elif head.startswith('--fml'):
                game += [head, rest]
            elif head.startswith('-'):
                jvm.append(head.replace('=libraries', f'={libs}'))
    i = 0
    while i < len(jvm):
        if jvm[i] in ('--add-opens', '--add-exports'):
            lines.append(f'jvm {jvm[i]}={jvm[i + 1]}')
            i += 2
        else:
            lines.append(f'jvm {jvm[i]}')
            i += 1
    lines += [f'prog {a}' for a in game]
    (run / 'horizon-java.args').write_text('\n'.join(lines) + '\n')


def instance(name, jars, boot):
    home = oracle.client()[0] if DIST == 'client' and boot else oracle.install()
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
        (run / 'mods' / PLUGIN_JAR.name).symlink_to(PLUGIN_JAR)
        write_args(run, home)
    return run


def host(run, opts, ops, facts=None):
    java = java_home()
    env = {**os.environ, 'JAVA_HOME': java, 'LD_LIBRARY_PATH': f'{java}/lib/server:{java}/lib', 'HORIZON_NEOFORGE': NEOFORGE,
           'HORIZON_JAVA_RUN': str(run), 'HORIZON_JAVA_HOST_JAR': str(HOST_JAR), 'HORIZON_JAVA_OPTS': opts}
    if facts:
        env['HORIZON_JAVA_FACTS'] = str(Path(facts).resolve())
    return subprocess.run([str(TARGET / 'fast' / 'horizon-java-host'), *ops], env=env, cwd=run, capture_output=True, text=True)


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


def session(args, drive):
    """The real game with these jars in its JVM, in scratch folders; `drive` sends it horizon-ctl commands."""
    scratch = Path(args.scratch or tempfile.mkdtemp(prefix='horizon-java-mods-'))
    java = java_home()
    homes = {name: scratch / name for name in ('HORIZON_HOME', 'HORIZON_LAUNCHER_HOME', 'XDG_DATA_HOME', 'XDG_CONFIG_HOME')}
    for home in homes.values():
        home.mkdir(exist_ok=True)
    env = lab.game_env(JAVA_HOME=java, HORIZON_NEOFORGE=NEOFORGE, HORIZON_JAVA_HOST_JAR=HOST_JAR, HORIZON_JAVA_OPTS=args.opts,
                       LD_LIBRARY_PATH=f"{java}/lib/server:{java}/lib:/run/opengl-driver/lib:{os.environ.get('LD_LIBRARY_PATH', '')}", **homes)
    if args.jars:
        env['HORIZON_JAVA_RUN'] = str(instance('play', args.jars, True))
    ctl = [str(oracle.ROOT / 'target' / 'fast' / 'horizon-ctl'), '--socket', str(scratch / 'ctl.sock')]
    launch = [*ctl, 'launch', '--world', 'J', '--seed', str(args.seed), '--offscreen', '--wait', '--resources', str(oracle.ROOT / '.local' / 'horizon-client' / '26.3')]
    for mod in args.dev_mod:
        launch += ['--dev-mod', str(Path(mod).resolve())]
    began = time.perf_counter()
    opened = subprocess.run(launch, env=env, check=True, cwd=scratch, capture_output=True, text=True)
    pid = json.loads(opened.stdout)['launch']['pid']
    print(f'world open after {time.perf_counter() - began:.1f} s, {rss_mb(pid)} MB', flush=True)

    def send(line):
        reply = subprocess.run([*ctl, *shlex.split(line)], env=env, capture_output=True, text=True, cwd=scratch)
        return reply.stdout.strip() + reply.stderr.strip()

    try:
        drive(send)
        print(f'{rss_mb(pid)} MB after the commands', flush=True)
    finally:
        subprocess.run([*ctl, 'quit'], env=env, capture_output=True, cwd=scratch)
        while Path(f'/proc/{pid}').exists():
            time.sleep(0.1)
    print(f'scratch folder: {scratch}')


def play(args):
    def drive(send):
        for line in args.commands:
            print(f'> {line}\n{send(line)}', flush=True)

    session(args, drive)


def bridges(args):
    """The test mod's wand on a real click: each block of the column it builds says one bridge answered as vanilla does."""
    jars = Path(tempfile.mkdtemp(prefix='horizon-java-test-'))
    (jars / TEST_JAR.name).symlink_to(TEST_JAR)
    for extra in Path(args.extra_jars).rglob('*.jar') if args.extra_jars else ():
        (jars / extra.name).symlink_to(extra.resolve())
    args.jars, args.scratch, args.dev_mod = str(jars), args.scratch, [str(SOURCES / 'testmod' / 'content')]
    wrong, positions = [], []

    def drive(send):
        x, y, z = json.loads(send('state'))['player']['block_pos']
        base = (x + 3, y, z)
        send(f'command fill {x + 1} {y - 1} {z - 2} {x + 5} {y + 12} {z + 2} air')
        # Each family's checks build in their own stretch, on the same flat ground in any world.
        for at in range(x + 6, x + 52, 15):
            send(f'command fill {at} {y} {z - 6} {at + 14} {y + 14} {z + 4} air')
            send(f'command fill {at} {y - 1} {z - 6} {at + 14} {y - 1} {z + 4} minecraft:stone')
        send('wait 2')
        send('command kill @e[type=item]')
        send(f'command setblock {base[0]} {base[1]} {base[2]} minecraft:stone')
        send(f'command setblock {base[0]} {base[1]} {base[2] + 2} minecraft:stone')
        machine = base[0] + 32
        send(f'command fill {machine - 3} {y} {z - 2} {machine + 2} {y + 8} {z + 2} air')
        send(f'command setblock {machine - 2} {y - 1} {z} minecraft:stone')
        send('give horizon_java_test:wand')
        send('wait 20')
        send('select horizon_java_test:wand')
        send(f'look-at-block {base[0]} {base[1]} {base[2]} up')
        send('wait 5')
        # Leaves cut off by the clearing decay meanwhile, and their saplings would land in the checked area.
        send('command kill @e[type=item]')
        print(send('use-item')[:200], flush=True)
        send('wait 40')
        expected = {(0, 1, 0): 'gold_block', (0, 2, 0): 'redstone_block', (0, 3, 0): 'emerald_block', (0, 4, 0): 'diamond_block', (0, 5, 0): 'lapis_block',
                    (0, 6, 0): 'iron_block', (0, 7, 0): 'quartz_block', (0, 8, 0): 'sand', (0, 9, 0): 'air', (1, 1, 0): 'obsidian', (-1, 1, 0): 'glowstone'}
        for offset, block in expected.items():
            at = f'{base[0] + offset[0]} {base[1] + offset[1]} {base[2] + offset[2]}'
            state = send(f'block {at}')
            ok = f'minecraft:{block}' in state
            print(f"  {offset}: want {block}: {'ok' if ok else 'WRONG ' + state[:160]}", flush=True)
            wrong.extend([] if ok else [block])
        if 'wand ok' not in send('messages'):
            print('  no chat line', flush=True)
            wrong.append('chat')
        # A real click on a mod's block, then the machine after it has run.
        send(f'command tp @s {machine - 2} {y} {z}')
        send('wait 40')
        send(f'look-at-block {machine} {y} {z} west')
        send('wait 5')
        if os.environ.get('HORIZON_JAVA_DEBUG'):
            print('looking', json.loads(send('state'))['looking_at'], send(f'block {machine} {y} {z}')[:120], flush=True)
            print('chest', send(f'command data get block {machine} {y + 2} {z}')[:300], flush=True)
            print('battery', send(f'command data get block {machine} {y} {z}')[:300], flush=True)
        print(send('use-item')[:200], flush=True)
        send('wait 10')
        if 'minecraft:gold_block' not in send(f'block {machine} {y + 1} {z}'):
            print('  the machine did not take the click', flush=True)
            wrong.append('block_click')
        send('command setblock ' + f'{machine - 1} {y + 1} {z} minecraft:stone')
        send(f'command tp @s {x} {y} {z}')
        send('give horizon_java_test:probe')
        send('wait 40')
        send('select horizon_java_test:probe')
        send(f'look-at-block {base[0]} {base[1]} {base[2] + 2} up')
        send('wait 5')
        send('use-item')
        send('wait 5')
        positions.append((x, y, z, base))

    def again(send):
        x, y, z, base = positions[0]
        send(f'command tp @s {x} {y} {z}')
        send('wait 20')
        send('select horizon_java_test:probe')
        send(f'look-at-block {base[0]} {base[1]} {base[2] + 2} up')
        send('wait 5')
        send('use-item')
        send('wait 5')

    if not args.scratch:
        args.scratch = tempfile.mkdtemp(prefix='horizon-java-mods-')
    session(args, drive)
    checks = re.findall(r'CHECK (.+?) (ok|FAILED|THREW.*)$', (Path(args.scratch) / 'ctl.log').read_text(errors='replace'), re.M)
    shutil.copy(Path(args.scratch) / 'ctl.log', Path(args.scratch) / 'first-session.log')
    session(args, again)
    second = re.findall(r'CHECK (.+?) (ok|FAILED|THREW.*)$', (Path(args.scratch) / 'ctl.log').read_text(errors='replace'), re.M)
    checks += [(name + ' (second session)', result) for name, result in second]
    for name, result in checks:
        if result != 'ok':
            print(f'  {name}: {result}', flush=True)
            wrong.append(name)
    print(f'{len(checks)} checks')
    if not checks:
        wrong.append('no checks ran')
    print('RESULT', 'all bridges answered' if not wrong else f'wrong at {wrong}')
    sys.exit(1 if wrong else 0)


def class_methods(data):
    """A class file's name, supers and (name, descriptor) methods."""
    count = struct.unpack('>H', data[8:10])[0]
    pool, at, k = [None] * count, 10, 1
    while k < count:
        tag = data[at]
        if tag == 1:
            size = struct.unpack('>H', data[at + 1:at + 3])[0]
            pool[k] = data[at + 3:at + 3 + size].decode('utf8', 'replace')
            at += 3 + size
        elif tag in (3, 4, 9, 10, 11, 12, 17, 18):
            at += 5
        elif tag in (5, 6):
            at, k = at + 9, k + 1
        elif tag in (7, 8, 16, 19, 20):
            pool[k] = struct.unpack('>H', data[at + 1:at + 3])[0]
            at += 3
        else:
            at += 4
        k += 1

    def u2():
        nonlocal at
        at += 2
        return struct.unpack('>H', data[at - 2:at])[0]

    u2()
    name = pool[pool[u2()]]
    supers = [pool[pool[s]] for s in [u2()] if s]
    supers += [pool[pool[u2()]] for _ in range(u2())]
    tables = []
    for _ in range(2):
        members = []
        for _ in range(u2()):
            u2()
            member = (pool[u2()], pool[u2()])
            for _ in range(u2()):
                u2()
                at += 4 + struct.unpack('>I', data[at:at + 4])[0]
            members.append(member)
        tables.append(members)
    return name, supers, tables[1]


def java_args(desc):
    args, i = [], 1
    while desc[i] != ')':
        start = i
        while desc[i] == '[':
            i += 1
        i = desc.index(';', i) + 1 if desc[i] == 'L' else i + 1
        args.append(desc[start:i])
    return args, desc[i + 1:]


def check_bridges(_args=None):
    """Every bridge against the real jars, without starting anything: its method exists, its arguments line up, Flat can carry them."""
    home = oracle.install()
    classes = {}
    for jar in sorted((home / 'libraries' / 'net' / 'neoforged').rglob('*.jar')):
        if 'minecraft-server-patched' in jar.name or 'universal' in jar.name:
            with zipfile.ZipFile(jar) as z:
                for name in z.namelist():
                    if name.endswith('.class'):
                        info = class_methods(z.read(name))
                        classes[info[0]] = info
    for path in (LOCAL / 'classes' / 'boot').rglob('*.class'):
        info = class_methods(path.read_bytes())
        classes[info[0]] = info
    flat = set(classes['dev/horizonlab/javamods/boot/Flat'][2])
    raws = {'i32': 'I', 'i64': 'J', 'f32': 'F', 'f64': 'D', 'bool': 'Z', '()': 'V'}
    bridge_rs = (oracle.ROOT / 'crates' / 'horizon-java-host' / 'src' / 'bridge.rs').read_text()
    for kind, ty, raw in re.findall(r'impl (Arg|Ret) for (\w+) \{.*?const RAW: &\'static str = "([^"]+)"', bridge_rs, re.S):
        raws[ty] = raw
    entry = re.compile(r'"([\w/$]+)"\.(\S+)\s+"(\([^"]*)"(?:\s+if\s+"([\w/$]+)")?\s*=>\s*fn\s+(\w+)\(\s*\w+\s*((?:,\s*\w+:\s*\w+\s*)*),?\s*\)\s*->\s*([\w()]+)')

    def extends(name, target):
        seen, queue = set(), [name]
        while queue:
            current = queue.pop()
            if current == target:
                return True
            if current in classes and current not in seen:
                seen.add(current)
                queue += classes[current][1]
        return False

    errors, total, natives = [], 0, set()
    for path in sorted((oracle.ROOT / 'crates' / 'horizon-client' / 'src' / 'game' / 'java_bridges').glob('*.rs')):
        for owner, method, desc, guard, native, params, result in entry.findall(path.read_text()):
            total += 1
            where = f'{path.name}: {native}'
            if native in natives:
                errors.append(f'{where}: the name is taken')
            natives.add(native)
            if owner not in classes or (method, desc) not in classes[owner][2]:
                errors.append(f'{where}: no {owner}.{method}{desc}')
                continue
            if guard and not extends(guard, owner):
                errors.append(f'{where}: {guard} is not a {owner}')
            args, ret = java_args(desc)
            types = [t.split(':')[1].strip() for t in params.split(',') if t.strip()]
            mine = [raws.get(t) for t in types]
            if None in mine:
                errors.append(f'{where}: no Arg for {types}')
                continue
            this = len(mine) == len(args) + 1
            if len(mine) != len(args) and not this:
                errors.append(f'{where}: {len(mine)} arguments for {desc}')
                continue
            lined = ([f'L{owner};'] if this else []) + args
            for java, raw in zip(lined, mine):
                if raw == '-':
                    continue
                if java[0] in 'L[':
                    if java == raw:
                        continue
                    if java[0] == 'L' and ('raw', f'({java}){raw}') not in flat:
                        errors.append(f'{where}: Flat has no raw({java}) -> {raw}')
                    elif java[0] == '[' and java != raw:
                        errors.append(f'{where}: {java} against {raw}')
                elif java != raw:
                    errors.append(f'{where}: Java {java} against {raw}')
            mine_ret = raws.get(result)
            if mine_ret is None:
                errors.append(f'{where}: no Ret for {result}')
            elif ret[0] == 'L':
                simple = ret[1:-1].replace('$', '/').split('/')[-1]
                if (f'to{simple}', f'({mine_ret}){ret}') not in flat:
                    errors.append(f'{where}: Flat has no to{simple}({mine_ret}) -> {ret}')
            elif ret != mine_ret:
                errors.append(f'{where}: returns {ret} against {mine_ret}')
    for error in errors:
        print(error)
    print(f'{total} bridges, {len(errors)} problems')
    sys.exit(1 if errors else 0)


def modrinth(path):
    request = urllib.request.Request('https://api.modrinth.com/v2/' + path, headers={'User-Agent': 'horizon-rust/horizon tools/java_mods.py'})
    with urllib.request.urlopen(request, timeout=60) as reply:
        return json.load(reply)


def fetch_mod(project, seen):
    """A project's newest 26.3 NeoForge jar and its required dependencies, into LIBRARIES; returns the jars' paths, or None when it has no such version."""
    versions = modrinth(f'project/{project}/version?game_versions=%5B%2226.3%22%5D&loaders=%5B%22neoforge%22%5D')
    if not versions:
        return None
    version = versions[0]
    project = version['project_id']
    if project in seen:
        return seen[project]
    chosen = next((f for f in version['files'] if f['primary']), version['files'][0])
    folder = LIBRARIES / project
    folder.mkdir(parents=True, exist_ok=True)
    jar = folder / chosen['filename']
    if not jar.exists():
        urllib.request.urlretrieve(chosen['url'], jar)
    seen[project] = [jar]
    for dep in version['dependencies']:
        if dep['dependency_type'] == 'required' and dep.get('project_id'):
            jars = fetch_mod(dep['project_id'], seen)
            if jars:
                seen[project] = seen[project] + [j for j in jars if j not in seen[project]]
    return seen[project]


def boot_one(name, jars, opts, timeout):
    """Real mod loading, real data packs and the real views, with only these jars; (ok, why, seconds)."""
    folder = LOCAL / 'library-sets' / name
    shutil.rmtree(folder, ignore_errors=True)
    folder.mkdir(parents=True)
    for jar in jars:
        (folder / Path(jar).name).symlink_to(Path(jar).resolve())
    run = instance(f'library-{name}', folder, True)
    java = java_home()
    env = {**os.environ, 'JAVA_HOME': java, 'LD_LIBRARY_PATH': f'{java}/lib/server:{java}/lib', 'HORIZON_NEOFORGE': NEOFORGE,
           'HORIZON_JAVA_RUN': str(run), 'HORIZON_JAVA_HOST_JAR': str(HOST_JAR), 'HORIZON_JAVA_OPTS': opts}
    began = time.perf_counter()
    try:
        p = subprocess.run([str(TARGET / 'fast' / 'horizon-java-host'), 'world', 'exit'], env=env, cwd=run, capture_output=True, text=True, errors='replace', timeout=timeout)
    except subprocess.TimeoutExpired as e:
        return False, 'hung: ' + ((e.stdout or b'').decode(errors='replace').strip().splitlines() or [''])[-1][:200], timeout
    took = time.perf_counter() - began
    (run / 'host.log').write_text(p.stdout + p.stderr)
    if 'WORLD OK' in p.stdout:
        return True, '', took
    text = p.stdout + p.stderr
    for pattern in (r'Failed to start FML: .*', r'Caused by: .*', r'ERROR .*', r'.*Exception.*'):
        found = re.findall(pattern, text)
        if found and not pattern.startswith('- '):
            detail = re.findall(r'^\s*- (.+)$', text, re.M)
            return False, (found[-1] + (' | ' + ' | '.join(detail[:3]) if detail else ''))[:400], took
    return False, 'no answer, exit ' + str(p.returncode), took


def libraries(args):
    """Boots the most used 26.3 NeoForge library mods one by one, each with its required dependencies; `--top` picks how many."""
    facets = urllib.parse.quote(json.dumps([['categories:library'], ['categories:neoforge'], ['versions:26.3'], ['project_type:mod']]))
    hits = []
    for offset in range(0, args.top, 100):
        hits += modrinth(f'search?limit={min(100, args.top - offset)}&offset={offset}&index=downloads&facets={facets}')['hits']
    seen, sets = {}, {}
    for hit in hits:
        try:
            jars = fetch_mod(hit['project_id'], seen)
        except OSError as e:
            print(f"{hit['slug']}: download failed: {e}", flush=True)
            continue
        if jars:
            sets[hit['slug']] = jars
    print(f'{len(sets)} of {len(hits)} libraries have a 26.3 NeoForge jar', flush=True)
    results = {}

    def one(item):
        return item[0], boot_one(item[0], item[1], args.opts, args.timeout), len(item[1])

    with concurrent.futures.ThreadPoolExecutor(args.jobs) as pool:
        for slug, (ok, why, took), jars in pool.map(one, sets.items()):
            results[slug] = {'ok': ok, 'why': why, 'seconds': round(took, 1), 'jars': jars}
            print(f"{'ok  ' if ok else 'FAIL'} {slug} ({jars} jars, {took:.1f} s) {why}", flush=True)
    good = sorted({j for slug, jars in sets.items() if results[slug]['ok'] for j in jars})
    together = boot_one('all-together', good, args.opts, args.timeout * 3)
    results['all-together'] = {'ok': together[0], 'why': together[1], 'seconds': round(together[2], 1), 'jars': len(good)}
    print(f"{'ok  ' if together[0] else 'FAIL'} all {len(good)} that booted, together ({together[2]:.1f} s) {together[1]}", flush=True)
    Path(args.out).write_text(json.dumps(results, indent=1))
    passed = sum(r['ok'] for r in results.values())
    print(f'RESULT {passed} of {len(results)} booted', flush=True)


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

    tables = (oracle.ROOT / 'crates' / 'horizon-client' / 'src' / 'game' / 'java_bridges').glob('*.rs')
    bound = {f'{owner}#{name}' for path in tables for owner, name in re.findall(r'"([\w/$]+)"\.(\w+)\s+"\(', path.read_text())}
    skipped = {}
    for path in (oracle.ROOT / 'tools' / 'java_mods_skips').glob('*.tsv'):
        for line in path.read_text().splitlines():
            if line.strip():
                key, reason, *_ = line.split('\t')
                skipped[key] = reason
    print(f'{len(users)} vanilla methods called by {args.jars}')
    print(collections.Counter(status(k) for k in users).most_common())
    ported = [k for k in users if status(k) in ('ported', 'mapped', 'named', 'structural', 'linked')]
    print('their Rust ports live in', collections.Counter(next((h for h in ('library', 'client', 'game') if h in homes(k)), 'none') for k in ported).most_common())
    ported = [k for k in ported if not methods[k].get('accessor') and methods[k].get('size', 99) > 4]
    client = [k for k in ported if methods[k].get('client') or methods[k]['class'].startswith(('net/minecraft/client/', 'com/mojang/blaze3d'))]
    print(f'worth serving from Rust: {len(ported)} ({len(client)} client only); bridged: {len(bound)}, of which these jars call {len(bound & users.keys())}')
    print('decided against, by reason:', collections.Counter(skipped[k] for k in ported if k in skipped and k not in bound).most_common())
    open_ = [k for k in ported if k not in bound and k not in skipped]
    print(f'still open: {len(open_)}')
    open_.sort(key=lambda k: (-len(users[k]), -methods[k].get('size', 0)))
    for key in open_[:args.top]:
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
    sub.add_parser('build').add_argument('--java-only', action='store_true')
    sub.add_parser('check')
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
    g.add_argument('--seed', type=int, default=1)
    g.add_argument('--scratch', help='reuse this folder: its world and settings')
    g.add_argument('--opts', default='--enable-native-access=ALL-UNNAMED')
    g.add_argument('--dev-mod', action='append', default=[])
    g.add_argument('commands', nargs='*')
    t = sub.add_parser('bridges')
    t.add_argument('--extra-jars', help='also load these, to see them live with the bridges')
    t.add_argument('--seed', type=int, default=1)
    t.add_argument('--opts', default='--enable-native-access=ALL-UNNAMED')
    t.add_argument('--scratch')
    l = sub.add_parser('libraries')
    l.add_argument('--top', type=int, default=60)
    l.add_argument('--jobs', type=int, default=4)
    l.add_argument('--timeout', type=int, default=180)
    l.add_argument('--opts', default='--enable-native-access=ALL-UNNAMED -Xmx2g')
    l.add_argument('--out', default=str(LOCAL / 'libraries.json'))
    f = sub.add_parser('fetch', help='a Modrinth project and its required dependencies, linked into a folder')
    f.add_argument('project')
    f.add_argument('--out', required=True)
    c = sub.add_parser('surface')
    c.add_argument('--jars', required=True)
    c.add_argument('--top', type=int, default=40)
    args = parser.parse_args()
    LOCAL.mkdir(parents=True, exist_ok=True)
    # Several agents build and check at once, and a build replaces the class folders.
    lock = (LOCAL / 'build.lock').open('w')
    if args.command in ('build', 'check'):
        fcntl.flock(lock, fcntl.LOCK_EX)
    if args.command == 'build':
        build(args.java_only)
    elif args.command == 'run':
        p = host(instance('run', args.jars, True), args.opts, args.ops, args.facts)
        print(p.stdout)
        print(p.stderr[-3000:], file=sys.stderr)
        sys.exit(p.returncode)
    elif args.command == 'play':
        play(args)
    elif args.command == 'surface':
        surface(args)
    elif args.command == 'bridges':
        bridges(args)
    elif args.command == 'libraries':
        libraries(args)
    elif args.command == 'fetch':
        folder = Path(args.out)
        folder.mkdir(parents=True, exist_ok=True)
        for jar in fetch_mod(args.project, {}) or []:
            if not (folder / jar.name).exists():
                (folder / jar.name).symlink_to(jar.resolve())
            print(jar.name)
    elif args.command == 'check':
        check_bridges()
    else:
        bench(args)


if __name__ == '__main__':
    main()
