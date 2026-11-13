import hashlib
import tempfile
import unittest
from pathlib import Path

from tools import java_mods_release

ARGS = 'java /old/jdk\nhost /old/host.jar\ndist client\ncp /a.jar:/b.jar\njvm -Xss1m\nprog --x\njava-like keeps\nmod /m.jar\n'


class RunFolder(unittest.TestCase):
    def test_java_and_host_lines_are_replaced_and_the_rest_kept_in_order(self):
        text = java_mods_release.rewrite_args(ARGS, '/jdk-25', '/out/java-mods/horizon-java-host.jar')
        self.assertEqual(text, 'java /jdk-25\nhost /out/java-mods/horizon-java-host.jar\ndist client\ncp /a.jar:/b.jar\njvm -Xss1m\nprog --x\njava-like keeps\nmod /m.jar\n')


class Sums(unittest.TestCase):
    def test_lines_are_hash_two_spaces_name(self):
        with tempfile.TemporaryDirectory() as tmp:
            one, two = Path(tmp) / 'a.tar.gz', Path(tmp) / 'b.tar.gz'
            one.write_bytes(b'one')
            two.write_bytes(b'two bytes')
            text = java_mods_release.sha256sums([one, two])
        want = f"{hashlib.sha256(b'one').hexdigest()}  a.tar.gz\n{hashlib.sha256(b'two bytes').hexdigest()}  b.tar.gz\n"
        self.assertEqual(text, want)
        self.assertTrue(all(len(line.split('  ')[0]) == 64 for line in text.splitlines()))


if __name__ == '__main__':
    unittest.main()
