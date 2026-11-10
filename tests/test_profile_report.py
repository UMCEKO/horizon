import os
import sys
import tempfile
import unittest

from tools import profile_report as pr

SERVER = (
    "t=1000 ticks=20 mean_ms=5.00 max_ms=40.00 p99_ms=30.00 players=12 chunks=900 mobs=50 items=3 orbs=0 "
    "block_entities=200 java_ms=60.00 java_pct=60.0 spans=mobs|20|50000|9000,lan|20|20000|2000"
)
JAVA = "t=1010 calls=block_entity_tick|400|60000|900 be=mekanism:cable|300|40000|500,ae2:drive|100|20000|400 lock=click_block|3|3000|1400 ev=ServerTickEvent.Post|20|90000|8000,LevelTickEvent.Pre|20|1000|100"


class Report(unittest.TestCase):
    def test_rows_parse_and_skip_junk(self):
        self.assertEqual(pr.parse_rows("a:b|2|10|7,junk,c|x|1|1"), {"a:b": (2, 10, 7)})
        self.assertEqual(pr.parse_rows("-"), {})

    def test_the_report_ranks_and_ties_worst_seconds_to_the_jvm(self):
        with tempfile.TemporaryDirectory() as d:
            open(os.path.join(d, "server-ticks.log"), "w").write(SERVER + "\nnot a line\n")
            open(os.path.join(d, "java-ticks.log"), "w").write(JAVA + "\n")
            text = pr.report(d, jfr=False)
        self.assertIn("1 s of server ticks, 20 ticks, mean 5.00 ms", text)
        self.assertLess(text.index("mekanism:cable"), text.index("ae2:drive"))
        self.assertIn("be mekanism:cable lock click_block", text)
        self.assertIn("span mobs", text)
        self.assertLess(text.index("ServerTickEvent.Post"), text.index("LevelTickEvent.Pre"))

    def test_an_empty_folder_reports_nothing_recorded(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertIn("no server-ticks.log", pr.report(d, jfr=False))


if __name__ == "__main__":
    unittest.main()
