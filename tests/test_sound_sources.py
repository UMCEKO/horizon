import json
import unittest

from tools import java_sound_sources as js
from tests.test_animate_closure import jdk25


class TableRules(unittest.TestCase):
    ROWS = [('a/B', '*', 'native'), ('a/C', 'tick', 'native'), ('a/D', 'tick', 'java')]

    def test_a_class_row_covers_its_lambdas_inner_classes_and_methods(self):
        self.assertEqual(js.table_owner(self.ROWS, 'a/B', 'anything'), 'native')
        self.assertEqual(js.table_owner(self.ROWS, 'a/B$Inner', 'run'), 'native')

    def test_a_method_row_covers_only_that_method(self):
        self.assertEqual(js.table_owner(self.ROWS, 'a/C', 'tick'), 'native')
        self.assertIsNone(js.table_owner(self.ROWS, 'a/C', 'other'))
        self.assertEqual(js.table_owner(self.ROWS, 'a/D', 'tick'), 'java')

    def test_plumbing_is_the_engine_and_the_play_entry_points(self):
        level = 'net/minecraft/client/multiplayer/ClientLevel'
        self.assertTrue(js.plumbing('net/minecraft/client/sounds/SoundManager', 'play'))
        self.assertTrue(js.plumbing(level, 'playLocalSound'))
        self.assertTrue(js.plumbing(level, 'lambda$playSound$0'))
        self.assertFalse(js.plumbing(level, 'tickWeatherEffects'))
        self.assertFalse(js.plumbing(level, 'lambda$tickWeatherEffects$0'))
        self.assertFalse(js.plumbing('net/minecraft/world/entity/Entity', 'playEntityOnFireExtinguishedSound'))


@unittest.skipUnless(js.JAR.is_file() and js.mr.LEDGER_JSON.is_file() and jdk25(), 'needs the vanilla client jar, the port ledger and JDK 25')
class SoundSources(unittest.TestCase):
    def test_no_sound_native_makes_is_left_out_of_the_table(self):
        with open(js.mr.LEDGER_JSON) as ledger:
            result = js.run(ledger=json.load(ledger))
        missing = [f'{cls}#{method}' for cls, method, _, owner, _ in result if owner == 'native-unlisted']
        self.assertEqual(missing, [], 'a native-owned vanilla source reaches a sound sink and boot/res/sound_sources.tsv has no row for it')


if __name__ == '__main__':
    unittest.main()
