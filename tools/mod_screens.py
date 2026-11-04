#!/usr/bin/env python3
"""Does it look like this on a real NeoForge client? One scenario, the same world and camera on both, screenshots side by side.

    mod_screens.py --install DIR --scenario S.json [--scenario T.json] --reference REF --out OUT [--world SAVE] [--game BIN] [--size 1600x1200]

Runs the real client (the parity driver mod, Xvfb), then Horizon (hz_scenario.py), never together, each through HORIZON_GAME_GATE.
Writes OUT/NAME-forge.png, OUT/NAME-horizon.png and OUT/NAME-side.png (Forge left, Horizon right) for every {"screenshot": NAME} step.
REF holds natives/, assets/ and game/options.txt of a real client. Images are PNGs read and written with zlib, nothing else. It judges nothing.
"""
import argparse
import json
import os
import shutil
import struct
import subprocess
import sys
import zlib
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
STEP_KINDS = {'wait', 'command', 'look', 'lookat', 'slot', 'use', 'attack', 'key', 'press', 'release', 'close', 'tp', 'camera', 'hud', 'screenshot', 'check', 'quit'}


# The scenario, shared by both sides

def scenario_steps(data):
    """The step list of a scenario file: a bare list or {"steps": [...]}."""
    return data['steps'] if isinstance(data, dict) else data


def check_scenario(data):
    """The screenshot names, in order; ValueError when a step is one a side cannot run."""
    names = []
    for item in scenario_steps(data):
        if 'name' in item:
            continue
        kind = next(iter(item))
        if kind not in STEP_KINDS:
            raise ValueError(f'step kind {kind!r} is not supported by both sides')
        if kind == 'camera' and len(item[kind]) != 5:
            raise ValueError('camera takes [x, y, z, yaw, pitch]')
        if kind == 'screenshot':
            names.append(item[kind])
    if not names:
        raise ValueError('the scenario has no screenshot step')
    return names


# PNG with zlib

def decode_png(data):
    """(width, height, RGB bytes per row) of an 8-bit RGB or RGBA PNG without interlacing."""
    if data[:8] != b'\x89PNG\r\n\x1a\n':
        raise ValueError('not a PNG')
    pos, idat, header = 8, [], None
    while pos < len(data):
        size, kind = struct.unpack('>I4s', data[pos:pos + 8])
        body = data[pos + 8:pos + 8 + size]
        if kind == b'IHDR':
            header = struct.unpack('>IIBBBBB', body)
        elif kind == b'IDAT':
            idat.append(body)
        pos += 12 + size
    width, height, depth, color, _, _, interlace = header
    if depth != 8 or color not in (2, 6) or interlace:
        raise ValueError(f'unsupported PNG (depth {depth}, color type {color}, interlace {interlace})')
    bpp = 3 if color == 2 else 4
    raw = zlib.decompress(b''.join(idat))
    stride = width * bpp
    rows, prev = [], bytes(stride)
    for y in range(height):
        kind, line = raw[y * (stride + 1)], bytearray(raw[y * (stride + 1) + 1:(y + 1) * (stride + 1)])
        if kind == 1:
            for i in range(bpp, stride):
                line[i] = (line[i] + line[i - bpp]) & 255
        elif kind == 2:
            line = bytearray((a + b) & 255 for a, b in zip(line, prev))
        elif kind == 3:
            for i in range(stride):
                line[i] = (line[i] + (((line[i - bpp] if i >= bpp else 0) + prev[i]) >> 1)) & 255
        elif kind == 4:
            for i in range(stride):
                a, b, c = (line[i - bpp] if i >= bpp else 0), prev[i], (prev[i - bpp] if i >= bpp else 0)
                p = a + b - c
                pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
                line[i] = (line[i] + (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255
        elif kind != 0:
            raise ValueError(f'bad filter {kind}')
        prev = bytes(line)
        if bpp == 4:
            rgb = bytearray(width * 3)
            rgb[0::3], rgb[1::3], rgb[2::3] = prev[0::4], prev[1::4], prev[2::4]
            rows.append(bytes(rgb))
        else:
            rows.append(prev)
    return width, height, rows


def encode_png(width, height, rows):
    def chunk(kind, body):
        return struct.pack('>I', len(body)) + kind + body + struct.pack('>I', zlib.crc32(kind + body))
    raw = b''.join(b'\0' + row for row in rows)
    return b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, 2, 0, 0, 0)) + chunk(b'IDAT', zlib.compress(raw, 6)) + chunk(b'IEND', b'')


# Side by side

FONT = {
    'F': ['11111', '10000', '10000', '11110', '10000', '10000', '10000'],
    'O': ['01110', '10001', '10001', '10001', '10001', '10001', '01110'],
    'R': ['11110', '10001', '10001', '11110', '10100', '10010', '10001'],
    'G': ['01110', '10001', '10000', '10111', '10001', '10001', '01111'],
    'E': ['11111', '10000', '10000', '11110', '10000', '10000', '11111'],
    'H': ['10001', '10001', '10001', '11111', '10001', '10001', '10001'],
    'I': ['01110', '00100', '00100', '00100', '00100', '00100', '01110'],
    'Z': ['11111', '00001', '00010', '00100', '01000', '10000', '11111'],
    'N': ['10001', '11001', '10101', '10011', '10001', '10001', '10001'],
    ' ': ['00000'] * 7,
}
BAR, GAP, SCALE, INK, BACK = 48, 8, 4, (255, 255, 255), (32, 32, 32)


def label_pixels(text, width, scale=SCALE):
    """A BAR-high strip of rows with the text at its left, in the bitmap font."""
    rows = [bytearray(bytes(BACK) * width) for _ in range(BAR)]
    x0, y0 = 12, (BAR - 7 * scale) // 2
    for n, char in enumerate(text.upper()):
        for gy, line in enumerate(FONT.get(char, FONT[' '])):
            for gx, bit in enumerate(line):
                if bit == '1':
                    for dy in range(scale):
                        for dx in range(scale):
                            x = x0 + (n * 6 + gx) * scale + dx
                            if x < width:
                                rows[y0 + gy * scale + dy][x * 3:x * 3 + 3] = bytes(INK)
    return [bytes(r) for r in rows]


def side_by_side(left, right, labels=('FORGE', 'HORIZON')):
    """(width, height, rows) of two decoded images, labelled, left then right; the shorter is padded."""
    (lw, lh, lrows), (rw, rh, rrows) = left, right
    height = max(lh, rh)
    width = lw + GAP + rw
    pad = lambda rows, w, h: rows + [bytes(BACK) * w] * (h - len(rows))
    lrows, rrows = pad(lrows, lw, height), pad(rrows, rw, height)
    gap = bytes(BACK) * GAP
    header = [a + gap + b for a, b in zip(label_pixels(labels[0], lw), label_pixels(labels[1], rw))]
    return width, height + BAR, header + [a + gap + b for a, b in zip(lrows, rrows)]


def compose(forge_png, horizon_png):
    return encode_png(*side_by_side(decode_png(Path(forge_png).read_bytes()), decode_png(Path(horizon_png).read_bytes())))


# The runs

OFFLINE_AUTH = [f'-Dminecraft.api.{name}.host=http://127.0.0.1:9' for name in ('auth', 'account', 'session', 'services')]  # a blocked network must fail fast, not time out

def parity():
    try:
        from . import mod_parity
    except ImportError:
        sys.path.insert(0, str(Path(__file__).resolve().parent))
        import mod_parity
    return mod_parity


def prepare_world(world, out):
    """OUT/world: one copy of the world, with commands allowed, that both games copy from."""
    mod_parity = parity()
    target = Path(out) / 'world'
    shutil.rmtree(target, ignore_errors=True)
    shutil.copytree(world, target, ignore=shutil.ignore_patterns('session.lock'))
    level = target / 'level.dat'
    level.write_bytes(mod_parity.allow_commands(level.read_bytes()))
    return target


def mini_install(install, world, out):
    """OUT/install: the install's mods and host, and a home holding only a copy of the world, named W."""
    install, out = Path(install).resolve(), Path(out)
    view = out / 'install'
    shutil.rmtree(view, ignore_errors=True)
    view.mkdir(parents=True)
    for name in ('libraries', 'mods', 'assets-index', 'host', 'client', 'install.json'):
        if (install / name).exists():
            (view / name).symlink_to((install / name).resolve())
    saves = view / 'home' / 'horizon' / 'saves'
    saves.mkdir(parents=True)
    shutil.copytree(world, saves / 'W', ignore=shutil.ignore_patterns('session.lock'))
    return view


def default_world(install):
    saves = Path(install) / 'home' / 'horizon' / 'saves'
    found = sorted(p for p in saves.glob('*') if (p / 'level.dat').is_file())
    return found[0] if found else sys.exit(f'no world under {saves}: pass --world')


def run_forge(args, install, scenario, out, world, jars):
    mod_parity = parity()
    real_gate = mod_parity.acquire_gate
    mod_parity.acquire_gate = lambda owner: real_gate('mod_screens')
    width, height = args.size
    code = mod_parity.run_real(install, scenario, out, world, jars, mod_parity.java_of(install), args.reference, size=(width, height),
                               extra_jvm=[f'-Dhorizon.parity.shots={out}', *OFFLINE_AUTH])
    return code, Path(out) / 'screenshots'


def run_horizon(args, install, scenario, out, world):
    view = mini_install(install, world, out.parent / 'view')
    command = [sys.executable, str(ROOT / 'tools' / 'hz_scenario.py'), 'run', '--install', str(view), '--script', str(scenario), '--out', str(out),
               '--world', 'W', '--size', f'{args.size[0]}x{args.size[1]}', '--timeout', str(args.timeout)]
    if args.game:
        command += ['--game', str(args.game)]
    if args.jars:
        command += ['--jars', str(args.jars)]
    env = dict(os.environ, GATE_OWNER='mod_screens')
    result = subprocess.run(command, env=env, capture_output=True, text=True)
    try:
        parsed = json.loads(result.stdout.strip().splitlines()[-1])
    except (ValueError, IndexError):
        parsed = {'ok': False, 'error': (result.stdout + result.stderr)[-500:]}
    return parsed, out


def parse_size(text):
    width, height = text.lower().split('x')
    return int(width), int(height)


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    parser.add_argument('--install', required=True, help='read only')
    parser.add_argument('--scenario', action='append', required=True, help='a scenario file; repeat for several (each runs both sides once)')
    parser.add_argument('--reference', required=True, help='a real client folder: natives/, assets/, game/options.txt')
    parser.add_argument('--out', required=True)
    parser.add_argument('--world', help='a world folder both games start from (default: the install\'s first save)')
    parser.add_argument('--game', help='a horizon-client built with java-mods (horizon-ctl beside it)')
    parser.add_argument('--jars', help='built horizon-java-*.jar for Horizon')
    parser.add_argument('--size', type=parse_size, default=(1600, 1200), help='window size WxH for both')
    parser.add_argument('--timeout', type=int, default=900, help='seconds per Horizon run')
    parser.add_argument('--skip-forge', action='store_true')
    parser.add_argument('--skip-horizon', action='store_true')
    args = parser.parse_args()
    out = Path(args.out).resolve()
    out.mkdir(parents=True, exist_ok=True)
    install = Path(args.install).resolve()
    scenarios = [(Path(s).resolve(), check_scenario(json.loads(Path(s).read_text()))) for s in args.scenario]
    java = json.loads((install / 'install.json').read_text())['java']
    os.environ['PATH'] = f'{java}/bin:{os.environ["PATH"]}'
    world = prepare_world(Path(args.world) if args.world else default_world(install), out / 'work')
    mod_parity = parity()
    jars = list(mod_parity.build_parity_jars(out / 'work' / 'jars', install))
    report = {'images': [], 'missing': [], 'runs': {}}
    for file, names in scenarios:
        work = out / 'work' / file.stem
        shots = {}
        if not args.skip_forge:
            code, folder = run_forge(args, install, file, work / 'forge', world, jars)
            report['runs'][f'{file.stem}-forge'] = {'exit': code, 'log': str(work / 'forge' / 'game.log')}
            shots['forge'] = folder
        if not args.skip_horizon:
            result, folder = run_horizon(args, install, file, work / 'horizon', world)
            report['runs'][f'{file.stem}-horizon'] = {'ok': result.get('ok'), 'failed': result.get('failed'), 'log': result.get('log')}
            shots['horizon'] = folder
        for name in names:
            paths = {side: folder / f'{name}.png' for side, folder in shots.items()}
            for side, path in paths.items():
                if path.is_file():
                    shutil.copy(path, out / f'{name}-{side}.png')
                    report['images'].append(str(out / f'{name}-{side}.png'))
                else:
                    report['missing'].append(f'{name}-{side}')
            if len(paths) == 2 and all(p.is_file() for p in paths.values()):
                (out / f'{name}-side.png').write_bytes(compose(paths['forge'], paths['horizon']))
                report['images'].append(str(out / f'{name}-side.png'))
    print(json.dumps(report, indent=1))
    sys.exit(1 if report['missing'] else 0)


if __name__ == '__main__':
    main()
