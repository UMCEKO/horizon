#!/usr/bin/env python3
"""Which vanilla behaviours the mods of an install change, read from the jars (no mod code runs), and where each one must run.

    python3 tools/java_mods.py replaces --install DIR --out OUT

Mechanisms found in every jar of DIR/mods and the jars nested in it:
  mixin     an injector or overwrite of a vanilla method (tools/mod_parity.py scan_install); conditional when the config has a plugin
  access    a mixin accessor or invoker; changes nothing
  override  a mod class under a vanilla class (or a NeoForge extension interface) redeclaring a method with the same name AND descriptor
  implement the same where the vanilla method is abstract: there is no vanilla behaviour to replace
  listener  @SubscribeEvent, IEventBus.addListener (event from the lambda's or method reference's instantiated type, or the Class argument)
  data      files under data/minecraft/ and assets/minecraft/, counted by kind; META-INF/accesstransformer.cfg lines, counted only
  opaque    transformation service files and coremods: "can't be analysed ahead", never guessed at; a mixin config plugin only makes its mixins conditional

Routing of one change (the rule is the code of `route`):
  java         a mixin of a vanilla method the port ledger says native owns (ported, mapped, named, structural, linked): that behaviour runs in Java
  java-own-type an override of such a method: native calls the mod's override for the mod's own types only
  undecided    a mixin on a method the ledger can't say native runs, and no verdict yet: decide it in tools/port_map
  java-already native does not own it (absent, skipped, mentioned, inferred, none, not vanilla), the change implements an abstract method,
               or it overrides a client GUI method (mod screens and widgets run in Java)
  native-hook  an event listener: native fires the event
  inert        access only

OUT/replaces.json (format 1):
  mods     {mod: {jars, can_analyse_ahead, opaque: [why], counts: {mechanism: n}, routing: {class: distinct keys}, unresolved_listeners}}
  methods  {"pkg/Class#name": {vanilla, native: {status, owned, fns, note}, routing, mods: [..], changes: [change]}}
           a change is {mod, mechanism, routing, ...}: mixin has kind, handler, desc, at, side, conditional;
           override and implement have desc, classes, classes_n, abstract, extension, scope "own-type"
  events   {"pkg/Event": {cancellable, bus, host_mentions, routing "native-hook", mods, changes}}; a change has via, class, method, priority, bus
  data     {kind: {mod: {files, samples}}}
  totals   {mechanism: n, routing: n, ...}
OUT/routing.json (format 1, tools/mod_routing.py) and routing.tsv, from the mixins routed java:
  routes     {"pkg/Class#name(desc)": {kind edge|body, static, family, args, ret, changes, returns, tail, overriders, closure, hot, side}}
  undecided  {key: {status, mods}}; lambdas {lambda key: enclosing key}; routing.tsv is owner, name, descriptor, kind, tail (0/1), returns
OUT/replaces.md is the per-mod summary and the 40 vanilla methods changed by most mods.
"""
import argparse
import collections
import gzip
import hashlib
import io
import json
import re
import struct
import sys
import time
import tomllib
import zipfile
from pathlib import Path

try:
    from . import mod_parity as mp, mod_parity_explain as ex
except ImportError:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import mod_parity as mp  # noqa: E402
    import mod_parity_explain as ex  # noqa: E402

ROOT = Path(__file__).resolve().parent.parent
LEDGER_JSON = ROOT / '.local' / 'port-ledger' / 'ledger.json'
DEFAULT_CACHE = ROOT / '.local' / 'mod-replaces'
VANILLA_PREFIXES = ('net/minecraft/', 'com/mojang/')
STATUS_ORDER = ('ported', 'mapped', 'named', 'structural', 'linked', 'inferred', 'mentioned', 'skipped', 'absent')
OWNED = ('ported', 'mapped', 'named', 'structural', 'linked')
ROUTING_ORDER = ('java', 'java-own-type', 'undecided', 'java-already', 'native-hook', 'inert')
WEAK_STATUSES = ('mentioned', 'inferred', 'absent', 'none', 'unknown')
PORT_MAP = ROOT / 'tools' / 'port_map'
OPAQUE_PACKAGES = ('net/neoforged/neoforgespi/transformation/', 'net/neoforged/neoforgespi/coremod/', 'cpw/mods/modlauncher/api/', 'org/spongepowered/asm/service/')
OPAQUE_SERVICES = ('net.neoforged.neoforgespi.transformation.ClassProcessorProvider', 'net.neoforged.neoforgespi.coremod.ICoreMod')
OPAQUE_FILES = ('META-INF/coremods.json',)
MIXIN = 'Lorg/spongepowered/asm/mixin/Mixin;'
SUBSCRIBE = 'Lnet/neoforged/bus/api/SubscribeEvent;'
CANCELLABLE = 'net/neoforged/bus/api/ICancellableEvent'
MOD_BUS_EVENT = 'net/neoforged/fml/event/IModBusEvent'
EVENT_BUS = 'net/neoforged/bus/api/IEventBus'
ACC_PRIVATE, ACC_STATIC, ACC_ABSTRACT = 0x2, 0x8, 0x400
ACTIVE_DATA = ('data/minecraft/', 'assets/minecraft/')


# Class files

def u2(d, p):
    return (d[p] << 8) | d[p + 1]


def u4(d, p):
    return struct.unpack_from('>I', d, p)[0]


class Pool:
    def __init__(self, data):
        count = u2(data, 8)
        self.data, self.e = data, [None] * count
        pos, i = 10, 1
        while i < count:
            tag = data[pos]
            if tag == 1:
                n = u2(data, pos + 1)
                self.e[i] = (1, pos + 3, n)
                pos += 3 + n
            elif tag == 3:
                self.e[i] = (3, struct.unpack_from('>i', data, pos + 1)[0])
                pos += 5
            elif tag == 4:
                pos += 5
            elif tag in (5, 6):
                pos += 9
                i += 1
            elif tag in (7, 8, 16, 19, 20):
                self.e[i] = (tag, u2(data, pos + 1))
                pos += 3
            elif tag in (9, 10, 11, 12, 17, 18):
                self.e[i] = (tag, u2(data, pos + 1), u2(data, pos + 3))
                pos += 5
            elif tag == 15:
                self.e[i] = (15, data[pos + 1], u2(data, pos + 2))
                pos += 4
            else:
                raise ValueError(f'constant pool tag {tag}')
            i += 1
        self.end = pos

    def utf(self, i):
        _, start, n = self.e[i]
        return self.data[start:start + n].decode('utf-8', 'replace')

    def cls(self, i):
        return self.utf(self.e[i][1]) if i else None

    def nat(self, i):
        _, n, d = self.e[i]
        return self.utf(n), self.utf(d)

    def ref(self, i):
        _, c, nt = self.e[i]
        return (self.cls(c), *self.nat(nt))


def element(d, p, pool):
    """One annotation element value at p: (value, next position). Enums give the constant's name, classes their descriptor."""
    tag = chr(d[p])
    if tag == 's':
        return pool.utf(u2(d, p + 1)), p + 3
    if tag == 'e':
        return pool.utf(u2(d, p + 3)), p + 5
    if tag == 'c':
        return pool.utf(u2(d, p + 1)), p + 3
    if tag == '@':
        return annotation(d, p + 1, pool)
    if tag == '[':
        items, p = [], p + 3
        for _ in range(u2(d, p - 2)):
            v, p = element(d, p, pool)
            items.append(v)
        return items, p
    entry = pool.e[u2(d, p + 1)]
    return (entry[1] if tag == 'Z' and entry[0] == 3 else None), p + 3


def annotation(d, p, pool):
    """((descriptor, {name: value}), next position)."""
    desc, values = pool.utf(u2(d, p)), {}
    count, p = u2(d, p + 2), p + 4
    for _ in range(count):
        name = pool.utf(u2(d, p))
        values[name], p = element(d, p + 2, pool)
    return (desc, values), p


def read_annotations(d, p, pool):
    found = {}
    count, p = u2(d, p), p + 2
    for _ in range(count):
        (desc, values), p = annotation(d, p, pool)
        found[desc] = values
    return found


LENGTHS = {0x10: 2, 0x11: 3, 0x12: 2, 0x13: 3, 0x14: 3, 0x84: 3, 0xa9: 2, 0xb9: 5, 0xba: 5, 0xbc: 2, 0xc5: 4, 0xc8: 5, 0xc9: 5}
for _op in list(range(0x15, 0x1a)) + list(range(0x36, 0x3b)):
    LENGTHS[_op] = 2
for _op in list(range(0x99, 0xa9)) + list(range(0xb2, 0xb9)) + [0xbb, 0xbd, 0xc0, 0xc1, 0xc6, 0xc7]:
    LENGTHS[_op] = 3


def op_length(code, p):
    """The length in bytes of the instruction at p."""
    op = code[p]
    if op == 0xaa:
        pad = 1 + (-(p + 1) % 4)
        low, high = struct.unpack_from('>ii', code, p + pad + 4)
        return pad + 12 + 4 * (high - low + 1)
    if op == 0xab:
        pad = 1 + (-(p + 1) % 4)
        return pad + 8 + 8 * u4(code, p + pad + 4)
    if op == 0xc4:
        return 6 if code[p + 1] == 0x84 else 4
    return LENGTHS.get(op, 1)


def scan_code(code, pool):
    """[('static', owner, name) | ('class', name) | ('call', owner, name, desc) | ('indy', bootstrap, name, desc)] in order."""
    events = []
    p, n = 0, len(code)
    while p < n:
        op = code[p]
        if op in (0x12, 0x13):
            entry = pool.e[code[p + 1] if op == 0x12 else u2(code, p + 1)]
            if entry and entry[0] == 7:
                events.append(('class', pool.utf(entry[1])))
        elif op == 0xb2:
            owner, name, _ = pool.ref(u2(code, p + 1))
            events.append(('static', owner, name))
        elif 0xb6 <= op <= 0xb9:
            events.append(('call', *pool.ref(u2(code, p + 1))))
        elif op == 0xba:
            _, bootstrap, nt = pool.e[u2(code, p + 1)]
            events.append(('indy', bootstrap, *pool.nat(nt)))
        p += op_length(code, p)
    return events


def parse_class(data, members=True, annotations=False, code=False):
    """{name, super, ifaces, access, methods, annos, bootstraps, pool}; a method is {name, desc, access, annos, events}."""
    pool = Pool(data)
    p = pool.end
    access, this, sup = u2(data, p), u2(data, p + 2), u2(data, p + 4)
    ifaces = [pool.cls(u2(data, p + 8 + 2 * i)) for i in range(u2(data, p + 6))]
    info = {'name': pool.cls(this), 'super': pool.cls(sup), 'ifaces': ifaces, 'access': access, 'methods': [], 'annos': {}, 'bootstraps': [], 'pool': pool}
    if not members:
        return info
    p += 8 + 2 * len(ifaces)
    fields, p = u2(data, p), p + 2
    for _ in range(fields):
        attrs, p = u2(data, p + 6), p + 8
        for _ in range(attrs):
            p += 6 + u4(data, p + 2)
    count, p = u2(data, p), p + 2
    for _ in range(count):
        m = {'access': u2(data, p), 'name': pool.utf(u2(data, p + 2)), 'desc': pool.utf(u2(data, p + 4)), 'annos': {}, 'events': []}
        attrs, p = u2(data, p + 6), p + 8
        for _ in range(attrs):
            kind, size = pool.utf(u2(data, p)), u4(data, p + 2)
            if kind == 'RuntimeVisibleAnnotations' and annotations:
                m['annos'] = read_annotations(data, p + 6, pool)
            elif kind == 'Code' and code:
                m['code'] = data[p + 14:p + 14 + u4(data, p + 10)]
                m['events'] = scan_code(m['code'], pool)
            p += 6 + size
        info['methods'].append(m)
    attrs, p = u2(data, p), p + 2
    for _ in range(attrs):
        kind, size = pool.utf(u2(data, p)), u4(data, p + 2)
        if kind == 'RuntimeVisibleAnnotations' and annotations:
            info['annos'] = read_annotations(data, p + 6, pool)
        elif kind == 'BootstrapMethods' and code:
            q = p + 8
            for _ in range(u2(data, p + 6)):
                args = [u2(data, q + 4 + 2 * i) for i in range(u2(data, q + 2))]
                info['bootstraps'].append((u2(data, q), args))
                q += 4 + 2 * len(args)
        p += 6 + size
    return info


def params(desc):
    """The parameter types of a method descriptor: class names (`a/B`), arrays and primitives as their descriptor text."""
    out, i = [], 1
    while desc[i] != ')':
        start = i
        while desc[i] == '[':
            i += 1
        i = desc.index(';', i) + 1 if desc[i] == 'L' else i + 1
        t = desc[start:i]
        out.append(t[1:-1] if t[0] == 'L' else t)
    return out


# Libraries: the vanilla jars, NeoForge, the loader and the event bus

def is_extension(name):
    return name.startswith('net/neoforged/neoforge/') and '/extensions/I' in name and name.endswith('Extension')


def is_vanilla(name):
    return name.startswith(VANILLA_PREFIXES)


def library_jars(libraries):
    libs = Path(libraries)
    jars = list(ex.patched_jars(libs))
    for pattern in ('net/neoforged/bus/*/bus-*.jar', 'net/fabricmc/sponge-mixin/*/*.jar'):
        found = sorted(p for p in libs.glob(pattern) if not p.name.endswith('-sources.jar'))
        jars += found[-1:]
    return jars


def build_library_index(jars):
    """{class: [super, interfaces, access, [[name, desc, access]]]}; methods are kept for vanilla classes and extension interfaces only."""
    out = {}
    for jar in jars:
        with zipfile.ZipFile(jar) as z:
            for entry in z.namelist():
                if not entry.endswith('.class') or entry.startswith('META-INF/') or entry == 'module-info.class':
                    continue
                try:
                    info = parse_class(z.read(entry))
                except (ValueError, IndexError, struct.error):
                    continue
                keep = is_vanilla(info['name']) or is_extension(info['name'])
                methods = [[m['name'], m['desc'], m['access']] for m in info['methods']] if keep else []
                if info['name'] in out:
                    seen = {(m[0], m[1]) for m in out[info['name']][3]}
                    out[info['name']][3] += [m for m in methods if (m[0], m[1]) not in seen]
                else:
                    out[info['name']] = [info['super'], info['ifaces'], info['access'], methods]
    return out


def load_library_index(jars, cache):
    stamp = [(str(j), Path(j).stat().st_size, Path(j).stat().st_mtime_ns) for j in jars]
    path = Path(cache) / f'library-{hashlib.sha256(json.dumps(stamp).encode()).hexdigest()[:20]}.json.gz' if cache else None
    if path and path.is_file():
        return json.loads(gzip.decompress(path.read_bytes()))
    index = build_library_index(jars)
    if path:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(gzip.compress(json.dumps(index).encode()))
    return index


class Hierarchy:
    """Superclasses and interfaces of every class the libraries and the mods define."""

    def __init__(self, libs):
        self.libs = libs
        self.jars = []
        self.heads = {n: (e[0], e[1]) for n, e in libs.items()}
        self._ancestors = {}

    def add(self, name, sup, ifaces):
        self.heads[name] = (sup, ifaces)

    def ancestors(self, name):
        """Supers breadth first, the class itself left out."""
        if name not in self._ancestors:
            seen, queue = [], [name]
            while queue:
                head = self.heads.get(queue.pop(0))
                if not head:
                    continue
                for parent in ([head[0]] if head[0] else []) + list(head[1]):
                    if parent not in seen:
                        seen.append(parent)
                        queue.append(parent)
            self._ancestors[name] = seen
        return self._ancestors[name]

    def is_a(self, name, target):
        return name == target or target in self.ancestors(name)

    def methods_of(self, cls):
        return self.libs.get(cls, (None, (), 0, ()))[3]

    def vanilla_ancestors(self, name):
        return [a for a in self.ancestors(name) if self.methods_of(a)]

    def declarer(self, cls, name, desc=None):
        """The nearest class at or above cls that declares the method (name, and descriptor when given), else cls."""
        for c in [cls] + self.ancestors(cls):
            if any(m[0] == name and (desc is None or m[1] == desc) for m in self.methods_of(c)):
                return c
        return cls

    def event_facts(self, event):
        if event not in self.heads:
            return {'cancellable': None, 'bus': None}
        return {'cancellable': self.is_a(event, CANCELLABLE), 'bus': 'mod' if self.is_a(event, MOD_BUS_EVENT) else 'game'}


def spi_names(jars):
    """Dotted names of the transformation SPI types the loader jars define, plus the known ones."""
    found = set(OPAQUE_SERVICES)
    for jar in jars:
        with zipfile.ZipFile(jar) as z:
            for entry in z.namelist():
                if entry.endswith('.class') and entry.startswith(OPAQUE_PACKAGES) and '$' not in entry:
                    found.add(entry[:-6].replace('/', '.'))
    return found


# The port ledger

class Native:
    """What the port ledger says about a vanilla method: who owns it in Rust."""

    def __init__(self, ledger):
        self.ledger = ledger
        self.methods = ledger.get('methods', {})
        self.fns = ledger.get('fns', [])

    def fn_ids(self, entry):
        entries = entry if isinstance(entry, list) else [entry]
        ids = [e.get('fn') if isinstance(e, dict) else e for e in entries]
        return [i for i in ids if isinstance(i, int) and i < len(self.fns)]

    def info(self, key):
        for kind in STATUS_ORDER:
            entry = self.ledger.get(kind, {}).get(key)
            if entry is None:
                continue
            fns = [f"{self.fns[i]['file']}:{self.fns[i]['line']}" for i in self.fn_ids(entry)[:3]]
            note = (entry.get('note') or entry.get('reason') or '') if isinstance(entry, dict) else ''
            return {'status': kind, 'owned': kind in OWNED and bool(fns), 'fns': fns, 'note': note}
        return {'status': 'none' if key in self.methods else 'unknown', 'owned': False, 'fns': [], 'note': ''}


def read_verdicts(directory=PORT_MAP):
    """{`Class.method`: verdict} from the verdict map's rows (tools/port_map/*.tsv, column 2)."""
    out = {}
    for path in sorted(Path(directory).glob('*.tsv')):
        for line in path.read_text(encoding='utf-8').splitlines():
            parts = [x.strip() for x in line.split('\t')]
            if len(parts) > 1 and parts[0] and not parts[0].startswith('#'):
                out[parts[0]] = parts[1]
    return out


def verdict_of(verdicts, key):
    """The verdict map's answer for a key `pkg/Outer$Inner#name(desc)`, else None."""
    cls, _, rest = key.partition('#')
    name = rest.partition('(')[0]
    simple = cls.rsplit('/', 1)[-1]
    for candidate in (simple, simple.rsplit('$', 1)[-1]):
        if f'{candidate}.{name}' in (verdicts or {}):
            return verdicts[f'{candidate}.{name}']
    return None


def route(mechanism, native, key='', verdicts=None):
    """See the module docstring."""
    if mechanism == 'access':
        return 'inert'
    if mechanism == 'listener':
        return 'native-hook'
    if mechanism == 'mixin' and native['status'] in WEAK_STATUSES:
        verdict = verdict_of(verdicts, key)
        if verdict in ('port', 'inline'):
            return 'java'
        return 'java-already' if verdict == 'missing' or (verdict or '').startswith('skip:') else 'undecided'
    if mechanism == 'implement' or not native['owned']:
        return 'java-already'
    if mechanism == 'mixin':
        return 'java'
    return 'java-already' if key.startswith('net/minecraft/client/gui/') else 'java-own-type'


# Reading the jars

def walk(archive, owner=None, label=''):
    """Yields (mod, label, ZipFile, neoforge.mods.toml) for a jar and the jars nested in it, naming mods as mod_parity does."""
    names = set(archive.namelist())
    toml = tomllib.loads(archive.read('META-INF/neoforge.mods.toml').decode('utf-8', 'replace')) if 'META-INF/neoforge.mods.toml' in names else {}
    ids = [m.get('modId') for m in toml.get('mods', []) if m.get('modId')]
    mod = ids[0] if ids else owner or label
    yield mod, label, archive, toml
    for name in sorted(names):
        if name.startswith('META-INF/jarjar/') and name.endswith('.jar'):
            yield from walk(zipfile.ZipFile(io.BytesIO(archive.read(name))), mod, f'{label}>{Path(name).stem}')


def declared_configs(archive, toml):
    configs = [m['config'] for m in toml.get('mixins', []) if 'config' in m]
    if 'META-INF/MANIFEST.MF' in archive.namelist():
        for line in archive.read('META-INF/MANIFEST.MF').decode('utf-8', 'replace').replace('\r\n ', '').splitlines():
            if line.startswith('MixinConfigs:'):
                configs += [c.strip() for c in line.split(':', 1)[1].split(',') if c.strip()]
    return list(dict.fromkeys(configs))


def data_kind(name):
    """`data/minecraft/tags/block/x.json` is `data:tags/block`; `assets/minecraft/lang/en_us.json` is `assets:lang`."""
    root = 'data' if name.startswith('data/') else 'assets'
    parts = name[len('data/minecraft/' if root == 'data' else 'assets/minecraft/'):].split('/')
    if len(parts) < 2:
        return f'{root}:(root)'
    return f'{root}:' + '/'.join(parts[:2] if parts[0] in ('tags', 'worldgen') and len(parts) > 2 else parts[:1])


def jar_facts(archive, toml, spis):
    """Data overrides, access transformer lines, opaque transformers and mixin config plugins of one archive."""
    names = archive.namelist()
    data = collections.defaultdict(list)
    for name in names:
        if name.startswith(ACTIVE_DATA) and not name.endswith('/'):
            data[data_kind(name)].append(name)
    at_files = ['META-INF/accesstransformer.cfg'] + [a['file'] for a in toml.get('accessTransformers', []) if a.get('file')]
    at_lines = 0
    for f in dict.fromkeys(at_files):
        if f in names:
            at_lines += sum(1 for line in archive.read(f).decode('utf-8', 'replace').splitlines() if line.split('#')[0].strip())
    opaque = []
    for name in names:
        if name.startswith('META-INF/services/') and name.count('/') == 2 and name.rsplit('/', 1)[1] in spis:
            providers = [line.split('#')[0].strip() for line in archive.read(name).decode('utf-8', 'replace').splitlines() if line.split('#')[0].strip()]
            opaque.append(f'service {name.rsplit("/", 1)[1]}: {", ".join(providers) or "(empty)"}')
        elif name in OPAQUE_FILES or (name.startswith('META-INF/coremods/') and name.endswith('.js')):
            opaque.append(f'coremod file {name}')
    plugins = {}
    for config in declared_configs(archive, toml):
        if config in names:
            plugin = mp.lenient_json(archive.read(config).decode('utf-8', 'replace')).get('plugin')
            if plugin:
                plugins[config] = plugin
    return dict(data), at_lines, opaque, plugins


def lambda_event(info, bootstrap):
    """The event class of a listener lambda or method reference: the instantiated parameter type, else the target's last parameter."""
    pool = info['pool']
    if bootstrap >= len(info['bootstraps']):
        return None
    args = info['bootstraps'][bootstrap][1]
    if len(args) < 3:
        return None
    instantiated = params(pool.utf(pool.e[args[2]][1]))
    if instantiated and instantiated[0] != 'java/lang/Object' and not instantiated[0].startswith('['):
        return instantiated[0]
    handle = pool.e[args[1]]
    if handle and handle[0] == 15:
        target = params(pool.ref(handle[2])[2])
        if target and len(target[-1]) > 1 and not target[-1].startswith('['):
            return target[-1]
    return None


def lambda_impl(info, bootstrap):
    """(class, name, descriptor, static) of the method a listener lambda or method reference runs, else None."""
    pool = info['pool']
    if bootstrap >= len(info['bootstraps']):
        return None
    args = info['bootstraps'][bootstrap][1]
    handle = pool.e[args[1]] if len(args) > 1 else None
    if not handle or handle[0] != 15:
        return None
    return (*pool.ref(handle[2]), handle[1] == 6)


def class_listeners(info, hier):
    """[(event, via, method, priority, receive_canceled, explicit bus, implementation)] of one parsed class; event is None when it can't be resolved.

    The implementation is (class, name, descriptor, static), or None when it isn't one method."""
    found = []
    subscriber = next((v for d, v in info['annos'].items() if d.endswith('/EventBusSubscriber;')), None)
    explicit = str(subscriber['bus']).lower() if subscriber and subscriber.get('bus') else None
    for m in info['methods']:
        sub = m['annos'].get(SUBSCRIBE)
        if sub is not None and params(m['desc']):
            via = 'subscriber-class' if subscriber is not None else 'subscribe'
            found.append((params(m['desc'])[0], via, m['name'], sub.get('priority', 'NORMAL'), bool(sub.get('receiveCanceled')), explicit,
                          (info['name'], m['name'], m['desc'], bool(m['access'] & ACC_STATIC))))
        indy = cls = priority = impl = None
        for ev in m['events']:
            if ev[0] == 'indy' and ev[3].endswith(')Ljava/util/function/Consumer;'):
                indy = lambda_event(info, ev[1])
                impl = lambda_impl(info, ev[1])
            elif ev[0] == 'class':
                cls = ev[1]
            elif ev[0] == 'static' and ev[1] == 'net/neoforged/bus/api/EventPriority':
                priority = ev[2]
            elif ev[0] == 'call' and ev[2] in ('addListener', 'addGenericListener') and ev[1] and hier.is_a(ev[1], EVENT_BUS):
                event = cls if 'Ljava/lang/Class;' in ev[3] and ev[2] == 'addListener' and cls else indy
                found.append((event, 'addListener', m['name'], priority if 'EventPriority;' in ev[3] and priority else 'NORMAL', False, None, impl))
                indy = cls = priority = impl = None
    return found


# The analysis

def split_target(target):
    """`Lx/Y;item(II)V` and `item` give (name, descriptor or None)."""
    target = re.sub(r'^L[^;]+;', '', target)
    name, paren, rest = target.partition('(')
    return name, (paren + rest) if paren else None


def mixin_records(entries, plugins, hier, native):
    """[(key, vanilla, record)] for the handlers of mixins.json."""
    out = []
    for e in entries:
        conditional = plugins.get((e['mod'], e['config']))
        for dotted in e['target_class']:
            cls = dotted.replace('.', '/')
            targets = [(e['handler'], None)] if e['access'] else [split_target(t) for t in e['target_method']] or [('<unknown>', None)]
            for name, desc in targets:
                owner = cls if e['access'] else hier.declarer(cls, name, desc)
                record = {'mod': e['mod'], 'mechanism': 'access' if e['access'] else 'mixin', 'kind': e['kind'], 'handler': f'{e["mixin"]}.{e["handler"]}',
                          'desc': desc, 'at': e['at'], 'side': e['side'], 'conditional': f'config plugin {conditional}' if conditional else None}
                out.append((f'{owner}#{name}', is_vanilla(owner), record))
    return out


def override_records(info, mod, hier):
    """{(key, desc): extension?, abstract?} for the methods of a mod class that redeclare a vanilla method."""
    if MIXIN in info['annos']:
        return {}
    supers = hier.vanilla_ancestors(info['name'])
    found = {}
    for m in info['methods']:
        if m['name'] in ('<init>', '<clinit>') or m['access'] & (ACC_STATIC | ACC_PRIVATE):
            continue
        for ancestor in supers:
            hit = next((v for v in hier.methods_of(ancestor) if v[0] == m['name'] and v[1] == m['desc']), None)
            if hit:
                if not hit[2] & (ACC_STATIC | ACC_PRIVATE):
                    found[(f'{ancestor}#{m["name"]}', m['desc'])] = (ancestor.startswith('net/neoforged/'), bool(hit[2] & ACC_ABSTRACT))
                break
    return found


def top_routing(routes):
    return min(routes, key=ROUTING_ORDER.index)


ANIMATE_CLOSURE = ROOT / 'mods' / 'horizon' / 'src' / 'java-mods-host' / 'boot' / 'res' / 'animate_closure.tsv'


def animate_classes(report):
    """The mod classes that override a vanilla `animateTick`, whatever the ledger says of the method: the boot runs them in Java for the mod's own blocks and fluids."""
    found = set()
    for key, entry in report['methods'].items():
        if not key.endswith('#animateTick'):
            continue
        for change in entry['changes']:
            if change['mechanism'] == 'override' and not change['abstract']:
                found.update(change['classes'])
    return sorted(found)


def animate_routing(report, closure_tsv=ANIMATE_CLOSURE):
    """What the mixins in `report` make Java run of the animate code, by the boot's rule (`Animate.route`): `wholeLoop`, and the block and fluid classes patched."""
    rows = [line.split('\t') for line in Path(closure_tsv).read_text().splitlines() if line]
    whole, blocks, fluids = False, set(), set()
    for key, entry in report['methods'].items():
        owner, _, name = key.partition('#')
        for change in entry['changes']:
            if change['mechanism'] != 'mixin':
                continue
            for role, row_owner, row_name, desc, *served in rows:
                if row_owner != owner or row_name != name or (change.get('desc') and change['desc'] != desc):
                    continue
                if role == 'loop':
                    whole = True
                elif role == 'block':
                    blocks.update(served[0].split(','))
                else:
                    fluids.update(served[0].split(','))
    return {'wholeLoop': whole, 'patchedBlocks': sorted(blocks), 'patchedFluids': sorted(fluids)}


def host_words(lines=None):
    return set(re.findall(r'\w+', '\n'.join(text for _, _, text in (ex.host_lines() if lines is None else lines))))


def analyse(install, out, ledger, cache=None, entries=None, host=None, log=print, verdicts=None, vanilla_jar=None, bridges=None):
    """The whole result (replaces.json) for an install; `entries` skips the mixin scan."""
    install, out = Path(install), Path(out)
    started = time.time()
    jars = library_jars(install / 'libraries')
    hier = Hierarchy(load_library_index(jars, cache))
    hier.jars = jars
    spis = spi_names(jars)
    log(f'libraries: {len(hier.libs)} classes, {time.time() - started:.1f}s')
    archives, classes = [], []
    for jar in sorted((install / 'mods').glob('*.jar')):
        for mod, label, archive, toml in walk(zipfile.ZipFile(jar), None, jar.stem):
            archives.append((mod, label, archive, toml))
            for entry in archive.namelist():
                if not entry.endswith('.class') or entry.startswith('META-INF/') or entry == 'module-info.class':
                    continue
                data = archive.read(entry)
                try:
                    head = parse_class(data, members=False)
                except (ValueError, IndexError, struct.error):
                    continue
                hier.add(head['name'], head['super'], head['ifaces'])
                marked = b'addListener' in data or b'addGenericListener' in data or b'SubscribeEvent' in data
                classes.append((len(archives) - 1, entry, head['name'], marked, b'addListener' in data or b'addGenericListener' in data))
    log(f'scanned {len(classes)} mod classes in {len(archives)} archives, {time.time() - started:.1f}s')

    mods = collections.defaultdict(lambda: {'jars': [], 'opaque': [], 'counts': collections.Counter(), 'unresolved_listeners': 0})
    data_total = collections.defaultdict(dict)
    plugins = {}
    for mod, label, archive, toml in archives:
        data, at_lines, opaque, plug = jar_facts(archive, toml, spis)
        row = mods[mod]
        row['jars'].append(label)
        row['opaque'] += opaque
        if at_lines:
            row['counts']['access_transformer'] += at_lines
        for config, plugin in plug.items():
            plugins[(mod, config)] = plugin
        for kind, files in data.items():
            row['counts']['data'] += len(files)
            data_total[kind][mod] = {'files': len(files) + data_total[kind].get(mod, {}).get('files', 0), 'samples': sorted(files)[:3]}

    if entries is None:
        entries = mp.scan_install(install, Path(out) / 'mixins')
    log(f'mixin handlers: {len(entries)}, {time.time() - started:.1f}s')
    native = Native(ledger)
    methods = collections.defaultdict(lambda: {'changes': []})
    events = collections.defaultdict(lambda: {'changes': []})
    vanilla_of = {}
    try:
        from . import mod_routing as mrt
    except ImportError:
        import mod_routing as mrt
    vanilla_classes = mrt.Vanilla(vanilla_jar or hier.jars)
    for key, vanilla, record in mrt.fold_lambdas(mixin_records(entries, plugins, hier, native), hier, vanilla_classes):
        methods[key]['changes'].append(record)
        vanilla_of[key] = vanilla
        mods[record['mod']]['counts'][record['mechanism']] += 1

    overrides = collections.defaultdict(lambda: {'classes': set()})
    listeners = collections.defaultdict(set)
    for archive_index, entry, name, marked, listens in classes:
        mod = archives[archive_index][0]
        if not marked and not hier.vanilla_ancestors(name):
            continue
        try:
            info = parse_class(archives[archive_index][2].read(entry), annotations=True, code=listens)
        except (ValueError, IndexError, struct.error):
            continue
        for (key, desc), (extension, abstract) in override_records(info, mod, hier).items():
            item = overrides[(mod, key, desc)]
            item['classes'].add(name.replace('/', '.'))
            item.update(extension=extension, abstract=abstract)
        for event, via, method, priority, receive_canceled, explicit, _ in class_listeners(info, hier):
            if event is None:
                mods[mod]['unresolved_listeners'] += 1
                continue
            facts = hier.event_facts(event)
            listeners[(mod, event, via, name.replace('/', '.'), method, priority, receive_canceled)].add(explicit or facts['bus'])
    for (mod, key, desc), item in overrides.items():
        classes_list = sorted(item['classes'])
        methods[key]['changes'].append({'mod': mod, 'mechanism': 'implement' if item['abstract'] else 'override', 'desc': desc, 'classes': classes_list[:10], 'classes_n': len(classes_list),
                                        'abstract': item['abstract'], 'extension': item['extension'], 'scope': 'own-type'})
        vanilla_of[key] = True
        mods[mod]['counts']['implement' if item['abstract'] else 'override'] += 1
    words = host_words(host)
    for (mod, event, via, cls, method, priority, receive_canceled), buses in listeners.items():
        facts = hier.event_facts(event)
        bus = sorted(b for b in buses if b)
        events[event]['cancellable'] = facts['cancellable']
        events[event]['host_mentions'] = event.rsplit('/', 1)[-1].split('$')[0] in words
        events[event]['changes'].append({'mod': mod, 'mechanism': 'listener', 'via': via, 'class': cls, 'method': method, 'priority': priority,
                                         'receive_canceled': receive_canceled, 'bus': bus[0] if bus else None})
        mods[mod]['counts']['listener'] += 1

    totals = collections.Counter()
    for key, row in methods.items():
        row['vanilla'] = vanilla_of.get(key, True)
        row['native'] = native.info(key) if row['vanilla'] else {'status': 'not-vanilla', 'owned': False, 'fns': [], 'note': ''}
        for change in row['changes']:
            change['routing'] = route(change['mechanism'], row['native'], key, verdicts)
            if change['mechanism'] == 'mixin' and change['routing'] == 'java-already' and row['native']['status'] in WEAK_STATUSES and verdict_of(verdicts, key) == 'missing':
                change['note'] = 'native lacks it'
            totals[change['mechanism']] += 1
        row['routing'] = top_routing([c['routing'] for c in row['changes']])
        row['mods'] = sorted({c['mod'] for c in row['changes']})
        totals['routing:' + row['routing']] += 1
        for c in row['changes']:
            mods[c['mod']].setdefault('keys', collections.defaultdict(set))[c['routing']].add(key)
    for event, row in events.items():
        row['routing'] = 'native-hook'
        row['bus'] = sorted({c['bus'] for c in row['changes'] if c['bus']})
        row['mods'] = sorted({c['mod'] for c in row['changes']})
        totals['listener'] += len(row['changes'])
        totals['routing:native-hook'] += 1
        for c in row['changes']:
            mods[c['mod']].setdefault('keys', collections.defaultdict(set))['native-hook'].add('event:' + event)
    totals['data'] = sum(r['counts']['data'] for r in mods.values())
    totals['access_transformer'] = sum(r['counts']['access_transformer'] for r in mods.values())

    summary = {}
    for mod, row in sorted(mods.items()):
        keys = row.get('keys', {})
        summary[mod] = {'jars': row['jars'], 'can_analyse_ahead': not row['opaque'], 'opaque': sorted(set(row['opaque'])), 'counts': dict(sorted(row['counts'].items())),
                        'routing': {r: len(keys.get(r, ())) for r in ROUTING_ORDER if keys.get(r)}, 'unresolved_listeners': row['unresolved_listeners']}
    try:
        from . import mod_entity_facts as ef
    except ImportError:
        import mod_entity_facts as ef
    entity_facts = ef.analyse(classes, archives, hier, entries, {'mods': summary, 'methods': methods})
    routes = mrt.routing({'methods': methods}, archives, hier, ledger, verdicts, None, bridges, vanilla_classes)
    return {'format': 1, 'entity_facts': entity_facts, 'routing': routes, 'install': str(install), 'mods': summary, 'methods': dict(sorted(methods.items())), 'events': dict(sorted(events.items())),
            'data': {k: dict(sorted(v.items())) for k, v in sorted(data_total.items())}, 'totals': dict(sorted(totals.items())), 'seconds': round(time.time() - started, 1)}


# The report

def table(headers, rows):
    return ['| ' + ' | '.join(headers) + ' |', '|' + '---|' * len(headers)] + ['| ' + ' | '.join(str(c) for c in r) + ' |' for r in rows]


def short(key):
    cls, _, name = key.partition('#')
    return f'{cls.rsplit("/", 1)[-1]}#{name}'


def markdown(result, top=40):
    mods, methods = result['mods'], result['methods']
    lines = [f'# Vanilla behaviour the mods of {result["install"]} change', '', f'Totals: {json.dumps(result["totals"])}', '', '## Per mod', '']
    rows = [(m, *(s['counts'].get(k, 0) for k in ('mixin', 'access', 'override', 'implement', 'listener', 'data', 'access_transformer')),
              *(s['routing'].get(r, 0) for r in ROUTING_ORDER), 'no' if not s['can_analyse_ahead'] else 'yes') for m, s in mods.items()]
    lines += table(['mod', 'mixin', 'access', 'override', 'implement', 'listener', 'data files', 'AT lines', *ROUTING_ORDER, 'analysable'], rows)
    opaque = [(m, s['opaque']) for m, s in mods.items() if s['opaque']]
    if opaque:
        lines += ['', "## Can't be analysed ahead", ''] + [f'- {m}: {"; ".join(o)}' for m, o in opaque]
    ranked = sorted(((k, v) for k, v in methods.items() if v['vanilla'] and v['routing'] != 'inert'), key=lambda kv: (-len(kv[1]['mods']), kv[0]))[:top]
    lines += ['', f'## Top {top} vanilla methods by number of mods that change them', '']
    lines += table(['method', 'mods', 'mechanisms', 'routing', 'native'],
                   [(f'`{short(k)}`', len(v['mods']), ', '.join(sorted({c['mechanism'] for c in v['changes']})), v['routing'], v['native']['status']) for k, v in ranked])
    report = result.get('routing')
    if report:
        routes = report['routes']
        lines += ['', '## Routes', '', f'{len(routes)} routes: {sum(r["kind"] == "edge" for r in routes.values())} edge, {sum(r["kind"] == "body" for r in routes.values())} body; '
                  f'{len(report["undecided"])} undecided; {len(report["lambdas"])} folded lambdas; {sum(r["hot"] for r in routes.values())} hot']
        need = [(f'`{short(k)}`', ', '.join(f'`{short(n)}`' for n in r['closure']['needs_bridge'])) for k, r in routes.items() if r['closure']['needs_bridge']]
        if need:
            lines += ['', 'Body routes whose vanilla callees native owns and no bridge covers:', ''] + table(['route', 'needs a bridge'], need)
        try:
            from . import java_routing
        except ImportError:
            import java_routing
        lines += ['', 'Gates (tools/java_routing.py):', ''] + java_routing.report_table(report)
    return '\n'.join(lines) + '\n'


def write_routing(out, report):
    try:
        from . import mod_routing as mrt
    except ImportError:
        import mod_routing as mrt
    out = Path(out)
    (out / 'routing.json').write_text(json.dumps(report, indent=1) + '\n')
    (out / 'routing.tsv').write_text(mrt.tsv(report))


def install_analysis(install, cache=None, log=print):
    """INSTALL/analysis/entity-facts.json for the install's mods, redone only when the jars or this analysis changed; exits when an entity method has no state group."""
    install = Path(install)
    out = install / 'analysis'
    sources = ''.join(hashlib.sha256((Path(__file__).resolve().parent / name).read_bytes()).hexdigest() for name in ('mod_replaces.py', 'mod_entity_facts.py', 'mod_routing.py', 'mod_parity.py'))
    ledger = json.loads(LEDGER_JSON.read_text()) if LEDGER_JSON.is_file() else {}
    verdicts = read_verdicts()
    sources += hashlib.sha256(json.dumps([LEDGER_JSON.stat().st_mtime_ns if LEDGER_JSON.is_file() else 0, sorted(verdicts.items())]).encode()).hexdigest()
    jars = [(j.name, j.stat().st_size, j.stat().st_mtime_ns) for j in sorted((install / 'mods').glob('*.jar'))]
    stamp = hashlib.sha256(json.dumps([jars, sources]).encode()).hexdigest()
    if (out / 'stamp').is_file() and (out / 'stamp').read_text() == stamp and (out / 'entity-facts.json').is_file() and (out / 'routing.json').is_file():
        return out / 'entity-facts.json'
    out.mkdir(parents=True, exist_ok=True)
    result = analyse(install, out, ledger, cache, log=log, verdicts=verdicts)
    facts = result['entity_facts']
    if facts['unclassified']:
        sys.exit('entity methods without a state group (add them to GROUPS in tools/mod_entity_facts.py):\n  ' + '\n  '.join(facts['unclassified']))
    write_routing(out, result['routing'])
    (out / 'entity-facts.json').write_text(json.dumps(facts, indent=1) + '\n')
    (out / 'stamp').write_text(stamp)
    return out / 'entity-facts.json'


def run(args):
    ledger_path = Path(args.ledger)
    if not ledger_path.is_file():
        sys.exit(f'no port ledger at {ledger_path}: run tools/port_ledger.py build')
    entries = json.loads(Path(args.mixins).read_text()) if args.mixins else None
    started = time.time()
    result = analyse(args.install, args.out, json.loads(ledger_path.read_text()), args.cache, entries, verdicts=read_verdicts())
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    facts = result.pop('entity_facts')
    report = result.pop('routing')
    write_routing(out, report)
    if getattr(args, 'routes_out', None):
        try:
            from . import mod_routing as mrt
        except ImportError:
            import mod_routing as mrt
        Path(args.routes_out).write_text(mrt.routes_tsv(report, Path(args.install).name))
    (out / 'entity-facts.json').write_text(json.dumps(facts, indent=1) + '\n')
    (out / 'replaces.json').write_text(json.dumps(result, indent=1) + '\n')
    (out / 'replaces.md').write_text(markdown(dict(result, routing=report)))
    print(f'{out / "replaces.json"}: {len(result["mods"])} mods, {len(result["methods"])} methods, {len(result["events"])} events in {time.time() - started:.1f}s')
    print(json.dumps(result['totals']))
    if facts['unclassified']:
        sys.exit('entity methods without a state group (add them to GROUPS in tools/mod_entity_facts.py):\n  ' + '\n  '.join(facts['unclassified']))


def add_arguments(parser):
    parser.add_argument('--install', required=True)
    parser.add_argument('--out', required=True)
    parser.add_argument('--ledger', default=str(LEDGER_JSON))
    parser.add_argument('--cache', default=str(DEFAULT_CACHE), help='the libraries\' class index, kept here')
    parser.add_argument('--routes-out', help='write the reference-pack list of routes (vanilla keys, kinds, mod ids) here')
    parser.add_argument('--mixins', help='a mixins.json of mod_parity.py to use instead of scanning')


def main():
    parser = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    add_arguments(parser)
    run(parser.parse_args())


if __name__ == '__main__':
    main()
