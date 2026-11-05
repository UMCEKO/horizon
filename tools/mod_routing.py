#!/usr/bin/env python3
"""How each vanilla method the mods' mixins change is routed to Java: edge or body, receivers, callees (format in mod_replaces.py)."""
import collections
import struct
import zipfile
from pathlib import Path

try:
    from . import mod_replaces as mr, mod_entity_facts as ef
except ImportError:
    import mod_replaces as mr
    import mod_entity_facts as ef

EDGE_AT = ('HEAD', 'RETURN', 'TAIL')
CLOSURE_CAP = 2000
HOT_DEPTH = 3
HOT_ROOTS = (
    'net/minecraft/world/entity/Entity#tick()V',
    'net/minecraft/world/entity/Entity#baseTick()V',
    'net/minecraft/world/entity/LivingEntity#aiStep()V',
    'net/minecraft/world/entity/Entity#move(Lnet/minecraft/world/entity/MoverType;Lnet/minecraft/world/phys/Vec3;)V',
    'net/minecraft/server/level/ServerLevel#tick(Ljava/util/function/BooleanSupplier;)V',
)
VIEW_ROOTS = ('net/minecraft/world/entity/Entity', 'net/minecraft/world/level/Level', 'net/minecraft/world/level/block/entity/BlockEntity',
              'net/minecraft/world/entity/player/Inventory', 'net/minecraft/world/scores/Scoreboard', 'net/minecraft/server/MinecraftServer',
              'net/minecraft/server/players/PlayerList', 'net/minecraft/server/level/ServerPlayer')
FAMILIES = (
    ('player', ('net/minecraft/world/entity/player/Player', 'net/minecraft/world/entity/player/Inventory')),
    ('entity', ('net/minecraft/world/entity/Entity',)),
    ('block_entity', ('net/minecraft/world/level/block/entity/BlockEntity', 'net/minecraft/world/RandomizableContainer')),
    ('block', ('net/minecraft/world/level/block/Block', 'net/minecraft/world/level/block/state/BlockBehaviour$BlockStateBase')),
    ('item', ('net/minecraft/world/item/Item',)),
    ('level', ('net/minecraft/world/level/Level',)),
    ('scoreboard', ('net/minecraft/world/scores/Scoreboard',)),
    ('menu', ('net/minecraft/world/inventory/AbstractContainerMenu',)),
)
LAMBDA_FACTORY = 'java/lang/invoke/LambdaMetafactory'
RETURNS = range(0xac, 0xb2)
INVOKESPECIAL, INVOKESTATIC, INVOKEVIRTUAL = 0xb7, 0xb8, 0xb6


class Vanilla:
    """The vanilla classes of the library jars, parsed with their code on first use."""

    def __init__(self, jars):
        self.index, self.cache, self.methods, self.code = {}, {}, {}, {}
        for jar in ([jars] if isinstance(jars, (str, Path)) else jars):
            z = zipfile.ZipFile(jar)
            for entry in z.namelist():
                if entry.endswith('.class') and mr.is_vanilla(entry[:-6]):
                    self.index.setdefault(entry[:-6], (z, entry))

    def info(self, name):
        if name not in self.cache:
            ref = self.index.get(name)
            try:
                self.cache[name] = mr.parse_class(ref[0].read(ref[1]), annotations=True, code=True) if ref else None
            except (ValueError, IndexError, struct.error):
                self.cache[name] = None
        return self.cache[name]

    def method(self, cls, name, desc):
        if cls not in self.methods:
            info = self.info(cls)
            self.methods[cls] = {(m['name'], m['desc']): m for m in info['methods']} if info else {}
        return self.methods[cls].get((name, desc))

    def ops(self, cls, name, desc):
        """([(opcode, owner, name, descriptor)] of the calls, number of returns) of a method; a lambda call site is a call to its implementation."""
        key = (cls, name, desc)
        if key not in self.code:
            m, calls, returns = self.method(cls, name, desc), [], 0
            if m and 'code' in m:
                info = self.info(cls)
                pool, code, p = info['pool'], m['code'], 0
                while p < len(code):
                    op = code[p]
                    if 0xb6 <= op <= 0xb9:
                        calls.append((op, *pool.ref(mr.u2(code, p + 1))))
                    elif op == 0xba:
                        impl = lambda_target(info, pool.e[mr.u2(code, p + 1)][1])
                        if impl:
                            calls.append((INVOKESTATIC if impl[3] else INVOKEVIRTUAL, *impl[:3]))
                    elif op in RETURNS:
                        returns += 1
                    p += mr.op_length(code, p)
            self.code[key] = (calls, returns)
        return self.code[key]


def lambda_target(info, bootstrap):
    """(class, name, descriptor, static) of the method a LambdaMetafactory call site runs, else None."""
    if bootstrap >= len(info['bootstraps']):
        return None
    pool = info['pool']
    handle = pool.e[info['bootstraps'][bootstrap][0]]
    if not handle or handle[0] != 15 or pool.ref(handle[2])[0] != LAMBDA_FACTORY:
        return None
    return mr.lambda_impl(info, bootstrap)


def enclosing(vanilla, cls, name, desc=None):
    """The (name, descriptor) of the non-lambda method whose code holds the call site of `cls.name`, through nested lambdas; None when none does."""
    info, seen, found = vanilla.info(cls), set(), None
    while name.startswith('lambda$') and (name, desc) not in seen and info:
        seen.add((name, desc))
        hit = None
        for m in info['methods']:
            for ev in m['events']:
                impl = lambda_target(info, ev[1]) if ev[0] == 'indy' else None
                if impl and impl[0] == cls and impl[1] == name and desc in (None, impl[2]):
                    hit = (m['name'], m['desc'])
                    break
            if hit:
                break
        if not hit:
            return None
        found = hit
        name, desc = hit
    return found


def fold_lambdas(records, hier, vanilla):
    """The (key, vanilla, record) list with each change to a vanilla `lambda$` method moved to its enclosing method; the change gets `folded`."""
    out = []
    for key, is_vanilla, record in records:
        owner, _, name = key.partition('#')
        if is_vanilla and name.startswith('lambda$'):
            lam_desc = record['desc'] or next((m[1] for m in hier.methods_of(owner) if m[0] == name), None)
            hit = enclosing(vanilla, owner, name, lam_desc)
            if hit:
                record = dict(record, desc=hit[1], folded=f'{name}{lam_desc or ""}')
                key = f'{owner}#{hit[0]}'
        out.append((key, is_vanilla, record))
    return out


def desc_parts(desc):
    """(parameter descriptors, return descriptor)."""
    out, i = [], 1
    while desc[i] != ')':
        start = i
        while desc[i] == '[':
            i += 1
        i = desc.index(';', i) + 1 if desc[i] == 'L' else i + 1
        out.append(desc[start:i])
    return out, desc[i + 1:]


def bridge_table():
    """{(owner, name, descriptor)} of the `java_bridges!` entries, read with the regex `java_mods.py check` uses."""
    try:
        from . import java_mods as jm
    except ImportError:
        import java_mods as jm
    return {(owner, name, desc) for path in sorted(jm.BRIDGES_DIR.glob('*.rs')) for owner, name, desc, *_ in jm.BRIDGE_ENTRY.findall(path.read_text())}


class Graph:
    """The vanilla call graph, each call resolved to the class that declares the method."""

    def __init__(self, vanilla, hier):
        self.vanilla, self.hier, self.resolved = vanilla, hier, {}

    def resolve(self, owner, name, desc):
        k = (owner, name, desc)
        if k not in self.resolved:
            decl = self.hier.declarer(owner, name, desc) if mr.is_vanilla(owner) else None
            self.resolved[k] = (decl, name, desc) if decl and mr.is_vanilla(decl) else None
        return self.resolved[k]

    def callees(self, k):
        return [r for r in (self.resolve(o, n, d) for _, o, n, d in self.vanilla.ops(*k)[0]) if r]


def key_of(k):
    return f'{k[0]}#{k[1]}{k[2]}'


def split_key(key):
    cls, _, rest = key.partition('#')
    name, paren, desc = rest.partition('(')
    return cls, name, (paren + desc) if paren else None


def hot_reach(graph):
    """(methods, classes) within HOT_DEPTH calls of the hot roots."""
    frontier = [split_key(r) for r in HOT_ROOTS]
    seen = set(frontier)
    for _ in range(HOT_DEPTH):
        nxt = []
        for k in frontier:
            for c in graph.callees(k):
                if c not in seen:
                    seen.add(c)
                    nxt.append(c)
        frontier = nxt
    return seen, {k[0] for k in seen}


def subclass_index(hier):
    children = collections.defaultdict(list)
    for name, (sup, ifaces) in hier.heads.items():
        if mr.is_vanilla(name):
            for parent in ([sup] if sup else []) + list(ifaces):
                children[parent].append(name)
    return children


def overriders(graph, children, owner, name, desc):
    """[{class, super_call}] of the vanilla subclasses of `owner` that declare name+desc."""
    hier, vanilla, memo = graph.hier, graph.vanilla, {}

    def declares(cls):
        return any(m[0] == name and m[1] == desc and not m[2] & (mr.ACC_STATIC | mr.ACC_PRIVATE) for m in hier.methods_of(cls))

    def super_calls(cls):
        if cls not in memo:
            memo[cls] = False
            for op, o, n, d in vanilla.ops(cls, name, desc)[0]:
                if op != INVOKESPECIAL or n != name or d != desc:
                    continue
                decl = hier.declarer(o, name, desc)
                if decl == owner or (decl != cls and hier.is_a(decl, owner) and declares(decl) and super_calls(decl)):
                    memo[cls] = True
                    break
        return memo[cls]

    found, queue, seen = [], list(children.get(owner, ())), set()
    while queue:
        cls = queue.pop()
        if cls in seen:
            continue
        seen.add(cls)
        queue += children.get(cls, ())
        if declares(cls):
            found.append({'class': cls, 'super_call': super_calls(cls)})
    return sorted(found, key=lambda o: o['class'])


def family(hier, owner, static):
    if owner.startswith('net/minecraft/world/level/levelgen/') or hier.is_a(owner, 'net/minecraft/server/level/ChunkMap'):
        return 'worldgen'
    if owner.startswith('net/minecraft/client/'):
        return 'client'
    if static:
        return 'static'
    return next((name for name, roots in FAMILIES if any(hier.is_a(owner, r) for r in roots)), 'value')


def closure(graph, root, routed, bridges, native):
    """The vanilla callees of a body route, walked breadth first and stopped at routed, bridged and view methods native owns."""
    seen, queue, walked = {root}, collections.deque([root]), 0
    out = {'bridged': set(), 'pure': set(), 'routed': set(), 'needs_bridge': set()}
    truncated = False
    while queue and not truncated:
        for c in graph.callees(queue.popleft()):
            if c in seen:
                continue
            seen.add(c)
            key = key_of(c)
            if key in routed:
                out['routed'].add(key)
            elif c in bridges:
                out['bridged'].add(key)
            elif any(graph.hier.is_a(c[0], r) for r in VIEW_ROOTS) and native.info(f'{c[0]}#{c[1]}')['owned']:
                out['needs_bridge'].add(key)
            else:
                walked += 1
                if walked > CLOSURE_CAP:
                    truncated = True
                    break
                out['pure'].add(key)
                queue.append(c)
    result = {k: sorted(v) for k, v in out.items()}
    if truncated:
        result['truncated'] = True
    return result


def handlers(archives, cache, change):
    mixin, _, handler = change['handler'].rpartition('.')
    k = (change['mod'], mixin, handler)
    if k not in cache:
        cache[k] = ef.handler_method(archives, change['mod'], mixin, handler)
    return mixin, handler, cache[k]


def callback_split(desc):
    """(target parameters, has captured locals) of a handler descriptor with a CallbackInfo, else None."""
    hp = mr.params(desc)
    for i, p in enumerate(hp):
        if p in ef.CALLBACKS:
            return hp[:i], len(hp) > i + 1
    return None


def annotation(method, suffix):
    return next((v for d, v in method['annos'].items() if d.endswith(suffix)), None)


def change_row(archives, cache, change, target):
    """(the change's fields in routing.json, whether it is an edge change) for one target descriptor."""
    mixin, handler, found = handlers(archives, cache, change)
    kind, at = change['kind'], change['at']
    fits = [f for f in found if (s := callback_split(f['desc'])) and target and s[0] in (mr.params(target), [])]
    method = (fits or found or [None])[0]
    inject = annotation(method, '/Inject;') if method and kind == 'inject' else None
    locals_ = bool(fits and kind == 'inject' and callback_split(fits[0]['desc'])[1])
    if change.get('folded'):
        edge = False
    elif kind == 'inject':
        edge = bool(at) and all(a in EDGE_AT for a in at) and bool(fits) and not locals_
    elif kind == 'modify_return_value':
        edge = all(a.split('(')[0] == 'RETURN' for a in at)
    elif kind == 'wrap_method':
        edge = True
    elif kind == 'modify_variable':
        edge = at == ['HEAD'] and bool(method and (annotation(method, '/ModifyVariable;') or {}).get('argsOnly'))
    else:
        edge = False
    row = {'mod': change['mod'], 'mixin': mixin, 'handler': handler, 'kind': kind, 'at': at,
           'cancellable': bool((inject or {}).get('cancellable')), 'locals': locals_, 'conditional': bool(change.get('conditional'))}
    if change.get('folded'):
        row['lambda'] = change['folded']
    return row, edge


def targets_of(hier, archives, cache, owner, name, change):
    """The descriptors a change stands for."""
    found = handlers(archives, cache, change)[2] if change['kind'] == 'inject' else []
    split = callback_split(found[0]['desc']) if len(found) == 1 else None
    return ef.library_target(hier, owner, name, change['desc'], split[0] if split else None)


def routing(result, archives, hier, ledger, verdicts, vanilla_jar, bridges=None, vanilla=None):
    """The routing report for the mixin changes of `mod_replaces.analyse`'s result; `vanilla_jar` is the jar or jars holding the vanilla code."""
    vanilla = vanilla or Vanilla(vanilla_jar)
    graph, native = Graph(vanilla, hier), mr.Native(ledger)
    bridges = bridge_table() if bridges is None else bridges
    cache, grouped, undecided, lambdas, status_of = {}, collections.defaultdict(list), {}, {}, {}
    for key, row in result['methods'].items():
        owner, _, name = key.partition('#')
        for change in row['changes']:
            if not row['vanilla'] or change['mechanism'] != 'mixin' or change['routing'] not in ('java', 'undecided') or name == '<unknown>':
                continue
            for desc in targets_of(hier, archives, cache, owner, name, change):
                full = f'{owner}#{name}{desc or ""}'
                if change.get('folded'):
                    lambdas[f'{owner}#{change["folded"]}'] = full
                if change['routing'] == 'undecided':
                    entry = undecided.setdefault(full, {'status': row['native']['status'], 'mods': []})
                    entry['mods'] = sorted(set(entry['mods']) | {change['mod']})
                else:
                    grouped[full].append((change, desc))
                    status_of[full] = row['native']['status']
    children, (reach, reach_owners), routes = subclass_index(hier), hot_reach(graph), {}
    for full, items in grouped.items():
        owner, name, desc = split_key(full)
        rows = [change_row(archives, cache, change, desc) for change, _ in items]
        kind = 'edge' if all(edge for _, edge in rows) else 'body'
        changes = [r for r, _ in rows]
        sides = {c['side'] for c, _ in items}
        static = bool(next((m[2] for m in hier.methods_of(owner) if m[0] == name and m[1] == desc), 0) & mr.ACC_STATIC)
        args, ret = desc_parts(desc) if desc else ([], 'V')
        k = (owner, name, desc)
        routes[full] = {
            'kind': kind, 'static': static, 'family': family(hier, owner, static), 'args': args, 'ret': ret, 'changes': changes,
            'returns': vanilla.ops(*k)[1], 'tail': any('TAIL' in c['at'] for c in changes),
            'overriders': overriders(graph, children, owner, name, desc) if desc else [],
            'closure': {'bridged': [], 'pure': [], 'routed': [], 'needs_bridge': []},
            'hot': k in reach or (kind == 'edge' and owner in reach_owners),
            'side': sides.pop() if len(sides) == 1 and sides <= {'client', 'server'} else 'common'}
        verdict = mr.verdict_of(verdicts, full)
        if verdict and status_of[full] in mr.WEAK_STATUSES:
            routes[full]['verdict'] = verdict
    for full, route in routes.items():
        if route['kind'] == 'body' and split_key(full)[2]:
            route['closure'] = closure(graph, split_key(full), set(routes), bridges, native)
    return {'format': 1, 'routes': dict(sorted(routes.items())), 'undecided': dict(sorted(undecided.items())), 'lambdas': dict(sorted(lambdas.items()))}


def tsv(report):
    """The class processor's list: owner, name, descriptor, kind, tail (0/1), returns."""
    lines = []
    for key, r in report['routes'].items():
        owner, name, desc = split_key(key)
        lines.append('\t'.join([owner, name, desc or '', r['kind'], str(int(r['tail'])), str(r['returns'])]))
    return '\n'.join(lines) + ('\n' if lines else '')


def routes_tsv(report, label):
    """The reference-pack list: one line per route, no handler or mod class names."""
    lines = []
    for key, r in report['routes'].items():
        mods = sorted({c['mod'] for c in r['changes']})
        lines.append('\t'.join([key, r['kind'], r['family'], str(int(r['tail'])), str(r['returns']), ','.join(mods), str(int(all(c['conditional'] for c in r['changes'])))]))
    return f'# routes of {label}, mod_routing format 1\n' + ''.join(line + '\n' for line in sorted(lines))


def summary(report):
    """Counts: routes by kind and family, undecided keys, folded lambdas, hot routes, the keys whose closure needs a bridge."""
    routes = report['routes']
    return {'routes': len(routes), 'by_kind': dict(collections.Counter(r['kind'] for r in routes.values())),
            'by_family': dict(sorted(collections.Counter(f'{r["kind"]}/{r["family"]}' for r in routes.values()).items())),
            'undecided_keys': len(report['undecided']), 'lambdas': len(report['lambdas']), 'hot': sorted(k for k, r in routes.items() if r['hot']),
            'needs_bridge': {k: r['closure']['needs_bridge'] for k, r in routes.items() if r['closure']['needs_bridge']}}
