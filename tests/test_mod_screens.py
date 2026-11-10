import json
import struct
import unittest
import zlib
from pathlib import Path

from tools import hz_scenario, mod_screens

SCENARIOS = Path(__file__).resolve().parent.parent / 'tools' / 'scenarios'


def png(width, height, color, raw_rows):
    def chunk(kind, body):
        return struct.pack('>I', len(body)) + kind + body + struct.pack('>I', zlib.crc32(kind + body))
    return (b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', struct.pack('>IIBBBBB', width, height, 8, color, 0, 0, 0))
            + chunk(b'IDAT', zlib.compress(b''.join(raw_rows))) + chunk(b'IEND', b''))


class Scenario(unittest.TestCase):
    def test_example_scenarios_are_valid(self):
        self.assertEqual(mod_screens.check_scenario(json.loads((SCENARIOS / 'gadget-preview-compare.json').read_text())), ['preview'])
        self.assertEqual(mod_screens.check_scenario(json.loads((SCENARIOS / 'gadget-preview-grass.json').read_text())), ['grass-preview'])

    def test_rejects_unknown_step_and_missing_screenshot(self):
        with self.assertRaises(ValueError):
            mod_screens.check_scenario([{'dance': 1}, {'screenshot': 'a'}])
        with self.assertRaises(ValueError):
            mod_screens.check_scenario([{'wait': 1}])
        with self.assertRaises(ValueError):
            mod_screens.check_scenario([{'camera': [1, 2, 3]}, {'screenshot': 'a'}])

    def test_every_kind_has_a_horizon_translation(self):
        samples = {'wait': 5, 'command': '/say x', 'look': [1, 2], 'lookat': [1, 2, 3], 'slot': 0, 'use': 2, 'attack': 2, 'key': 'key.inventory',
                   'press': 'key.sneak', 'release': 'key.sneak', 'close': True, 'tp': [1, 2, 3], 'camera': [1, 2, 3, 0, 45], 'hud': False, 'quit': True}
        for kind, arg in samples.items():
            hz_scenario.ctl_commands(kind, arg)

    def test_camera_teleports_looks_then_waits(self):
        self.assertEqual(hz_scenario.ctl_commands('camera', [1, 2, 3, 10, 45]),
                         [['tp', '1', '2', '3'], ['look', '10', '45'], ['wait', str(hz_scenario.CAMERA_SETTLE)]])

    def test_hud_false_taps_f1_and_true_does_nothing(self):
        self.assertEqual(hz_scenario.ctl_commands('hud', False), [['key', 'f1', 'tap']])
        self.assertEqual(hz_scenario.ctl_commands('hud', True), [])

    def test_size_argument(self):
        self.assertEqual(mod_screens.parse_size('1600x1200'), (1600, 1200))


class Images(unittest.TestCase):
    def test_round_trip_rgb_and_rgba(self):
        rows = [bytes([1, 2, 3, 4, 5, 6]), bytes([7, 8, 9, 10, 11, 12])]
        self.assertEqual(mod_screens.decode_png(mod_screens.encode_png(2, 2, rows)), (2, 2, rows))
        rgba = png(2, 1, 6, [b'\0' + bytes([1, 2, 3, 255, 4, 5, 6, 255])])
        self.assertEqual(mod_screens.decode_png(rgba), (2, 1, [bytes([1, 2, 3, 4, 5, 6])]))

    def test_filters(self):
        row1, row2 = bytes([10, 20, 30, 40, 50, 60]), bytes([11, 22, 33, 44, 55, 66])
        sub = bytes([row2[i] - (row2[i - 3] if i >= 3 else 0) & 255 for i in range(6)])
        up = bytes((b - a) & 255 for a, b in zip(row1, row2))
        for kind, line in ((1, sub), (2, up)):
            data = png(2, 2, 2, [b'\0' + row1, bytes([kind]) + line])
            self.assertEqual(mod_screens.decode_png(data)[2], [row1, row2])
        avg = bytes((row2[i] - (((row2[i - 3] if i >= 3 else 0) + row1[i]) >> 1)) & 255 for i in range(6))
        self.assertEqual(mod_screens.decode_png(png(2, 2, 2, [b'\0' + row1, b'\3' + avg]))[2], [row1, row2])
        paeth = []
        for i in range(6):
            a, b, c = (row2[i - 3] if i >= 3 else 0), row1[i], (row1[i - 3] if i >= 3 else 0)
            p = a + b - c
            pa, pb, pc = abs(p - a), abs(p - b), abs(p - c)
            paeth.append((row2[i] - (a if pa <= pb and pa <= pc else b if pb <= pc else c)) & 255)
        self.assertEqual(mod_screens.decode_png(png(2, 2, 2, [b'\0' + row1, b'\4' + bytes(paeth)]))[2], [row1, row2])

    def test_rejects_non_png(self):
        with self.assertRaises(ValueError):
            mod_screens.decode_png(b'nope')

    def test_side_by_side_layout(self):
        red = (2, 3, [bytes([255, 0, 0] * 2)] * 3)
        blue = (3, 2, [bytes([0, 0, 255] * 3)] * 2)
        width, height, rows = mod_screens.side_by_side(red, blue)
        self.assertEqual((width, height), (2 + mod_screens.GAP + 3, 3 + mod_screens.BAR))
        self.assertTrue(all(len(r) == width * 3 for r in rows))
        body = rows[mod_screens.BAR:]
        self.assertEqual(body[0][:3], bytes([255, 0, 0]))
        self.assertEqual(body[0][-3:], bytes([0, 0, 255]))
        self.assertEqual(body[2][-3:], bytes(mod_screens.BACK))

    def test_labels_are_drawn_on_the_correct_side(self):
        left, right = (60, 1, [bytes(180)]), (60, 1, [bytes(180)])
        _, _, rows = mod_screens.side_by_side(left, right)
        ink = bytes(mod_screens.INK)
        half = 60 * 3
        self.assertTrue(any(ink in r[:half] for r in rows[:mod_screens.BAR]))
        self.assertTrue(any(ink in r[half + mod_screens.GAP * 3:] for r in rows[:mod_screens.BAR]))

    def test_compose_writes_a_valid_png(self):
        import tempfile
        with tempfile.TemporaryDirectory() as d:
            a, b = Path(d) / 'a.png', Path(d) / 'b.png'
            a.write_bytes(mod_screens.encode_png(2, 2, [bytes(6)] * 2))
            b.write_bytes(mod_screens.encode_png(2, 2, [bytes([9] * 6)] * 2))
            width, height, _ = mod_screens.decode_png(mod_screens.compose(a, b))
            self.assertEqual((width, height), (4 + mod_screens.GAP, 2 + mod_screens.BAR))


if __name__ == '__main__':
    unittest.main()
