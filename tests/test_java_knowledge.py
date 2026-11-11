import json
import tempfile
import unittest
from pathlib import Path

from tools import java_knowledge as jk


class WriteTests(unittest.TestCase):
    def test_rows_escape_text_and_methods_without_a_status_are_none(self):
        ledger = {'fns': [{'file': 'crates/a.rs', 'line': 7}], 'methods': {'p/B#other': {}},
                  'ported': {'p/B#tick': {'fn': 0}}, 'absent': {'p/B#must': {'note': 'a\tb\nc\\d'}}}
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / 'ledger.json'
            path.write_text(json.dumps(ledger))
            jk.write(Path(tmp) / 'out', path)
            rows = (Path(tmp) / 'out' / 'native.tsv').read_text().split('\n')
        self.assertEqual(rows, ['p/B#must\tabsent\t0\t\ta\\tb\\nc\\\\d', 'p/B#other\tnone\t0\t\t', 'p/B#tick\tported\t1\tcrates/a.rs:7\t', ''])

    def test_no_ledger_stops_the_build(self):
        with self.assertRaises(SystemExit) as stop:
            jk.write(tempfile.gettempdir() + '/none/out', Path(tempfile.gettempdir()) / 'none' / 'ledger.json')
        self.assertIn('no port ledger', str(stop.exception))


if __name__ == '__main__':
    unittest.main()
