"""tools/worldgen_compare.py: the diff of two small hand-made dumps (no server or game is started)."""

import unittest

from tools import worldgen_compare as wc


def section(n):
    return f'{n:040x}'


def chunk(x, z, sections, surface=64, entities=()):
    heightmaps = {name: [surface] * 256 for name in wc.HEIGHTMAPS}
    return {'x': x, 'z': z, 'sections': sections, 'heightmaps': heightmaps, 'block_entities': list(entities)}


def start(structure, at, box, pieces=()):
    return {'structure': structure, 'chunk': list(at), 'box': box, 'pieces': [list(p) for p in pieces]}


def dump(biomes, starts, chunks, seed=7):
    return {'seed': seed, 'dimensions': {wc.LEVEL: {'biomes': biomes, 'starts': starts, 'chunks': chunks}}}


def same():
    return dump(
        [[0, 64, 0, 'minecraft:plains'], [64, 64, 0, 'minecraft:forest']],
        [start('minecraft:village_plains', (3, 4), [0, 60, 0, 9, 70, 9], [('minecraft:jigsaw', [0, 60, 0, 4, 66, 4])])],
        [chunk(0, 0, [section(1), section(2)])],
    )


class DiffTests(unittest.TestCase):
    def test_equal_dumps_have_no_differences(self):
        result = wc.diff(same(), same())
        self.assertEqual(result['differences'], 0)
        for part in result['dimensions'][wc.LEVEL].values():
            self.assertEqual(part['differences'], 0)

    def test_biome_difference_and_missing_point_count_per_point(self):
        theirs = same()
        biomes = theirs['dimensions'][wc.LEVEL]['biomes']
        biomes[0][3] = 'minecraft:desert'
        del biomes[1]
        found = wc.diff(same(), theirs)['dimensions'][wc.LEVEL]['biomes']
        self.assertEqual((found['compared'], found['differences']), (2, 2))
        self.assertEqual(found['rate'], 1.0)
        self.assertEqual(found['first'][0], {'at': [0, 64, 0], 'forge': 'minecraft:plains', 'horizon': 'minecraft:desert'})
        self.assertIsNone(found['first'][1]['horizon'])

    def test_starts_are_keyed_by_structure_and_chunk(self):
        theirs = same()
        starts = theirs['dimensions'][wc.LEVEL]['starts']
        starts[0]['box'] = [0, 60, 0, 9, 71, 9]
        starts.append(start('minecraft:pillager_outpost', (1, 1), [0, 0, 0, 1, 1, 1]))
        found = wc.diff(same(), theirs)['dimensions'][wc.LEVEL]['starts']
        self.assertEqual((found['missing'], found['extra'], found['box_differences'], found['piece_differences']), (0, 1, 1, 0))
        self.assertEqual(found['differences'], 2)

    def test_piece_difference_alone_is_found(self):
        theirs = same()
        theirs['dimensions'][wc.LEVEL]['starts'][0]['pieces'][0][0] = 'minecraft:other'
        found = wc.diff(same(), theirs)['dimensions'][wc.LEVEL]['starts']
        self.assertEqual((found['piece_differences'], found['box_differences'], found['differences']), (1, 0, 1))
        self.assertEqual(found['first'][0]['counts'], [1, 1])

    def test_a_start_only_forge_makes_is_missing(self):
        theirs = same()
        theirs['dimensions'][wc.LEVEL]['starts'] = []
        found = wc.diff(same(), theirs)['dimensions'][wc.LEVEL]['starts']
        self.assertEqual((found['missing'], found['extra'], found['differences']), (1, 0, 1))
        self.assertEqual(found['missing_first'], [{'structure': 'minecraft:village_plains', 'chunk': [3, 4]}])

    def test_chunk_reports_sections_heightmaps_and_block_entities(self):
        theirs = same()
        theirs['dimensions'][wc.LEVEL]['chunks'] = [
            chunk(0, 0, [section(1), section(9)], entities=[[1, 2, 3, 'minecraft:chest']])]
        theirs['dimensions'][wc.LEVEL]['chunks'][0]['heightmaps']['OCEAN_FLOOR'][5] = 63
        found = wc.diff(same(), theirs)['dimensions'][wc.LEVEL]['chunks']
        self.assertEqual(found['differences'], 1)
        entry = found['first'][0]
        self.assertEqual(entry['sections'], [1])
        self.assertEqual(entry['heightmap_columns'], {'OCEAN_FLOOR': 1})
        self.assertIn('block_entities', entry)

    def test_unloaded_chunk_and_other_seed_and_missing_level_count(self):
        theirs = same()
        theirs['dimensions'][wc.LEVEL]['chunks'] = []
        self.assertEqual(wc.diff(same(), theirs)['differences'], 1)
        self.assertEqual(wc.diff(same(), dump([], [], [], seed=8))['differences'], 1 + 2 + 1 + 1)
        self.assertEqual(wc.diff(same(), {'seed': 7, 'dimensions': {}})['differences'], 1)

    def test_only_the_first_twenty_differences_are_listed(self):
        ours = dump([[i, 64, 0, 'a'] for i in range(50)], [], [])
        theirs = dump([[i, 64, 0, 'b'] for i in range(50)], [], [])
        found = wc.diff(ours, theirs)['dimensions'][wc.LEVEL]['biomes']
        self.assertEqual((found['differences'], len(found['first'])), (50, 20))


if __name__ == '__main__':
    unittest.main()
