import tempfile
import unittest
from pathlib import Path

from tools import mod_replaces as mr

CLOSURE = '\n'.join([
    'loop\tnet/minecraft/client/multiplayer/ClientLevel\tdoAnimateTick\t(IIII)V',
    'loop\tnet/minecraft/client/multiplayer/ClientLevel\taddParticle\t(Lp;DDDDDD)V',
    'block\tnet/minecraft/world/level/block/BaseFireBlock\tanimateTick\t(Lb;)V\tnet/minecraft/world/level/block/BaseFireBlock',
    'block\tnet/minecraft/world/level/block/AbstractCandleBlock\taddParticlesAndSound\t()V\tnet/minecraft/world/level/block/CandleBlock,net/minecraft/world/level/block/CandleCakeBlock',
    'fluid\tnet/minecraft/world/level/material/LavaFluid\tanimateTick\t(Lf;)V\tnet/minecraft/world/level/material/LavaFluid',
]) + '\n'


def report(**changes):
    return {'methods': {key: {'changes': value} for key, value in changes.items()}}


def mixin(desc=None):
    return {'mod': 'm', 'mechanism': 'mixin', 'kind': 'inject', 'desc': desc, 'routing': 'java'}


def override(*classes, abstract=False):
    return {'mod': 'm', 'mechanism': 'override', 'classes': list(classes), 'abstract': abstract, 'routing': 'java-already'}


class AnimateRouting(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.closure = Path(self.tmp.name) / 'animate_closure.tsv'
        self.closure.write_text(CLOSURE)

    def routing(self, **changes):
        return mr.animate_routing(report(**changes), self.closure)

    def test_overrides_of_animate_tick_are_the_classes_java_runs(self):
        found = mr.animate_classes(report(**{
            'net/minecraft/world/level/block/Block#animateTick': [override('a.One', 'a.Two'), override('b.Three', abstract=True)],
            'net/minecraft/world/level/block/TorchBlock#animateTick': [override('c.Torchy')],
            'net/minecraft/world/level/block/Block#tick': [override('d.NotThis')],
        }))
        self.assertEqual(found, ['a.One', 'a.Two', 'c.Torchy'])

    def test_a_mixin_of_the_loop_runs_vanillas_whole_loop(self):
        got = self.routing(**{'net/minecraft/client/multiplayer/ClientLevel#doAnimateTick': [mixin()]})
        self.assertEqual(got, {'wholeLoop': True, 'patchedBlocks': [], 'patchedFluids': []})

    def test_a_mixin_of_an_effect_sink_is_the_loop_too(self):
        got = self.routing(**{'net/minecraft/client/multiplayer/ClientLevel#addParticle': [mixin('(Lp;DDDDDD)V')]})
        self.assertTrue(got['wholeLoop'])

    def test_a_mixin_of_a_blocks_animate_tick_patches_that_block_class_only(self):
        got = self.routing(**{'net/minecraft/world/level/block/BaseFireBlock#animateTick': [mixin()]})
        self.assertEqual(got, {'wholeLoop': False, 'patchedBlocks': ['net/minecraft/world/level/block/BaseFireBlock'], 'patchedFluids': []})

    def test_a_helper_serves_every_block_class_that_only_it_serves(self):
        got = self.routing(**{'net/minecraft/world/level/block/AbstractCandleBlock#addParticlesAndSound': [mixin()]})
        self.assertEqual(got['patchedBlocks'], ['net/minecraft/world/level/block/CandleBlock', 'net/minecraft/world/level/block/CandleCakeBlock'])

    def test_a_mixin_of_a_fluid_patches_that_fluid_class(self):
        got = self.routing(**{'net/minecraft/world/level/material/LavaFluid#animateTick': [mixin()]})
        self.assertEqual(got['patchedFluids'], ['net/minecraft/world/level/material/LavaFluid'])

    def test_another_descriptor_is_another_method(self):
        got = self.routing(**{'net/minecraft/world/level/block/BaseFireBlock#animateTick': [mixin('(Lother;)V')]})
        self.assertEqual(got['patchedBlocks'], [])

    def test_a_mixin_of_an_unrelated_method_changes_nothing(self):
        got = self.routing(**{'net/minecraft/world/level/block/BaseFireBlock#tick': [mixin()]})
        self.assertEqual(got, {'wholeLoop': False, 'patchedBlocks': [], 'patchedFluids': []})


if __name__ == '__main__':
    unittest.main()
