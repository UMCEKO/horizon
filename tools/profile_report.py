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


TREE_FIELDS = ("count", "incl_us", "excl_us", "max_us", "callbacks", "nested", "depth", "bytes_in", "bytes_out", "copied", "ser_us")


def parse_tree(text):
    """`tree=` rows (`name|` then TREE_FIELDS, comma separated, `-` for none) as {name: {field: int}}."""
    rows = {}
    for row in text.split(","):
        f = row.split("|")
        if len(f) != len(TREE_FIELDS) + 1:
            continue
        try:
            rows[f[0]] = dict(zip(TREE_FIELDS, map(int, f[1:])))
        except ValueError:
            continue
    return rows


def merge_tree(into, rows):
    for name, row in rows.items():
        have = into.setdefault(name, dict.fromkeys(TREE_FIELDS, 0))
        for key, value in row.items():
            have[key] = max(have[key], value) if key in ("max_us", "depth") else have[key] + value


def tree_lines(rows, top):
    """The call tree's nodes by exclusive time: a `native:` row is a Java to Rust callback, the rest Rust to Java requests."""
    return [
        f"{name[:40]:<40} {r['count']:>8} {r['incl_us'] / 1000:>9.1f} {r['excl_us'] / 1000:>9.1f} {r['ser_us'] / 1000:>7.1f} "
        f"{r['callbacks']:>8} {r['nested']:>7} {r['depth']:>5} {r['bytes_in'] / 1024:>9.1f} {r['bytes_out'] / 1024:>9.1f} {r['copied'] / 1024:>9.1f}"
        for name, r in sorted(rows.items(), key=lambda kv: -kv[1]["excl_us"])[:top]
    ]


TREE_HEADER = (f"{'node':<40} {'count':>8} {'incl ms':>9} {'excl ms':>9} {'ser ms':>7} {'callbks':>8} {'nested':>7} {'depth':>5} "
               f"{'in KiB':>9} {'out KiB':>9} {'copy KiB':>9}")


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
    be, calls, locks, holds, spans, events, tree, chains, posts = {}, {}, {}, {}, {}, {}, {}, {}, {}
    for j in java:
        merge(be, parse_rows(j.get("be", "-")))
        merge(calls, parse_rows(j.get("calls", "-")))
        merge(locks, parse_rows(j.get("lock", "-")))
        merge(holds, parse_rows(j.get("hold", "-")))
        merge(events, parse_rows(j.get("ev", "-")))
        merge(posts, parse_rows(j.get("posts", "-")))
        merge(chains, parse_rows(j.get("chains", "-")))
        merge_tree(tree, parse_tree(j.get("tree", "-")))
    dims = int(sum(num(j, "dims") for j in java))
    out.append(f"dimension names sent to Java: {dims}")
    if tree:
        requests = sum(r["count"] for n, r in tree.items() if not n.startswith("native:"))
        natives = sum(r["count"] for n, r in tree.items() if n.startswith("native:"))
        moved = sum(r["bytes_in"] + r["bytes_out"] for r in tree.values())
        copied = sum(r["copied"] for r in tree.values())
        scratch = int(max(num(j, "scratch_kb") for j in java))
        out.append(f"crossings: {requests} Rust to Java, {natives} Java to Rust; {moved / 1024:.1f} KiB on the wire, "
                   f"{copied / 1024:.1f} KiB copied; scratch high-water {scratch} KiB, kept after requests "
                   f"{int(max(num(j, 'scratch_kept_kb') for j in java))} KiB (top rows of each second only)")
    if java and "jvm_classes" in java[-1]:
        first, last = java[0], java[-1]
        span = lambda key: f"{int(num(first, key))} -> {int(num(last, key))}"
        out.append(f"JVM runtime (not bridge objects), first -> last second: classes {span('jvm_classes')}, "
                   f"metaspace {span('jvm_meta_kb')} KiB, class space {span('jvm_class_kb')} KiB, "
                   f"code cache {span('jvm_code_kb')} KiB, direct buffers {span('direct_kb')} KiB; "
                   f"largest Java reply buffer {int(max(num(j, 'reply_high_kb') for j in java))} KiB")
    crossings = {}
    for s in server:
        merge(spans, parse_rows(s.get("spans", "-")))
        for row in s.get("event_crossings", "-").split(","):
            name, _, n = row.partition("|")
            if n.isdigit():
                crossings[name] = crossings.get(name, 0) + int(n)
    out += table("Java block entity types (ticker time)", STAT_HEADER, stat_lines(be, top))
    out += table("Java events sent to mods (listener time: server/level ticks, player ticks...)", STAT_HEADER, stat_lines(events, top))
    out += table("Game bus posts by event class (compiled dispatch, listener time included)", STAT_HEADER, stat_lines(posts, top))
    out += table("Native hook crossings to Java by event class (none for a class no Java mod listens to)", f"{'event':<48} {'crossings':>9} {'per tick':>9}",
                 [f"{name[:48]:<48} {n:>9} {n / max(ticks, 1):>9.2f}" for name, n in sorted(crossings.items(), key=lambda kv: -kv[1])[:top]])
    out += table("Native spans (server tick)", STAT_HEADER, stat_lines(spans, top))
    out += table("Java calls from Rust, by kind (includes turn wait)", STAT_HEADER, stat_lines(calls, top))
    out += table("Server-side turn wait (another server-side caller, never the window), by call kind", STAT_HEADER, stat_lines(locks, top))
    out += table("Server-side turn held, by outermost call kind", STAT_HEADER, stat_lines(holds, top))
    out += table("Crossing call tree, by exclusive time (nested crossings not counted twice)", TREE_HEADER, tree_lines(tree, top))
    out += table("Dominant call chains (root>...>call, inclusive)", STAT_HEADER, stat_lines(chains, top))
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
