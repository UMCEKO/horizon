import json
import tempfile
import unittest
from pathlib import Path

from tools import java_mods

NEO = {'arguments': {'jvm': ['-Dlibs=${library_directory}', '--add-opens', 'java.base/java.lang=ALL-UNNAMED', '-Xss1m'], 'game': ['--fml.neoForgeVersion', '1']}}
SERVER = '\n'.join([
    '-Dlibs=libraries', '--add-modules ALL-MODULE-PATH', '-classpath', 'libraries/a/a.jar:libraries/b/b.jar',
    '--add-exports java.base/x=y', '--fml.mcVersion 26.3', 'dev.Main', ''])


class Args(unittest.TestCase):
    def test_client_args_name_the_libraries_assets_and_neoforge(self):
        libs = Path('/i/libraries')
        lines = java_mods.args_lines('client', libs, '9.9', client=(NEO, [libs / 'a.jar', libs / 'b.jar']), assets=(Path('/i/assets'), '34'))
        self.assertEqual(lines, [
            'dist client', 'cp /i/libraries/a.jar:/i/libraries/b.jar', 'jvm -Dlibs=/i/libraries',
            'jvm --add-opens=java.base/java.lang=ALL-UNNAMED', 'jvm -Xss1m', 'prog --fml.neoForgeVersion', 'prog 1',
            'mod /i/libraries/net/neoforged/neoforge/9.9/neoforge-9.9-universal.jar',
            'prog --assetsDir', 'prog /i/assets', 'prog --assetIndex', 'prog 34'])

    def test_server_args_come_from_unix_args_and_have_no_dist_line(self):
        lines = java_mods.args_lines('server', Path('/i/libraries'), '9.9', server=SERVER)
        self.assertEqual(lines[0], 'cp /i/libraries/a/a.jar:/i/libraries/b/b.jar')
        self.assertIn('jvm -Dlibs=/i/libraries', lines)
        self.assertIn('jvm --add-exports=java.base/x=y', lines)
        self.assertIn('prog --fml.mcVersion', lines)
        self.assertNotIn('dist client', lines)
        self.assertFalse(any('assetsDir' in line for line in lines))


class Jdk(unittest.TestCase):
    def test_platform_names(self):
        self.assertEqual(java_mods.jdk_platform('linux', 'x86_64'), ('linux', 'x64'))
        self.assertEqual(java_mods.jdk_platform('darwin', 'arm64'), ('mac', 'aarch64'))
        with self.assertRaises(SystemExit):
            java_mods.jdk_platform('win32', 'AMD64')

    def test_query_asks_for_the_jdk_of_the_platform(self):
        url = java_mods.jdk_query('linux', 'x64')
        self.assertIn('/latest/25/hotspot', url)
        self.assertIn('architecture=x64', url)
        self.assertIn('image_type=jdk', url)

    def test_asset_carries_the_published_checksum(self):
        listing = [{'release_name': 'jdk-25+1', 'binary': {'package': {'name': 'j.tar.gz', 'link': 'https://x/j.tar.gz', 'checksum': 'ab'}}}]
        self.assertEqual(java_mods.jdk_asset(listing), {'name': 'j.tar.gz', 'url': 'https://x/j.tar.gz', 'sha256': 'ab', 'release': 'jdk-25+1'})
        listing[0]['binary']['package']['name'] = 'j.zip'
        with self.assertRaises(SystemExit):
            java_mods.jdk_asset(listing)

    def test_download_checks_the_checksum_and_keeps_good_files(self):
        with tempfile.TemporaryDirectory() as tmp:
            source = Path(tmp) / 'src.bin'
            source.write_bytes(b'jdk')
            good = java_mods.lab.digest(source, 'sha256')
            dest = Path(tmp) / 'cache' / 'j.bin'
            java_mods.download(source.as_uri(), dest, good)
            self.assertEqual(dest.read_bytes(), b'jdk')
            source.write_bytes(b'tampered')
            self.assertEqual(java_mods.download(source.as_uri(), dest, good), dest)
            self.assertEqual(dest.read_bytes(), b'jdk')
            with self.assertRaises(SystemExit):
                java_mods.download(source.as_uri(), Path(tmp) / 'other.bin', good)

    def test_a_given_jdk_must_be_25(self):
        with tempfile.TemporaryDirectory() as tmp:
            home = Path(tmp)
            (home / 'bin').mkdir()
            (home / 'bin' / 'java').write_text('')
            (home / 'release').write_text('JAVA_VERSION="21.0.1"\n')
            with self.assertRaises(SystemExit):
                java_mods.provide_jdk(home / 'install', home, home)
            (home / 'release').write_text('JAVA_VERSION="25.0.5"\n')
            self.assertEqual(java_mods.provide_jdk(home / 'install', home, home), home.resolve())


class Launch(unittest.TestCase):
    record = {'java': '/i/jdk', 'neoforge': '9.9', 'resources': '/r'}

    def test_env_has_what_the_game_needs_and_homes_inside_the_install(self):
        env = java_mods.launch_env(Path('/i'), self.record, False, {'PATH': '/bin'})
        self.assertEqual(env['JAVA_HOME'], '/i/jdk')
        self.assertEqual(env['HORIZON_JAVA_RUN'], '/i/client')
        self.assertEqual(env['HORIZON_JAVA_HOST_JAR'], '/i/host/horizon-java-host.jar')
        self.assertEqual(env['HORIZON_NEOFORGE'], '9.9')
        self.assertEqual(env['HORIZON_HOME'], '/i/home/horizon')
        self.assertTrue(env.get('LD_LIBRARY_PATH', env.get('DYLD_LIBRARY_PATH')).startswith('/i/jdk/lib/server:/i/jdk/lib'))
        self.assertEqual(env['PATH'], '/bin')

    def test_homes_the_caller_set_stay_and_the_server_has_its_own_run(self):
        env = java_mods.launch_env(Path('/i'), self.record, True, {'HORIZON_HOME': '/mine', 'LD_LIBRARY_PATH': '/x'})
        self.assertEqual(env['HORIZON_HOME'], '/mine')
        self.assertEqual(env['XDG_DATA_HOME'], '/i/home/xdg-data')
        self.assertEqual(env['HORIZON_JAVA_RUN'], '/i/server')

    def test_command_adds_resources_unless_given(self):
        self.assertEqual(java_mods.launch_command('g', self.record, ['--world', 'W']), ['g', '--resources', '/r', '--world', 'W'])
        self.assertEqual(java_mods.launch_command('g', self.record, ['--resources', '/mine']), ['g', '--resources', '/mine'])


class Sync(unittest.TestCase):
    def test_links_follow_the_wanted_jars_only(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 'mods').mkdir()
            for name in ('a.jar', 'b.jar'):
                (root / 'mods' / name).write_bytes(b'x')
            folder = root / 'run' / 'mods'
            java_mods.sync_links(folder, [root / 'mods' / 'a.jar', root / 'mods' / 'b.jar'])
            java_mods.sync_links(folder, [root / 'mods' / 'b.jar'])
            self.assertEqual(sorted(p.name for p in folder.iterdir()), ['b.jar'])
            self.assertEqual((folder / 'b.jar').read_bytes(), b'x')

    def test_sync_file_replaces_a_changed_file(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            (root / 's').write_bytes(b'one')
            java_mods.sync_file(root / 's', root / 'd' / 'f')
            (root / 's').write_bytes(b'three')
            java_mods.sync_file(root / 's', root / 'd' / 'f')
            self.assertEqual((root / 'd' / 'f').read_bytes(), b'three')


if __name__ == '__main__':
    unittest.main()
