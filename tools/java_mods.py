#!/usr/bin/env python3
"""Dev driver for horizon-java-host: real mod jars on real NeoForge classes in a JVM embedded in Rust (needs nix-shell -p jdk25 pkg-config)."""
import argparse
import collections
import concurrent.futures
import fcntl
import fnmatch
import hashlib
import io
import json
import os
import re
import shlex
import shutil
import struct
import platform
import subprocess
import sys
import tarfile
import tempfile
import threading
import time
import urllib.parse
import urllib.request
import zipfile
from pathlib import Path

os.environ.setdefault('HORIZON_NEOFORGE', '26.3.0.101')
try:
    from . import lab, neoforge_oracle as oracle
except ImportError:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import lab  # noqa: E402
    import neoforge_oracle as oracle  # noqa: E402

NEOFORGE = oracle.NEOFORGE
SOURCES = oracle.ROOT / 'mods' / 'horizon' / 'src' / 'java-mods-host'
LOCAL = oracle.ROOT / '.local' / 'java-mods'
TARGET = LOCAL / 'target'
# A private folder for an unproven build: its jars, classes and runs, instead of the shared ones every game uses.
JARS = Path(os.environ.get('HORIZON_JAVA_MODS_DIR') or LOCAL)
HOST_JAR = JARS / 'horizon-java-host.jar'
BOOT_JAR = JARS / 'horizon-java-boot.jar'
PLUGIN_JAR = JARS / 'horizon-java-plugin.jar'
TEST_JAR = JARS / 'horizon-java-test.jar'
LIBRARIES = LOCAL / 'library-jars'
# A game's JVM boots the client side, as a NeoForge client does; a dedicated Horizon's would boot the server side.
DIST = os.environ.get('HORIZON_JAVA_DIST', 'client')


def java_home():
    if os.environ.get('JAVA_HOME'):
        return os.environ['JAVA_HOME']
    settings = subprocess.run(['java', '-XshowSettings:properties', '-version'], capture_output=True, text=True).stderr
    return re.search(r'java\.home = (.+)', settings).group(1).strip()


def mixinextras(classpath):
    """The MixinExtras jar NeoForge's universal jar nests, taken out once so the test mod compiles against it."""
    out = JARS / 'mixinextras.jar'
    if not out.is_file():
        universal = next(jar for jar in classpath.split(':') if jar.endswith('-universal.jar'))
        with zipfile.ZipFile(universal) as z:
            nested = next(n for n in z.namelist() if n.startswith('META-INF/jarjar/mixinextras'))
            out.write_bytes(z.read(nested))
    return str(out)


def build(java_only=False):
    JARS.mkdir(parents=True, exist_ok=True)
    # Against the client, which holds the server's classes too.
    classpath = oracle.client_classpath(oracle.client()[0])
    cp = ':'.join(jar for jar in classpath.split(':') if 'installertools' not in jar) + ':' + mixinextras(classpath)
    host_classes = str(JARS / 'classes' / 'host')
    for name, jar, extra in (('host', HOST_JAR, ''), ('plugin', PLUGIN_JAR, host_classes), ('boot', BOOT_JAR, host_classes), ('testmod', TEST_JAR, '')):
        classes = JARS / 'classes' / name
        shutil.rmtree(classes, ignore_errors=True)
        classes.mkdir(parents=True)
        sources = [str(p) for p in sorted((SOURCES / name).rglob('*.java'))]
        if name == 'boot':
            sources += [str(p) for p in sorted(oracle.SHARED.rglob('*.java'))]
            sources += [str(p) for p in sorted(oracle.SHARED_CLIENT.rglob('*.java'))]
            sources += [str(SOURCES / 'parity' / 'scan' / 'dev' / 'horizonlab' / 'javamods' / 'parity' / 'MixinScanner.java')]
        subprocess.run(['javac', '--release', '25', '-proc:none', '-nowarn', '-cp', f'{cp}:{extra}', '-d', str(classes), *sources], check=True)
        jar_args = ['jar', 'cf', str(jar), '-C', str(classes), '.']
        if name == 'plugin':
            manifest = JARS / 'plugin.manifest'
            manifest.write_text('FMLModType: LIBRARY\n')
            jar_args = ['jar', 'cfm', str(jar), str(manifest), '-C', str(classes), '.']
        if (SOURCES / name / 'res').exists():
            jar_args += ['-C', str(SOURCES / name / 'res'), '.']
        subprocess.run(jar_args, check=True)
    if java_only:
        return
    env = {**os.environ, 'JAVA_HOME': java_home(), 'CARGO_TARGET_DIR': str(TARGET), 'RUSTC_WRAPPER': ''}
    subprocess.run(['cargo', 'build', '--profile', 'fast', '-q', '-p', 'horizon-java-host'], check=True, env=env, cwd=oracle.ROOT)


def unit():
    """The host's Java unit checks (`unit/**/*Check.java`): plain JVMs over the built classes, no boot."""
    cp = ':'.join(jar for jar in oracle.client_classpath(oracle.client()[0]).split(':') if 'installertools' not in jar)
    built = f"{JARS / 'classes' / 'boot'}:{JARS / 'classes' / 'host'}:{JARS / 'classes' / 'plugin'}"
    classes = JARS / 'classes' / 'unit'
    shutil.rmtree(classes, ignore_errors=True)
    classes.mkdir(parents=True)
    sources = sorted((SOURCES / 'unit').rglob('*.java'))
    subprocess.run(['javac', '--release', '25', '-proc:none', '-nowarn', '-cp', f'{cp}:{built}', '-d', str(classes), *map(str, sources)], check=True)
    failed = 0
    for source in sources:
        if source.stem.endswith('Check'):
            name = '.'.join(source.relative_to(SOURCES / 'unit').with_suffix('').parts)
            failed += subprocess.run(['java', '-cp', f'{cp}:{built}:{classes}', name]).returncode != 0
    sys.exit(1 if failed else 0)


def assets_index(resources, index_id, root):
    """Vanilla's assets folder (`indexes/<id>.json`, `objects/<2 hex>/<sha1>`) as symlinks over the prepared assets, built once per content."""
    source = json.loads((resources / 'asset-index.json').read_text())['objects']
    assets = (resources / 'assets').resolve()
    objects = {key: entry for key, entry in sorted(source.items()) if (assets / key).is_file()}
    index = json.dumps({'objects': objects}, separators=(',', ':'))
    key = hashlib.sha1(f'{assets}\0{index_id}\0{index}'.encode()).hexdigest()[:16]
    folder = root / 'assets-index' / key
    if not (folder / 'indexes' / f'{index_id}.json').is_file():
        building = folder.with_name(f'{key}.{os.getpid()}')
        shutil.rmtree(building, ignore_errors=True)
        (building / 'indexes').mkdir(parents=True)
        for name, entry in objects.items():
            link = building / 'objects' / entry['hash'][:2] / entry['hash']
            link.parent.mkdir(parents=True, exist_ok=True)
            if not link.is_symlink():
                link.symlink_to(assets / name)
        (building / 'indexes' / f'{index_id}.json').write_text(index)
        try:
            building.rename(folder)
        except OSError:
            shutil.rmtree(building, ignore_errors=True)
    return folder


def args_lines(dist, libs, neoforge, client=None, server=None, assets=None, analysis=None):
    """The lines of `horizon-java.args`: `client` is (NeoForge's client profile, its library jars), `server` the text of its `unix_args.txt`, `assets` (folder, index id), `analysis` the install's entity-facts.json."""
    lines = []
    if dist == 'client':
        neo, jars = client
        lines.append('dist client')
        lines.append('cp ' + ':'.join(map(str, jars)))
        jvm = [a.replace('${library_directory}', str(libs)) for a in neo['arguments']['jvm']]
        game = neo['arguments']['game']
    else:
        args = server.splitlines()
        home = libs.parent
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
    lines.append(f'mod {libs}/net/neoforged/neoforge/{neoforge}/neoforge-{neoforge}-universal.jar')
    if analysis:
        lines.append(f'analysis {analysis}')
    if dist == 'client':
        folder, index_id = assets
        lines += ['prog --assetsDir', f'prog {folder}', 'prog --assetIndex', f'prog {index_id}']
    return lines


def mods_analysis(name, jars):
    """The analysis of the mods in `jars` alone (not the host's own jars), so the boot confirms their routes."""
    try:
        from . import mod_replaces
    except ImportError:
        import mod_replaces
    folder = JARS / 'runs' / f'{name}-analysis'
    shutil.rmtree(folder, ignore_errors=True)
    (folder / 'mods').mkdir(parents=True)
    (folder / 'libraries').symlink_to((oracle.client()[0] if DIST == 'client' else oracle.install()) / 'libraries')
    for jar in sorted(Path(jars).glob('*.jar')):
        (folder / 'mods' / jar.name).symlink_to(jar.resolve())
    out = folder / 'analysis'
    out.mkdir()
    ledger = json.loads(mod_replaces.LEDGER_JSON.read_text()) if mod_replaces.LEDGER_JSON.is_file() else {}
    result = mod_replaces.analyse(folder, out, ledger, JARS / 'mod-replaces', verdicts=mod_replaces.read_verdicts())
    mod_replaces.write_routing(out, result['routing'])
    (out / 'entity-facts.json').write_text(json.dumps(result['entity_facts'], indent=1) + '\n')
    return out / 'entity-facts.json'


def write_args(run, home, analysis=None):
    """What the host starts the JVM with, in absolute paths, as NeoForge's launcher arguments would be."""
    libs = home / 'libraries'
    if DIST == 'client':
        _, neo, _, jars = oracle.client()
        index_id = json.loads((lab.GAME / 'manifest.json').read_text())['assetIndex']['id']
        lines = args_lines('client', libs, NEOFORGE, client=(neo, jars), assets=(assets_index(lab.RESOURCES, index_id, JARS), index_id), analysis=analysis)
    else:
        lines = args_lines('server', libs, NEOFORGE, server=(libs / 'net/neoforged/neoforge' / NEOFORGE / 'unix_args.txt').read_text(), analysis=analysis)
    (run / 'horizon-java.args').write_text('\n'.join(lines) + '\n')


def instance(name, jars, boot):
    home = oracle.client()[0] if DIST == 'client' and boot else oracle.install()
    run = JARS / 'runs' / name
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
        write_args(run, home, mods_analysis(name, jars))
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
        # A command's own quotes (SNBT strings) must reach the game.
        words = ['command', line.split(' ', 1)[1]] if line.startswith('command ') else shlex.split(line)
        reply = subprocess.run([*ctl, *words], env=env, capture_output=True, text=True, cwd=scratch)
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
        # ctl counts slots from 1: the server's setSelectedSlot sees 1, 7 (the test mod cancels it), then 2.
        for slot in (2, 8, 3):
            send(f'select {slot}')
            send('wait 3')
        # A body route: with slot 9 selected the test mod's getFreeSlot reports slot 0 full, so the stack lands in slot 2.
        send('command clear @s')
        send('select 9')
        send('wait 3')
        send('give minecraft:stone 1')
        send('wait 3')
        held = json.loads(send('inventory'))
        held = held.get('inventory', held)['hotbar']
        if 'stone' in json.dumps(held[0]) or 'stone' not in json.dumps(held[1]):
            wrong.append('route_body_free_slot')
        send('select 3')
        send('wait 3')
        send('command kill @e[type=item]')
        # Outside the checked stretches: an entity the player's client tracks.
        send(f'command summon minecraft:armor_stand {base[0] + 40} {y} {z - 12} {{NoGravity:1b}}')
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
        # A native change to the chest the Java side holds, between the two clicks.
        send(f'command item replace block {machine} {y + 4} {z} container.0 with minecraft:cobblestone 20')
        send(f'command setblock {machine} {y + 8} {z} minecraft:chest')
        send(f'command setblock {machine} {y + 10} {z} minecraft:air')
        send(f'command tp @s {x} {y} {z}')
        send('give horizon_java_test:probe')
        send('wait 40')
        send('select horizon_java_test:probe')
        send(f'look-at-block {base[0]} {base[1]} {base[2] + 2} up')
        send('wait 5')
        send('use-item')
        send('wait 5')
        chest = send(f'command data get block {machine} {y + 4} {z}')
        print('chest', chest[:200], flush=True)
        if not re.search(r'count\D{0,3}18\b', chest):
            wrong.append('chest_written_through')
        # A mod's key under chat (the window's own screen) stays quiet; with chat closed it fires.
        send('key t tap')
        send('wait 5')
        send('key k down')
        send('wait 3')
        send('key k up')
        send('key escape tap')
        send('wait 5')
        send('key k down')
        send('wait 3')
        send('key k up')
        send('wait 3')
        # A key held while chat closes is down after it, as KeyMapping.setAll leaves it.
        send('key t tap')
        send('wait 5')
        send('key k down')
        send('wait 3')
        send('key escape tap')
        send('wait 3')
        send('key k up')
        send('wait 3')
        # The window's rebind of a mod's key reaches its own KeyMapping: the mapping follows the new key.
        send('options-set key_key.horizon_java_test.probe key.keyboard.j')
        send('wait 3')
        send('key j down')
        send('wait 3')
        send('key j up')
        send('wait 3')
        # A real survival break with a tool whose speed and harvest are its mod's code: stone drops, a snow layer goes at once.
        send('command gamemode survival')
        send('give horizon_java_test:digger')
        send('wait 10')
        send('select horizon_java_test:digger')
        send('command kill @e[type=item]')
        send(f'command setblock {x + 2} {y} {z - 1} minecraft:stone')
        send('wait 5')
        print(send(f'break {x + 2} {y} {z - 1} --timeout 20')[:160], flush=True)
        send('wait 5')
        for name, ok in (('mined_stone_goes', 'minecraft:air' in send(f'block {x + 2} {y} {z - 1}')),
                         ('mined_stone_drops_cobblestone', 'passed' in send('command execute if entity @e[type=item,nbt={Item:{id:"minecraft:cobblestone"}}]'))):
            print(f"  {name}: {'ok' if ok else 'WRONG'}", flush=True)
            wrong.extend([] if ok else [name])
        send(f'command setblock {x + 2} {y} {z + 1} minecraft:stone')
        send(f'command setblock {x + 2} {y + 1} {z + 1} minecraft:snow[layers=1]')
        send('wait 5')
        print(send(f'break {x + 2} {y + 1} {z + 1} --timeout 20')[:160], flush=True)
        send('wait 5')
        ok = 'minecraft:air' in send(f'block {x + 2} {y + 1} {z + 1}')
        print(f"  snow_layer_instant: {'ok' if ok else 'WRONG'}", flush=True)
        wrong.extend([] if ok else ['snow_layer_instant'])
        # A mod block whose drops come from code, then one that refuses its removal.
        send(f'command setblock {x + 2} {y} {z - 3} horizon_java_test:dropper')
        send('wait 5')
        broke = send(f'break {x + 2} {y} {z - 3} --timeout 20')
        print(broke[:200], flush=True)
        ok = '"minecraft:diamond":1' in broke.replace(' ', '') and 'air' in send(f'block {x + 2} {y} {z - 3}')
        print(f"  mod_block_getdrops: {'ok' if ok else 'WRONG'}", flush=True)
        wrong.extend([] if ok else ['mod_block_getdrops'])
        send(f'command setblock {x + 2} {y} {z + 3} horizon_java_test:anchor')
        send('wait 5')
        print(send(f'break {x + 2} {y} {z + 3} --timeout 4')[:200], flush=True)
        send('wait 10')
        ok = 'horizon_java_test:anchor' in send(f'block {x + 2} {y} {z + 3}')
        print(f"  mod_block_refuses_removal: {'ok' if ok else 'WRONG'}", flush=True)
        wrong.extend([] if ok else ['mod_block_refuses_removal'])
        send('command gamemode creative')
        positions.append((x, y, z, base))

    def again(send):
        x, y, z, base = positions[0]
        # Far enough for the machine's chunk to unload, then back for it to load again.
        send('command gamemode spectator')
        send(f'command tp @s {x + 4000} {y + 60} {z}')
        send('wait 60')
        send(f'command tp @s {x} {y} {z}')
        send('command gamemode survival')
        send('wait 60')
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
    wrong.extend(name for name in ('key_quiet_under_a_native_screen', 'key_fires_without_a_screen', 'key_held_through_a_native_screen_is_down', 'mod_key_follows_the_rebind', 'client_profile_resolver', 'route_tail_sees_body', 'route_head_cancel', 'route_return_equals_tail', 'route_body_free_slot_java') if (name, 'ok') not in checks)
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


BRIDGES_DIR = oracle.ROOT / 'crates' / 'horizon-client' / 'src' / 'game' / 'java'
BRIDGE_ENTRY = re.compile(r'"([\w/$]+)"\.(\S+)\s+"(\([^"]*)"(?:\s+if\s+"(@?[\w/$.]+)")?\s*=>\s*fn\s+(\w+)\(\s*\w+\s*((?:,\s*\w+:\s*\w+\s*)*),?\s*\)\s*->\s*([\w()]+)')


def unbridged_writers(analysis, extends=None):
    """The entity writers of an install's entity-facts.json with no bridged native and no `java-owned` line in tools/java_mods_skips: [(key, mods)].

    A bridge on a subclass covers its parent's method (an abstract `hurtServer` has no body to bridge)."""
    facts = json.loads(Path(analysis).read_text())
    tables = (oracle.ROOT / 'crates' / 'horizon-client' / 'src' / 'game' / 'java').glob('*.rs')
    bridged = [(owner, name) for path in tables for owner, name in re.findall(r'"([\w/$]+)"\.(\w+)\s+"\(', path.read_text())]
    covered = {f'{owner}#{name}' for owner, name in bridged}
    for path in (oracle.ROOT / 'tools' / 'java_mods_skips').glob('*.tsv'):
        for line in path.read_text().splitlines():
            key, _, rest = line.partition('\t')
            if rest.split('\t')[0] == 'java-owned':
                covered.add(key)
    missing = collections.defaultdict(set)
    for w in facts['writes']:
        owner, _, name = w['key'].split('(')[0].partition('#')
        below = extends is not None and any(n == name and extends(o, owner) for o, n in bridged)
        if w['group'] != 'java' and w['key'].split('(')[0] not in covered and not below:
            missing[w['key']].update(w['mods'])
    return [(key, sorted(mods)) for key, mods in sorted(missing.items())]


def return_problem(ret, mine_ret, flat):
    """Why a bridge's native return type can't stand for the method's, or None: equal descriptors need no converter, an object needs `Flat.to<Name>`."""
    if ret[0] in 'L[' and ret == mine_ret:
        return None
    if ret[0] == 'L':
        simple = ret[1:-1].replace('$', '/').split('/')[-1]
        if (f'to{simple}', f'({mine_ret}){ret}') not in flat:
            return f'Flat has no to{simple}({mine_ret}) -> {ret}'
        return None
    return f'returns {ret} against {mine_ret}' if ret != mine_ret else None


def add_missing_classes(classes, infos):
    """Adds the classes `classes` lacks; the ones it has stay as they are."""
    for info in infos:
        classes.setdefault(info[0], info)


def check_bridges(args=None):
    """Every bridge against the real jars, without starting anything: its method exists, its arguments line up, Flat can carry them.

    With --analysis (an install's entity-facts.json) every entity writer mod code calls must also be bridged or `java-owned`."""
    analysis = getattr(args, 'analysis', None)
    home = oracle.install()
    classes = {}
    for jar in sorted((home / 'libraries' / 'net' / 'neoforged').rglob('*.jar')):
        if 'minecraft-server-patched' in jar.name or 'universal' in jar.name:
            with zipfile.ZipFile(jar) as z:
                for name in z.namelist():
                    if name.endswith('.class'):
                        info = class_methods(z.read(name))
                        classes[info[0]] = info
    # IntegratedServer and the other client classes: only what the server jars lack.
    for jar in sorted((oracle.client()[0] / 'libraries' / 'net' / 'neoforged' / 'minecraft-client-patched').rglob('*.jar')):
        with zipfile.ZipFile(jar) as z:
            add_missing_classes(classes, (class_methods(z.read(name)) for name in z.namelist() if name.endswith('.class')))
    for path in (JARS / 'classes' / 'boot').rglob('*.class'):
        info = class_methods(path.read_bytes())
        classes[info[0]] = info
    flat = set(classes['dev/horizonlab/javamods/boot/Flat'][2])
    raws = {'i32': 'I', 'i64': 'J', 'f32': 'F', 'f64': 'D', 'bool': 'Z', '()': 'V'}
    bridge_rs = (oracle.ROOT / 'crates' / 'horizon-java-host' / 'src' / 'bridge.rs').read_text()
    for kind, ty, raw in re.findall(r'impl (Arg|Ret) for (\w+) \{.*?const RAW: &\'static str = "([^"]+)"', bridge_rs, re.S):
        raws[ty] = raw
    flats = dict(re.findall(r'impl Arg for (\w+) \{[^}]*?const FLAT: &\'static str = "([^"]+)"', bridge_rs))
    entry = BRIDGE_ENTRY

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
    for path in sorted((oracle.ROOT / 'crates' / 'horizon-client' / 'src' / 'game' / 'java').glob('*.rs')):
        for owner, method, desc, guard, native, params, result in entry.findall(path.read_text()):
            total += 1
            where = f'{path.name}: {native}'
            if native in natives:
                errors.append(f'{where}: the name is taken')
            natives.add(native)
            if owner not in classes or (method, desc) not in classes[owner][2]:
                errors.append(f'{where}: no {owner}.{method}{desc}')
                continue
            if guard and not guard.startswith('@') and not extends(guard, owner):
                errors.append(f'{where}: {guard} is not a {owner}')
            args, ret = java_args(desc)
            types = [t.split(':')[1].strip() for t in params.split(',') if t.strip()]
            mine = [raws.get(t) for t in types]
            named = [flats.get(t, 'raw') for t in types]
            if None in mine:
                errors.append(f'{where}: no Arg for {types}')
                continue
            this = len(mine) == len(args) + 1
            if len(mine) != len(args) and not this:
                errors.append(f'{where}: {len(mine)} arguments for {desc}')
                continue
            lined = ([f'L{"net/minecraft/world/entity/Entity" if guard.startswith("@") else owner};'] if this else []) + args
            for java, raw, name in zip(lined, mine, named):
                if raw == '-':
                    continue
                if java[0] in 'L[':
                    if java == raw:
                        continue
                    if java[0] == 'L' and (name, f'({java}){raw}') not in flat:
                        errors.append(f'{where}: Flat has no {name}({java}) -> {raw}')
                    elif java[0] == '[' and java != raw:
                        errors.append(f'{where}: {java} against {raw}')
                elif java != raw:
                    errors.append(f'{where}: Java {java} against {raw}')
            mine_ret = raws.get(result)
            if mine_ret is None:
                errors.append(f'{where}: no Ret for {result}')
            elif problem := return_problem(ret, mine_ret, flat):
                errors.append(f'{where}: {problem}')
    if analysis:
        errors += [f'no bridge for {key} ({", ".join(mods)})' for key, mods in unbridged_writers(analysis, extends)]
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

    tables = (oracle.ROOT / 'crates' / 'horizon-client' / 'src' / 'game' / 'java').glob('*.rs')
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


def crossing(args):
    """The Rust to Java crossing's ns per request (`crates/horizon-java-host/examples/crossing_bench.rs`): a dedicated-dist boot, no mods, through the game gate."""
    global DIST
    DIST = 'server'
    java = java_home()
    env = {**os.environ, 'JAVA_HOME': java, 'CARGO_TARGET_DIR': str(TARGET), 'RUSTC_WRAPPER': ''}
    subprocess.run(['cargo', 'build', '--profile', 'fast', '-q', '-p', 'horizon-java-host', '--example', 'crossing_bench'], check=True, env=env, cwd=oracle.ROOT)
    jars = Path(args.jars) if args.jars else JARS / 'crossing-mods'
    jars.mkdir(parents=True, exist_ok=True)
    run = instance('crossing', jars, True)
    gate = os.environ.get('HORIZON_GAME_GATE')
    gate_env = {**os.environ, 'GATE_OWNER': os.environ.get('GATE_OWNER', 'crossing')}
    while gate and subprocess.run(shlex.split(gate), env=gate_env).returncode != 0:
        time.sleep(30)
    env = {**gate_env, 'JAVA_HOME': java, 'LD_LIBRARY_PATH': f'{java}/lib/server:{java}/lib', 'HORIZON_NEOFORGE': NEOFORGE,
           'HORIZON_JAVA_RUN': str(run), 'HORIZON_JAVA_HOST_JAR': str(HOST_JAR)}
    command = [str(TARGET / 'fast' / 'examples' / 'crossing_bench'), '--rounds', str(args.rounds), '--n', str(args.n), '--soak', str(args.soak)]
    sys.exit(subprocess.run(['nice', *command], env=env, cwd=run).returncode)


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


# ---- a player's install: `install` builds a self-contained folder from a pack of mod jars, `launch` starts the game from it ----

JDK_FEATURE = 25
INSTALL_FORMAT = 1
FETCH_HEADERS = {'User-Agent': 'horizon-java-mods/1', 'Accept': 'application/json'}


def default_cache():
    return Path(os.environ.get('XDG_CACHE_HOME') or Path.home() / '.cache') / 'horizon' / 'java-mods'


def jdk_platform(system=None, machine=None):
    """Adoptium's (os, architecture) names for this machine."""
    system = system or sys.platform
    machine = (machine or platform.machine()).lower()
    names = {'linux': 'linux', 'darwin': 'mac'}
    arches = {'x86_64': 'x64', 'amd64': 'x64', 'aarch64': 'aarch64', 'arm64': 'aarch64'}
    if system not in names or machine not in arches:
        sys.exit(f'no JDK {JDK_FEATURE} download for {system}/{machine}: pass --java with one')
    return names[system], arches[machine]


def jdk_query(os_name, arch):
    return (f'https://api.adoptium.net/v3/assets/latest/{JDK_FEATURE}/hotspot?architecture={arch}'
            f'&image_type=jdk&os={os_name}&vendor=eclipse')


def jdk_asset(listing):
    """The archive to fetch out of Adoptium's answer: its link, name and published sha256."""
    package = listing[0]['binary']['package']
    if not package['name'].endswith(('.tar.gz', '.tgz')):
        sys.exit(f'unexpected JDK archive {package["name"]}')
    return {'name': package['name'], 'url': package['link'], 'sha256': package['checksum'], 'release': listing[0]['release_name']}


def download(url, dest, sha256=None):
    """`url` into `dest`, kept: an existing file with the right checksum is reused."""
    def digest(path):
        return lab.digest(path, 'sha256')
    if dest.exists() and (sha256 is None or digest(dest) == sha256):
        return dest
    dest.parent.mkdir(parents=True, exist_ok=True)
    part = dest.with_name(dest.name + f'.{os.getpid()}.part')
    try:
        with urllib.request.urlopen(urllib.request.Request(url, headers=FETCH_HEADERS), timeout=120) as r, part.open('wb') as out:
            shutil.copyfileobj(r, out)
        if sha256 and digest(part) != sha256:
            sys.exit(f'{url}: sha256 mismatch')
        part.replace(dest)
    finally:
        part.unlink(missing_ok=True)
    return dest


def jdk_major(home):
    """The feature version a JDK folder says it is (its `release` file), or None."""
    try:
        text = (Path(home) / 'release').read_text()
    except OSError:
        return None
    found = re.search(r'JAVA_VERSION="?(\d+)', text)
    return int(found.group(1)) if found and (Path(home) / 'bin' / 'java').exists() else None


def find_jdk(root):
    """The JDK home inside an extracted archive (`jdk-25+8/`, or `jdk-25+8/Contents/Home` on macOS)."""
    for candidate in (*sorted(Path(root).glob('*/Contents/Home')), *sorted(Path(root).glob('*')), Path(root)):
        if jdk_major(candidate):
            return candidate
    return None


def provide_jdk(install, cache, given):
    """The JDK 25 home the install uses: `given`, or Temurin downloaded into the cache (sha256-checked) and extracted into the install."""
    if given:
        home = Path(given).resolve()
        if jdk_major(home) != JDK_FEATURE:
            sys.exit(f'{home} is not a JDK {JDK_FEATURE} (no bin/java, or its release file says {jdk_major(home)})')
        return home
    target = install / 'jdk'
    marker = target / '.horizon-jdk'
    os_name, arch = jdk_platform()
    with urllib.request.urlopen(urllib.request.Request(jdk_query(os_name, arch), headers=FETCH_HEADERS), timeout=60) as r:
        asset = jdk_asset(json.load(r))
    if marker.exists() and marker.read_text() == asset['sha256'] and find_jdk(target):
        return find_jdk(target)
    archive = download(asset['url'], cache / 'jdk' / asset['name'], asset['sha256'])
    building = install / f'jdk.{os.getpid()}'
    shutil.rmtree(building, ignore_errors=True)
    building.mkdir(parents=True)
    with tarfile.open(archive) as tar:
        tar.extractall(building, filter='data')
    (building / '.horizon-jdk').write_text(asset['sha256'])
    shutil.rmtree(target, ignore_errors=True)
    building.rename(target)
    print(f'JDK: {asset["release"]}')
    return find_jdk(target)


def sync_file(source, dest):
    """`dest` as a copy (a hard link when the disk allows) of `source`, untouched when it already is one."""
    if dest.is_symlink() or (dest.exists() and (dest.stat().st_size, dest.stat().st_mtime_ns) != (source.stat().st_size, source.stat().st_mtime_ns)):
        dest.unlink()
    if dest.exists():
        return
    dest.parent.mkdir(parents=True, exist_ok=True)
    try:
        os.link(source, dest)
    except OSError:
        shutil.copy2(source, dest)


SIGNATURE = re.compile(r'^META-INF/[^/]+\.(SF|RSA|DSA|EC)$', re.I)


def sync_stripped(source, dest):
    """`dest` as a copy of the unsigned `source` jar whose manifest keeps only its main section (FML keeps the per-entry digests in memory); a signed jar is linked as is."""
    with zipfile.ZipFile(source) as zin:
        if any(SIGNATURE.match(name) for name in zin.namelist()) or 'META-INF/MANIFEST.MF' not in zin.namelist():
            return sync_file(source, dest)
        stat = source.stat()
        if dest.exists() and not dest.is_symlink() and dest.stat().st_ino != stat.st_ino and dest.stat().st_mtime_ns == stat.st_mtime_ns:
            return
        dest.parent.mkdir(parents=True, exist_ok=True)
        part = dest.with_name(f'{dest.name}.{os.getpid()}.part')
        with zipfile.ZipFile(part, 'w') as zout:
            for info in zin.infolist():
                data = zin.read(info)
                if info.filename == 'META-INF/MANIFEST.MF':
                    data = re.split(rb'\r?\n\r?\n', data, maxsplit=1)[0] + b'\r\n\r\n'
                entry = zipfile.ZipInfo(info.filename, info.date_time)
                entry.external_attr, entry.comment, entry.extra = info.external_attr, info.comment, info.extra
                zout.writestr(entry, data, compress_type=info.compress_type)
    os.utime(part, ns=(stat.st_atime_ns, stat.st_mtime_ns))
    dest.unlink(missing_ok=True)
    os.replace(part, dest)


def patched_client(relative):
    return fnmatch.fnmatch(relative.as_posix(), 'net/neoforged/minecraft-client-patched/*/minecraft-client-patched-*.jar')


def sync_tree(source, dest):
    for path in sorted(source.rglob('*')):
        if path.is_file():
            relative = path.relative_to(source)
            (sync_stripped if patched_client(relative) else sync_file)(path.resolve(), dest / relative)


def sync_links(folder, targets):
    """`folder` holds exactly relative symlinks to `targets`."""
    folder.mkdir(parents=True, exist_ok=True)
    wanted = {t.name for t in targets}
    for old in folder.iterdir():
        if old.name not in wanted and (old.is_symlink() or old.suffix == '.jar'):
            old.unlink()
    for target in targets:
        link = folder / target.name
        want = Path(os.path.relpath(target, folder))
        if not link.is_symlink() or Path(os.readlink(link)) != want:
            link.unlink(missing_ok=True)
            link.symlink_to(want)


def write_if_changed(path, text):
    if not path.exists() or path.read_text() != text:
        path.write_text(text)


def run_name(dedicated):
    return 'server' if dedicated else 'client'


def install_pack(args):
    """Builds the install folder; running it again brings the folder up to date and downloads nothing it has."""
    pack = sorted(Path(args.pack).glob('*.jar'))
    if not pack:
        sys.exit(f'{args.pack} holds no .jar files')
    resources = Path(args.resources).resolve()
    if not (resources / 'asset-index.json').is_file():
        sys.exit(f'{resources} is not a prepared Horizon (no asset-index.json): prepare it first')
    manifest = Path(args.minecraft) / 'manifest.json'
    if not manifest.is_file():
        sys.exit(f'{manifest} is missing: it names the asset index of the Minecraft install')
    index_id = json.loads(manifest.read_text())['assetIndex']['id']
    jars = {name: Path(args.host_jars) / name for name in ('horizon-java-host.jar', 'horizon-java-boot.jar', 'horizon-java-plugin.jar')}
    for jar in jars.values():
        if not jar.is_file():
            sys.exit(f'{jar} is missing: build the Java side with `java_mods.py build`')
    install = Path(args.out).resolve()
    install.mkdir(parents=True, exist_ok=True)
    cache = Path(args.cache) if args.cache else default_cache()
    java = provide_jdk(install, cache, args.java)
    # NeoForge's installers run on this JDK, and keep their downloads in the cache.
    os.environ['PATH'] = f'{java}/bin{os.pathsep}{os.environ["PATH"]}'
    oracle.LOCAL = cache / 'neoforge'
    oracle.LOCAL.mkdir(parents=True, exist_ok=True)
    client_home, neo, _, client_jars = oracle.client()
    server_home = oracle.install()
    libs = install / 'libraries'
    for home in (client_home, server_home):
        sync_tree(home / 'libraries', libs)
    for name, jar in jars.items():
        sync_file(jar.resolve(), install / 'host' / name)
    mods = install / 'mods'
    mods.mkdir(exist_ok=True)
    for old in mods.glob('*.jar'):
        if old.name not in {j.name for j in pack}:
            old.unlink()
    for jar in pack:
        sync_file(jar.resolve(), mods / jar.name)
    assets = assets_index(resources, index_id, install)
    record = {'format': INSTALL_FORMAT, 'neoforge': NEOFORGE, 'java': str(java), 'resources': str(resources),
              'mods': [j.name for j in pack]}
    write_if_changed(install / 'install.json', json.dumps(record, indent=2) + '\n')
    try:
        from . import mod_replaces
    except ImportError:
        import mod_replaces
    analysis = mod_replaces.install_analysis(install, cache / 'mod-replaces')
    mod_links = [*sorted(mods.glob('*.jar')), install / 'host' / 'horizon-java-boot.jar', install / 'host' / 'horizon-java-plugin.jar']
    for dedicated in (False, True):
        run = install / run_name(dedicated)
        run.mkdir(exist_ok=True)
        sync_links(run / 'mods', mod_links)
        if not (run / 'libraries').is_symlink():
            (run / 'libraries').symlink_to(Path('..') / 'libraries')
        if dedicated:
            lines = args_lines('server', libs, NEOFORGE, server=(server_home / 'libraries/net/neoforged/neoforge' / NEOFORGE / 'unix_args.txt').read_text(), analysis=analysis)
            if args.accept_eula:
                write_if_changed(run / 'eula.txt', 'eula=true\n')
        else:
            lines = args_lines('client', libs, NEOFORGE, client=(neo, [libs / j.relative_to(client_home / 'libraries') for j in client_jars]), assets=(assets, index_id), analysis=analysis)
        write_if_changed(run / 'horizon-java.args', '\n'.join(lines) + '\n')
    print(f'{install}: {len(pack)} mods, NeoForge {NEOFORGE}, JDK at {java}')
    if not (install / 'server' / 'eula.txt').exists():
        print('the dedicated server needs its folder\'s eula.txt to say eula=true (or install with --accept-eula)')


def launch_env(install, record, dedicated, base):
    """The environment the game needs for the Java mods of `install`; a home folder the caller set stays theirs, the rest live in the install."""
    java = record['java']
    env = dict(base)
    library_path = 'DYLD_LIBRARY_PATH' if sys.platform == 'darwin' else 'LD_LIBRARY_PATH'
    drivers = '/run/opengl-driver/lib' if Path('/run/opengl-driver/lib').is_dir() else ''
    env[library_path] = ':'.join(p for p in (f'{java}/lib/server', f'{java}/lib', drivers, base.get(library_path, '')) if p)
    env.update(JAVA_HOME=java, HORIZON_NEOFORGE=record['neoforge'], HORIZON_JAVA_HOST_JAR=str(install / 'host' / 'horizon-java-host.jar'),
               HORIZON_JAVA_RUN=str(install / run_name(dedicated)))
    for name, folder in (('HORIZON_HOME', 'horizon'), ('HORIZON_LAUNCHER_HOME', 'launcher'), ('XDG_DATA_HOME', 'xdg-data'), ('XDG_CONFIG_HOME', 'xdg-config')):
        if not env.get(name):
            env[name] = str(install / 'home' / folder)
    return env


def launch_command(game, record, game_args):
    """The game's command line: the install's prepared resources unless the caller names theirs."""
    command = [str(game)]
    if '--resources' not in game_args:
        command += ['--resources', record['resources']]
    return command + game_args


def launch(args, game_args):
    install = Path(args.install).resolve()
    try:
        record = json.loads((install / 'install.json').read_text())
    except OSError:
        sys.exit(f'{install} is not an install: run `java_mods.py install` first')
    dedicated = '--dedicated' in game_args
    game = Path(args.game or oracle.ROOT / 'target' / 'fast' / 'horizon-client')
    if not game.is_file():
        sys.exit(f'{game} is not a file: build the game with --features java-mods, or pass --game')
    env = launch_env(install, record, dedicated, os.environ)
    cwd = install / 'server' if dedicated else Path(env['HORIZON_HOME'])
    cwd.mkdir(parents=True, exist_ok=True)
    for name in ('HORIZON_HOME', 'HORIZON_LAUNCHER_HOME', 'XDG_DATA_HOME', 'XDG_CONFIG_HOME'):
        Path(env[name]).mkdir(parents=True, exist_ok=True)
    os.chdir(cwd)
    os.execve(game, launch_command(game, record, game_args), env)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    sub = parser.add_subparsers(dest='command', required=True)
    sub.add_parser('build').add_argument('--java-only', action='store_true')
    sub.add_parser('unit', help='the host\'s Java unit checks, after build')
    sub.add_parser('check').add_argument('--analysis', help="an install's analysis/entity-facts.json: its entity writers need a bridge")
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
    cr = sub.add_parser('crossing', help='ns per Rust to Java request; fails above 1000 ns for an empty one')
    cr.add_argument('--jars', help='a folder of mod jars; default none')
    cr.add_argument('--rounds', type=int, default=5)
    cr.add_argument('--n', type=int, default=200000)
    cr.add_argument('--soak', type=int, default=3, help='seconds of the three-thread check')
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
    i = sub.add_parser('install', help='a self-contained install folder (JDK, NeoForge, mods, host) from a folder of mod jars')
    i.add_argument('--pack', required=True, help='a folder of mod jars')
    i.add_argument('--out', required=True, help='the install folder, made or brought up to date')
    i.add_argument('--java', help='a JDK 25 home; default: Eclipse Temurin, downloaded and checksum-verified')
    i.add_argument('--cache', help=f'downloads kept here; default {default_cache()}')
    i.add_argument('--resources', default=str(lab.RESOURCES), help='the prepared Horizon resources')
    i.add_argument('--minecraft', default=str(lab.GAME), help='the Minecraft 26.3 install horizon-prepare read (its manifest.json)')
    i.add_argument('--host-jars', default=str(JARS), help='the built horizon-java-{host,boot,plugin}.jar')
    i.add_argument('--accept-eula', action='store_true', help='write eula.txt for the dedicated server: you agree to https://aka.ms/MinecraftEULA')
    x = sub.add_parser('mixins', help='the static list of every mixin handler of an install\'s mods (see mod_parity.py)')
    x.add_argument('--install', required=True)
    x.add_argument('--out', required=True)
    rp = sub.add_parser('replaces', help='the vanilla behaviours an install\'s mods change, read from the jars (see mod_replaces.py)')
    try:
        from . import mod_replaces
    except ImportError:
        import mod_replaces
    mod_replaces.add_arguments(rp)
    n = sub.add_parser('launch', allow_abbrev=False, help='starts the game from an install; other arguments go to the game (--dedicated starts the server)')
    n.add_argument('--install', required=True)
    n.add_argument('--game', help='the horizon-client built with --features java-mods')
    args, extra = parser.parse_known_args()
    if args.command == 'launch':
        return launch(args, [a for a in extra if a != '--'])
    if extra:
        parser.error(f'unrecognized arguments: {" ".join(extra)}')
    if args.command == 'install':
        return install_pack(args)
    if args.command == 'mixins':
        try:
            from . import mod_parity
        except ImportError:
            import mod_parity
        return mod_parity.scan_install(args.install, args.out)
    if args.command == 'replaces':
        return mod_replaces.run(args)
    LOCAL.mkdir(parents=True, exist_ok=True)
    JARS.mkdir(parents=True, exist_ok=True)
    # Several agents build and check at once, and a build replaces the class folders.
    lock = (JARS / 'build.lock').open('w')
    if args.command in ('build', 'check'):
        fcntl.flock(lock, fcntl.LOCK_EX)
    if args.command == 'build':
        build(args.java_only)
    elif args.command == 'unit':
        unit()
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
        check_bridges(args)
    elif args.command == 'crossing':
        crossing(args)
    else:
        bench(args)


if __name__ == '__main__':
    main()
