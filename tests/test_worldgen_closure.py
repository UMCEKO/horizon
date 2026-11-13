import unittest

from tests.test_animate_closure import jdk25
from tools import worldgen_closure

PIECE = 'net/minecraft/world/level/levelgen/structure/StructurePiece'
LEGACY = ('mineshaft', 'fortress', 'stronghold', 'ocean_monument', 'desert_pyramid', 'jungle_temple', 'swamp_hut')


def rows():
    return [line.split('\t') for line in worldgen_closure.TSV.read_text().splitlines()]


class CommittedTable(unittest.TestCase):
    def setUp(self):
        self.rows = rows()

    def seams_of(self, owner, method, desc=None):
        return {(r[0], r[1]) for r in self.rows if r[2] == owner and r[3] == method and (desc is None or r[4] == desc)}

    def test_rows_are_sorted_unique_and_typed_only_where_the_seam_is_per_type(self):
        lines = worldgen_closure.TSV.read_text().splitlines()
        self.assertEqual(lines, sorted(set(lines)))
        for seam, ty, *_ in self.rows:
            self.assertEqual(seam in ('carver', 'feature', 'start', 'place'), ty != '', (seam, ty))

    def test_place_block_belongs_to_every_legacy_structure_placement_and_no_start(self):
        seams = self.seams_of(PIECE, 'placeBlock')
        for legacy in LEGACY:
            self.assertIn(('place', 'minecraft:' + legacy), seams)
        self.assertFalse({s for s in seams if s[0] == 'start'})

    def test_jigsaw_children_are_placed_under_the_jigsaw_start(self):
        seams = self.seams_of('net/minecraft/world/level/levelgen/structure/pools/JigsawPlacement$Placer', 'tryPlacingChildren')
        self.assertIn(('start', 'minecraft:jigsaw'), seams)
        self.assertNotIn(('start', 'minecraft:mineshaft'), seams)

    def test_the_multi_noise_target_point_lookup_is_the_biome_seam(self):
        seams = self.seams_of('net/minecraft/world/level/biome/MultiNoiseBiomeSource', 'getNoiseBiome',
                              '(Lnet/minecraft/world/level/biome/Climate$TargetPoint;)Lnet/minecraft/core/Holder;')
        self.assertEqual(seams, {('biomes', '')})

    def test_build_terrain_is_the_terrain_seam(self):
        self.assertIn(('terrain', ''), self.seams_of('net/minecraft/world/level/levelgen/NoiseBasedChunkGenerator', 'buildTerrain'))


@unittest.skipUnless(worldgen_closure.JAR.is_file() and jdk25(), 'needs .local/minecraft/26.3/jars/server-game.jar and JDK 25')
class Regenerated(unittest.TestCase):
    def test_the_checked_in_table_is_what_the_jar_gives(self):
        self.assertEqual(worldgen_closure.TSV.read_text(), worldgen_closure.generate())


if __name__ == '__main__':
    unittest.main()
