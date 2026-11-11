#!/usr/bin/env python3
"""Dev tool: what only Horizon's tree knows, as the boot jar's analysis/ resources (native.tsv, verdicts.tsv, host_words.txt)."""
import argparse
import json
import sys
from pathlib import Path

try:
    from . import mod_replaces as mr
except ImportError:
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import mod_replaces as mr  # noqa: E402


def escape(text):
    return str(text).replace('\\', '\\\\').replace('\t', '\\t').replace('\n', '\\n')


def native_rows(ledger):
    """[key, status, owned, fns, note] for every key of the status tables and of `methods`, sorted by key."""
    native = mr.Native(ledger)
    keys = set(native.methods)
    for kind in mr.STATUS_ORDER:
        keys.update(ledger.get(kind, {}))
    rows = []
    for key in sorted(keys):
        info = native.info(key)
        rows.append([key, info['status'], '1' if info['owned'] else '0', ','.join(info['fns']), info['note']])
    return rows


def write(out_dir, ledger_path=None):
    """Writes native.tsv, verdicts.tsv and host_words.txt into out_dir."""
    ledger_path = Path(ledger_path or mr.LEDGER_JSON)
    if not ledger_path.is_file():
        sys.exit(mr.NO_LEDGER)
    ledger = json.loads(ledger_path.read_text())
    out = Path(out_dir)
    out.mkdir(parents=True, exist_ok=True)
    native = ''.join('\t'.join(escape(f) for f in row) + '\n' for row in native_rows(ledger))
    verdicts = ''.join(f'{escape(key)}\t{escape(verdict)}\n' for key, verdict in sorted(mr.read_verdicts().items()))
    words = ''.join(f'{word}\n' for word in sorted(mr.host_words()))
    for name, text in (('native.tsv', native), ('verdicts.tsv', verdicts), ('host_words.txt', words)):
        with open(out / name, 'w', encoding='utf-8', newline='\n') as f:
            f.write(text)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--out', required=True)
    parser.add_argument('--ledger', default=str(mr.LEDGER_JSON))
    args = parser.parse_args()
    write(args.out, args.ledger)


if __name__ == '__main__':
    main()
