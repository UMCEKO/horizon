#!/usr/bin/env python3
"""Ranks what a HORIZON_PROFILE folder recorded: usage `profile_report.py <dir> [--top N] [--no-jfr]`."""
import argparse
import glob
import os
import shutil
import subprocess
import sys


def parse_rows(text):
    """`name|count|total_us|max_us` rows (comma separated, `-` for none) as {name: (count, total_us, max_us)}."""
    rows = {}
    for row in text.split(","):
        f = row.split("|")
        if len(f) != 4:
            continue
        try:
            rows[f[0]] = (int(f[1]), int(f[2]), int(f[3]))
        except ValueError:
            continue
    return rows


def parse_line(line):
    """`key=value` fields of a log line, split on spaces; None when it has no time."""
    fields = dict(p.split("=", 1) for p in line.split() if "=" in p)
    return fields if "t" in fields else None


def read_log(path):
    if not os.path.exists(path):
        return []
    with open(path, errors="replace") as f:
        return [x for x in map(parse_line, f) if x]


def merge(into, rows):
    for name, (count, total, peak) in rows.items():
        c, t, m = into.get(name, (0, 0, 0))
        into[name] = (c + count, t + total, max(m, peak))


def ranked(rows, top):
    return sorted(rows.items(), key=lambda kv: -kv[1][1])[:top]


def table(title, header, lines):
    out = ["", f"== {title} =="]
    if not lines:
        return out + ["(none)"]
    out.append(header)
    return out + lines


def stat_lines(rows, top, per="count"):
    total = sum(v[1] for v in rows.values()) or 1
    return [
        f"{name[:48]:<48} {t / 1000:>10.1f} {100 * t / total:>5.1f}% {c:>9} {t / max(c, 1):>9.0f} {m:>9}"
        for name, (c, t, m) in ranked(rows, top)
    ]


STAT_HEADER = f"{'name':<48} {'total ms':>10} {'share':>6} {'count':>9} {'mean us':>9} {'max us':>9}"


def num(fields, key):
    try:
        return float(fields.get(key, 0))
    except ValueError:
        return 0.0


def nearest(java, t):
    best = min(java, key=lambda j: abs(num(j, "t") - t), default=None)
    return best if best is not None and abs(num(best, "t") - t) <= 1500 else None


def top_name(text):
    rows = ranked(parse_rows(text), 1)
    return rows[0][0] if rows else "-"


def report(folder, top=15, jfr=True):
    server = read_log(os.path.join(folder, "server-ticks.log"))
    java = read_log(os.path.join(folder, "java-ticks.log"))
    out = [f"profile: {folder}"]
    if server:
        ticks = sum(num(s, "ticks") for s in server)
        mean = sum(num(s, "mean_ms") * num(s, "ticks") for s in server) / max(ticks, 1)
        p99s = sorted(num(s, "p99_ms") for s in server)
        out.append(
            f"{len(server)} s of server ticks, {int(ticks)} ticks, mean {mean:.2f} ms, "
            f"worst p99 second {p99s[-1]:.1f} ms, max {max(num(s, 'max_ms') for s in server):.1f} ms, "
            f"players up to {int(max(num(s, 'players') for s in server))}, "
            f"Java share of the tick {sum(num(s, 'java_ms') for s in server) / max(sum(num(s, 'mean_ms') * num(s, 'ticks') for s in server), 1e-9) * 100:.1f}%"
        )
    else:
        out.append("no server-ticks.log")
    be, calls, locks, spans, events = {}, {}, {}, {}, {}
    for j in java:
        merge(be, parse_rows(j.get("be", "-")))
        merge(calls, parse_rows(j.get("calls", "-")))
        merge(locks, parse_rows(j.get("lock", "-")))
        merge(events, parse_rows(j.get("ev", "-")))
    for s in server:
        merge(spans, parse_rows(s.get("spans", "-")))
    out += table("Java block entity types (ticker time)", STAT_HEADER, stat_lines(be, top))
    out += table("Java events sent to mods (listener time: server/level ticks, player ticks...)", STAT_HEADER, stat_lines(events, top))
    out += table("Native spans (server tick)", STAT_HEADER, stat_lines(spans, top))
    out += table("Java calls from Rust, by kind (includes lock wait)", STAT_HEADER, stat_lines(calls, top))
    out += table("GameBridge lock wait, by call kind", STAT_HEADER, stat_lines(locks, top))
    worst = sorted(server, key=lambda s: -num(s, "max_ms"))[:top]
    lines = []
    for s in worst:
        j = nearest(java, num(s, "t"))
        lines.append(
            f"{int(num(s, 't'))} max {num(s, 'max_ms'):>6.1f} ms p99 {num(s, 'p99_ms'):>6.1f} "
            f"players {int(num(s, 'players')):>3} chunks {int(num(s, 'chunks')):>5} mobs {int(num(s, 'mobs')):>4} "
            f"java {num(s, 'java_pct'):>4.0f}% span {top_name(s.get('spans', '-'))}"
            + (f" be {top_name(j.get('be', '-'))} lock {top_name(j.get('lock', '-'))}" if j else "")
        )
    out += table("Worst seconds", "t(unix ms)", lines)
    if jfr:
        out += jfr_lines(folder, top)
    return "\n".join(out)


def jfr_lines(folder, top):
    files = [f for f in glob.glob(os.path.join(folder, "*.jfr")) if os.path.getsize(f) > 0]
    files = sorted(files, key=os.path.getmtime)[-1:]
    tool = shutil.which("jfr")
    out = []
    for path in files:
        out += ["", f"== JFR {os.path.basename(path)} =="]
        if not tool:
            out.append("(the jfr tool is not on PATH)")
            continue
        view = subprocess.run([tool, "view", "--width", "200", "hot-methods", path], capture_output=True, text=True)
        if view.returncode == 0 and view.stdout.strip():
            out += view.stdout.strip().splitlines()[: top + 6]
        else:
            summary = subprocess.run([tool, "summary", path], capture_output=True, text=True)
            out += (summary.stdout or summary.stderr).strip().splitlines()[: top + 6]
    return out


def main():
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("dir")
    ap.add_argument("--top", type=int, default=15)
    ap.add_argument("--no-jfr", action="store_true")
    args = ap.parse_args()
    if not os.path.isdir(args.dir):
        sys.exit(f"{args.dir}: not a folder")
    print(report(args.dir, args.top, not args.no_jfr))


if __name__ == "__main__":
    main()
