#!/usr/bin/env python3
"""Compact summary of a Horizon game log: phases, errors, warnings, noise, tail."""
import argparse
import re
import sys
from collections import Counter

CUT = 300
SKIP_FRAMES = ('java.', 'jdk.', 'sun.', 'org.spongepowered', 'cpw.', 'net.neoforged.fml.loading',
               'std::', 'core::', 'rust_', '__')
SHOW_FRAMES = 3

# (name, regex, take the last match instead of the first)
PHASES = [
    ('java boot', r'Starting FancyModLoader', False),
    ('mods loaded', r'Java mods ready in', False),
    ('resource reload', r'Reloading ResourceManager', False),
    ('first frame', r'presenting the first frame', False),
    ('joined', r'logged in with entity id|joined the game', False),
    ('exit', r'JVM is shutting down|Stopping (the )?server|Stopping!', True),
]

# Harmless noise: (label, regex), matched against an entry's text
KNOWN = [
    ('narrator', r'loading the narrator|libflite|Failed to load library flite'),
    ('offline mode', r'OFFLINE/INSECURE MODE|make no attempt to authenticate|opens up the ability for hackers'
                     r'|"online-mode" to "true"'),
    ('jade', r'Start loading plugin'),
]

TS = re.compile(r'^\s*\[(\d\d:\d\d:\d\d)\]')
LOG_START = re.compile(r'^\s*\[\d\d:\d\d:\d\d\]|^\d{4}-\d\d-\d\dT|^Horizon')
ERR = re.compile(r'/(?:ERROR|FATAL)\]|^\S+ \S+ (?:ERROR|FATAL) |\bpanicked at\b')
WARN = re.compile(r'/WARN\]|^\S+ \S+ WARN |^WARNING:')
EXC = re.compile(r'^\s*(?:Caused by: |Suppressed: |Exception in thread "[^"]*" )?[\w$.]*(?:Exception|Error|Throwable)(?::|$)')
JAVA_FRAME = re.compile(r'^\s+at\s+(\S+\()')
PY_FRAME = re.compile(r'^\s+File "([^"]+)", line (\d+), in (\S+)')
RUST_FRAME = re.compile(r'^\s+\d+:\s+(\S.*)$')
CONT = re.compile(r'^\s|^\.\.\.|^Caused by:|^Suppressed:')


def cut(s, n=CUT):
    s = s.replace('\t', ' ').rstrip('\r\n')
    return s if len(s) <= n else f'{s[:n]}…(+{len(s) - n} chars)'


def norm(s):
    s = TS.sub('', s).strip()
    s = re.sub(r'^\d{4}-\d\d-\d\dT\S+ ', '', s)
    s = re.sub(r'\b[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}\b', 'U', s)
    s = re.sub(r'0x[0-9a-fA-F]+|@[0-9a-f]{5,}', 'H', s)
    return re.sub(r'-?\d+(?:\.\d+)?', 'N', s)


def frame_name(line):
    m = JAVA_FRAME.match(line)
    if m:
        return m.group(1)[:-1].split('/')[-1]
    m = PY_FRAME.match(line)
    if m:
        return f'{m.group(1).rsplit("/", 1)[-1]}:{m.group(2)} {m.group(3)}'
    return None


class Entry:
    def __init__(self, lineno, head, ctx=None, exc=None):
        self.lineno, self.head, self.ctx, self.exc = lineno, head, ctx, exc
        self.frames = []
        self.count = 1
        self.expect_msg = True

    def text(self):
        return ' '.join(x for x in (self.ctx, self.head, self.exc) if x)

    def key(self):
        return norm(self.text())

    def shown(self):
        keep = [f for f in self.frames if not f.startswith(SKIP_FRAMES)][:SHOW_FRAMES]
        return keep, len(self.frames) - len(keep)


def group_key(line):
    label = known(line)
    return f'known:{label}' if label else norm(line)


def known(text):
    for label, rx in KNOWN:
        if re.search(rx, text):
            return label
    return None


def scan(lines):
    phases, last_time = {}, '--:--:--'
    errors, warns, noise = {}, {}, Counter()
    first_noise = {}
    cur, prev = None, None
    for no, line in enumerate(lines, 1):
        m = TS.match(line)
        if m:
            last_time = m.group(1)
        for name, rx, last in PHASES:
            if re.search(rx, line) and (last or name not in phases):
                phases[name] = (last_time, no, line)
        plain = line.rstrip('\r\n')
        name = frame_name(plain)
        if name is None and cur is not None and RUST_FRAME.match(plain):
            name = RUST_FRAME.match(plain).group(1)
        if name is not None:
            if cur is not None:
                cur.frames.append(name)
                cur.expect_msg = False
        elif EXC.match(plain):
            if re.match(r'\s*(Caused by|Suppressed):', plain) and cur is not None:
                cur.expect_msg = True
            elif cur is not None and cur.exc is None and not cur.frames and cur.expect_msg:
                cur.exc = plain.strip()
            else:
                ctx = prev.strip() if prev and prev.startswith('Horizon') and not ERR.search(prev) else None
                cur = register(errors, Entry(no, plain.strip(), ctx))
        elif ERR.search(plain):
            cur = register(errors, Entry(no, plain.strip()))
        elif cur is not None and (CONT.match(plain) or cur.expect_msg) and plain.strip() and not LOG_START.match(plain):
            pass
        else:
            cur = None
            if not plain.strip():
                pass
            elif WARN.search(plain):
                k = group_key(plain)
                w = warns.setdefault(k, [0, plain.strip()])
                w[0] += 1
            else:
                k = group_key(plain)
                noise[k] += 1
                first_noise.setdefault(k, plain.strip())
        prev = plain
    return phases, errors, warns, noise, first_noise


def register(errors, e):
    first = errors.setdefault(e.key(), e)
    if first is not e:
        first.count += 1
        e.frames = []  # a repeat's frames are dropped; only the first is shown
    return e


def fmt_known(text):
    label = known(text)
    return f' [known: {label}]' if label else ''


def build(lines, max_lines):
    phases, errors, warns, noise, first_noise = scan(lines)
    out_phases = ['== Phases ==']
    for name, (t, no, line) in sorted(phases.items(), key=lambda kv: kv[1][1]):
        out_phases.append(cut(f'{t} L{no} {name}: {line.strip()}'))
    for name, _, _ in PHASES:
        if name not in phases:
            out_phases.append(f'--:--:-- {name}: not reached')

    err_lines = [f'== Errors ({len(errors)} distinct, {sum(e.count for e in errors.values())}) ==']
    ordered = sorted(errors.values(), key=lambda e: (known(e.text()) is not None, e.lineno))
    for e in ordered:
        keep, extra = e.shown()
        err_lines.append(cut(f'x{e.count} L{e.lineno}{fmt_known(e.text())} {e.ctx + " | " if e.ctx else ""}{e.head}'))
        if e.exc:
            err_lines.append(cut('    ' + e.exc))
        for f in keep:
            err_lines.append(cut('    at ' + f))
        if extra:
            err_lines.append(f'    (+{extra} frames)')

    warn_rows = sorted(warns.items(), key=lambda kv: -kv[1][0])
    warn_lines = [f'== Warnings ({len(warn_rows)} distinct, {sum(c for c, _ in warns.values())}) ==']
    for k, (c, text) in warn_rows[:15]:
        warn_lines.append(cut(f'{c:>6} x {norm(text)}{fmt_known(text)}'))

    noise_rows = [(k, c) for k, c in noise.most_common(10) if c > 1]
    noise_lines = ['== Repeated noise ==']
    for k, c in noise_rows:
        noise_lines.append(cut(f'{c:>6} x {norm(first_noise[k])}{fmt_known(first_noise[k])}'))

    tail = ['== Last 10 lines =='] + [cut(x) for x in lines[-10:]]
    return fit(max_lines, out_phases, [err_lines, warn_lines, noise_lines], tail)


def fit(max_lines, head, mids, tail):
    avail = max_lines - len(head) - len(tail)
    alloc = {}
    left = sorted(range(len(mids)), key=lambda i: len(mids[i]))
    for n, i in enumerate(left):
        share = avail // (len(left) - n)
        alloc[i] = min(len(mids[i]), max(share, 1))
        avail -= alloc[i]
    out = list(head)
    for i, sec in enumerate(mids):
        n = alloc[i]
        if len(sec) > n and n >= 2:
            out += sec[:n - 1] + [f'  …(+{len(sec) - n + 1} more lines, raise --max)']
        else:
            out += sec[:n]
    out += tail
    return out[:max_lines] if len(out) > max_lines else out


def grep(lines, rx, max_lines):
    pat = re.compile(rx)
    seen, order = {}, []
    hits = 0
    for no, line in enumerate(lines, 1):
        if pat.search(line):
            hits += 1
            k = norm(line)
            if k in seen:
                seen[k][0] += 1
            else:
                seen[k] = [1, no, line]
                order.append(k)
    out = [f'== grep {rx!r}: {hits} lines, {len(order)} distinct ==']
    for k in order:
        c, no, line = seen[k]
        out.append(cut(f'L{no} x{c} {line.strip()}' if c > 1 else f'L{no} {line.strip()}'))
    if len(out) > max_lines:
        extra = len(out) - max_lines + 1
        out = out[:max_lines - 1] + [f'…(+{extra} more distinct lines, raise --max)']
    return out


def summarize(text, max_lines=80, since=None, grep_rx=None):
    lines = text.split('\n')
    if lines and lines[-1] == '':
        lines.pop()
    if since:
        pat = re.compile(since)
        idx = next((i for i, x in enumerate(lines) if pat.search(x)), None)
        if idx is None:
            return [f'--since {since!r}: no line matches']
        lines = lines[idx:]
    if grep_rx:
        return grep(lines, grep_rx, max_lines)
    return build(lines, max_lines)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument('log')
    ap.add_argument('--max', type=int, default=80)
    ap.add_argument('--since', metavar='PATTERN')
    ap.add_argument('--grep', metavar='RE')
    a = ap.parse_args(argv)
    with open(a.log, encoding='utf-8', errors='replace') as f:
        text = f.read()
    print('\n'.join(summarize(text, a.max, a.since, a.grep)))
    return 0


if __name__ == '__main__':
    sys.exit(main())
