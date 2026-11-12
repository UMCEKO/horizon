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

    def test_the_call_tree_ranks_by_exclusive_time_and_sums_both_directions(self):
        line = (JAVA + " hold=block_entity_tick|400|50000|800"
                " tree=block_entity_tick|400|60000|20000|900|800|0|1|8000|2000|8000|300,native:level_get_block|800|40000|40000|90|0|0|2|0|0|0|0"
                " chains=block_entity_tick>native:level_get_block|800|40000|90,block_entity_tick|400|60000|900 scratch_kb=128")
        with tempfile.TemporaryDirectory() as d:
            open(os.path.join(d, "java-ticks.log"), "w").write(line + "\n")
            text = pr.report(d, jfr=False)
        self.assertIn("crossings: 400 Rust to Java, 800 Java to Rust; 9.8 KiB on the wire, 7.8 KiB copied; scratch high-water 128 KiB", text)
        tree = text[text.index("Crossing call tree"):]
        self.assertLess(tree.index("native:level_get_block"), tree.index("block_entity_tick"))
        self.assertIn("block_entity_tick>native:level_get_block", text)
        self.assertIn("Server-side turn held", text)

    def test_an_empty_folder_reports_nothing_recorded(self):
        with tempfile.TemporaryDirectory() as d:
            self.assertIn("no server-ticks.log", pr.report(d, jfr=False))


if __name__ == "__main__":
    unittest.main()
