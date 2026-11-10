import json
import tempfile
import unittest
from pathlib import Path

from tools import java_mods


def prepared(root, files):
    resources = root / 'resources'
    (resources / 'assets').mkdir(parents=True)
    objects = {}
    for name, (digest, body) in files.items():
        objects[name] = {'hash': digest, 'size': len(body or b'')}
        if body is not None:
            path = resources / 'assets' / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(body)
    (resources / 'asset-index.json').write_text(json.dumps({'objects': objects}))
    return resources


class AssetsIndex(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.root = Path(self.tmp.name)
        self.addCleanup(self.tmp.cleanup)

    def test_vanilla_layout_of_symlinks_over_the_prepared_files(self):
        sound = 'ab' + '1' * 38
        resources = prepared(self.root, {'minecraft/sounds/a.ogg': (sound, b'ogg'), 'minecraft/sounds.json': ('cd' + '2' * 38, b'{}')})
        folder = java_mods.assets_index(resources, '34', self.root / 'out')
        index = json.loads((folder / 'indexes' / '34.json').read_text())
        self.assertEqual(index['objects']['minecraft/sounds/a.ogg'], {'hash': sound, 'size': 3})
        link = folder / 'objects' / 'ab' / sound
        self.assertTrue(link.is_symlink())
        self.assertEqual(link.read_bytes(), b'ogg')
        self.assertEqual(link.resolve(), (resources / 'assets' / 'minecraft/sounds/a.ogg').resolve())

    def test_entries_the_prepared_assets_lack_are_left_out(self):
        resources = prepared(self.root, {'icons/x.png': ('ef' + '3' * 38, None), 'minecraft/lang/en_us.json': ('01' + '4' * 38, b'{}')})
        folder = java_mods.assets_index(resources, '7', self.root / 'out')
        self.assertEqual(list(json.loads((folder / 'indexes' / '7.json').read_text())['objects']), ['minecraft/lang/en_us.json'])
        self.assertFalse((folder / 'objects' / 'ef').exists())

    def test_builds_into_a_root_that_does_not_exist_yet(self):
        resources = prepared(self.root, {'minecraft/a.ogg': ('40' + '7' * 38, b'1')})
        folder = java_mods.assets_index(resources, '34', self.root / 'fresh' / 'jars')
        self.assertTrue((folder / 'objects' / '40' / ('40' + '7' * 38)).is_symlink())

    def test_generated_once_and_regenerated_when_the_content_changes(self):
        resources = prepared(self.root, {'minecraft/a.ogg': ('aa' + '5' * 38, b'1')})
        first = java_mods.assets_index(resources, '34', self.root / 'out')
        marker = first / 'marker'
        marker.write_text('kept')
        self.assertEqual(java_mods.assets_index(resources, '34', self.root / 'out'), first)
        self.assertTrue(marker.exists())
        (resources / 'assets' / 'minecraft' / 'b.ogg').write_bytes(b'2')
        index = json.loads((resources / 'asset-index.json').read_text())
        index['objects']['minecraft/b.ogg'] = {'hash': 'bb' + '6' * 38, 'size': 1}
        (resources / 'asset-index.json').write_text(json.dumps(index))
        self.assertNotEqual(java_mods.assets_index(resources, '34', self.root / 'out'), first)
        self.assertNotEqual(java_mods.assets_index(resources, '35', self.root / 'out'), first)


if __name__ == '__main__':
    unittest.main()
