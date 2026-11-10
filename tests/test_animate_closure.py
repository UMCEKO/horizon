import shutil
import subprocess
import unittest

from tools import animate_closure


def jdk25():
    java = shutil.which('java')
    if not java:
        return False
    version = subprocess.run([java, '-version'], capture_output=True, text=True).stderr
    return '"25' in version or '"26' in version or '"27' in version


@unittest.skipUnless(animate_closure.JAR.is_file() and jdk25(), 'needs .local/minecraft/26.3/jars/client.jar and JDK 25')
class AnimateClosure(unittest.TestCase):
    def test_the_checked_in_table_is_what_the_jar_gives(self):
        self.assertEqual(animate_closure.TSV.read_text(), animate_closure.generate())

    def test_every_role_is_there_and_the_loop_holds_its_effect_sinks(self):
        rows = [line.split('\t') for line in animate_closure.TSV.read_text().splitlines()]
        self.assertEqual({row[0] for row in rows}, {'loop', 'block', 'fluid'})
        loop = {(row[1].rsplit('/', 1)[1], row[2]) for row in rows if row[0] == 'loop'}
        for expected in (('ClientLevel', 'doAnimateTick'), ('ClientLevel', 'addParticle'), ('ClientLevel', 'doAddParticle'), ('ClientLevel', 'playLocalSound'),
                         ('Block', 'animateTick'), ('Fluid', 'animateTick'), ('FluidState', 'getDripParticle')):
            self.assertIn(expected, loop)
        blocks = {row[1] for row in rows if row[0] == 'block' and row[2] == 'animateTick'}
        self.assertIn('net/minecraft/world/level/block/CampfireBlock', blocks)
        self.assertNotIn('net/minecraft/world/level/block/Block', blocks)


if __name__ == '__main__':
    unittest.main()
