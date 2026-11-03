import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

from tools import logsum

NARRATOR = """[23:36:29] [Thread-0/ERROR] [mojang/Narrator]: Error while loading the narrator
com.mojang.text2speech.Narrator$InitializeException: Failed to load library flite
\tat com.mojang.text2speech.NarratorLinux$FliteLibrary.loadNative(NarratorLinux.java:82) ~[text2speech-1.19.12.jar:?] {}
\tat com.mojang.text2speech.NarratorLinux.<init>(NarratorLinux.java:19) [text2speech-1.19.12.jar:?] {}
\tat java.base/java.lang.Thread.run(Thread.java:1474)
\tat TRANSFORMER/minecraft@26.3/net.minecraft.client.GameNarrator.<init>(GameNarrator.java:22) [x.jar:?] {}
\tat org.spongepowered.asm.mixin.Foo.bar(Foo.java:1)
\tat TRANSFORMER/horizon_java@1/dev.horizonlab.javamods.boot.Boot.main(Boot.java:62) [horizon-java-boot.jar:?] {}
Caused by: java.lang.UnsatisfiedLinkError: Unable to load library 'flite':
libflite.so: cannot open shared object file: No such file or directory
\tat com.sun.jna.NativeLibrary.getInstance(NativeLibrary.java:481) ~[jna-5.17.0.jar:5.17.0 (b0)] {}
\t... 6 more
\tSuppressed: java.io.IOException: Native library not found
\t\tat com.sun.jna.Native.open(Native Method)
"""

ITEM_FAIL = """Horizon: an item's text failed
java.lang.IllegalStateException: no client level
\tat TRANSFORMER/horizon_java@1/dev.horizonlab.javamods.boot.ItemText.take(ItemText.java:32)
\tat TRANSFORMER/horizon_java@1/dev.horizonlab.javamods.boot.ClientBridge.run(ClientBridge.java:182)
\tat java.base/java.util.concurrent.FutureTask.run(FutureTask.java:328)
\tat java.base/java.lang.Thread.run(Thread.java:1474)
"""

BOOT = """Horizon 0.1.0
[23:36:24] [Thread-0/INFO] [ne.ne.fm.lo.FMLLoader/]: Starting FancyModLoader version 12.0.12 (CLIENT in PROD)
[23:36:29] [Thread-0/INFO] [minecraft/ReloadableResourceManager]: Reloading ResourceManager: vanilla
Horizon: Java mods ready in 10.8s (mods 6.6s, data 3.7s, facts 0.5s), 1136 items
[23:36:46] [Server thread/INFO]: Player14000[/127.0.0.1:0] logged in with entity id 81 at (-13.0, 64.0, -4.1)
[23:36:46] [Server thread/INFO]: System chat: Player14000 joined the game
"""

WARNINGS = """[23:36:45] [Server thread/WARN] [me.co.Mekanism/]: Failed to retrieve username for UUID 80da6dda-77c6-3bb2-9aef-2d6a421b42b8, you might want
[23:36:46] [Server thread/WARN] [me.co.Mekanism/]: Failed to retrieve username for UUID 11111111-2222-3333-4444-555555555555, you might want
[23:36:47] [Render thread/WARN] [minecraft/ClientPacketListener]: Ignoring chat session from Player14000 due to missing Services public key
    [23:51:55] [Server thread/WARN]: **** SERVER IS RUNNING IN OFFLINE/INSECURE MODE!
WARNING: A restricted method in java.lang.System has been called
"""

NOISE = ''.join(f'Horizon: use kind=7 result=Pass[] held={n} buildinggadgets2:gadget_building\n' for n in range(5)) + \
    '[23:36:29] [modloading-sync-worker/INFO] [sn.ja.Jade/]: Start loading plugin from Mekanism: mekanism.Plugin\n' \
    '[23:36:29] [modloading-sync-worker/INFO] [sn.ja.Jade/]: Start loading plugin from AE2: ae2.Plugin\n'


def run(text, **kw):
    return logsum.summarize(text, **kw)


class LogsumTests(unittest.TestCase):
    def test_phases_with_times_and_lines(self):
        out = '\n'.join(run(BOOT))
        self.assertIn('23:36:24 L2 java boot', out)
        self.assertIn('23:36:29 L3 resource reload', out)
        self.assertIn('23:36:29 L4 mods loaded', out)
        self.assertIn('23:36:46 L5 joined', out)
        self.assertIn('exit: not reached', out)

    def test_exit_takes_last_match(self):
        text = BOOT + 'Horizon: the JVM is shutting down (System.exit)\nHorizon: the JVM is shutting down (again)\n'
        out = '\n'.join(run(text))
        self.assertIn('L8 exit', out)

    def test_frames_skip_internals_and_count(self):
        out = run(NARRATOR + ITEM_FAIL)
        text = '\n'.join(out)
        self.assertIn('at com.mojang.text2speech.NarratorLinux$FliteLibrary.loadNative', text)
        self.assertIn('at net.minecraft.client.GameNarrator.<init>', text)
        self.assertNotIn('at java.lang.Thread.run', text)
        self.assertNotIn('org.spongepowered', text)
        self.assertNotIn('at dev.horizonlab.javamods.boot.Boot.main', text)  # the 4th kept frame is cut
        self.assertIn('(+5 frames)', text)
        self.assertIn('(+2 frames)', text)
        self.assertIn('at dev.horizonlab.javamods.boot.ItemText.take', text)
        self.assertIn('Horizon: an item\'s text failed | java.lang.IllegalStateException: no client level', text)

    def test_cause_and_suppressed_stay_in_one_entry(self):
        out = run(NARRATOR)
        self.assertIn('== Errors (1 distinct, 1) ==', out)

    def test_known_flagged_and_sorted_last(self):
        out = run(NARRATOR + ITEM_FAIL)
        heads = [x for x in out if x.startswith('x1 ')]
        self.assertIn('[known: narrator]', heads[1])
        self.assertNotIn('[known', heads[0])

    def test_errors_merge_ignoring_numbers_and_addresses(self):
        text = ('[1:00:00] x\n'
                '[10:00:01] [Server thread/ERROR]: Failed chunk 12 at 0xdead\n'
                '\tat dev.mod.A.f(A.java:1)\n'
                '[10:00:02] [Server thread/ERROR]: Failed chunk 99 at 0xbeef\n'
                '\tat dev.mod.A.f(A.java:1)\n')
        out = run(text)
        self.assertIn('== Errors (1 distinct, 2) ==', out)
        self.assertTrue(any(x.startswith('x2 L2 ') for x in out))
        self.assertEqual(sum(x.startswith('    at dev.mod.A.f') for x in out), 1)

    def test_warnings_merge_and_flag_known(self):
        out = run(WARNINGS)
        self.assertIn('== Warnings (4 distinct, 5) ==', out)
        merged = [x for x in out if 'Failed to retrieve username' in x and x.startswith(' ')]
        self.assertEqual(len(merged), 1)
        self.assertIn('2 x', merged[0])
        self.assertIn('UUID U', merged[0])
        self.assertTrue(any(x.startswith(' ') and 'OFFLINE/INSECURE' in x and '[known: offline mode]' in x for x in out))

    def test_noise_counts_and_known(self):
        out = run(NOISE)
        self.assertTrue(any(x.strip().startswith('5 x Horizon: use kind=N result=Pass[] held=N') for x in out))
        self.assertTrue(any(x.strip().startswith('2 x') and 'Start loading plugin' in x and '[known: jade]' in x for x in out))

    def test_rust_panic_is_an_error(self):
        text = ("thread 'main' panicked at src/x.rs:1:2:\nboom\nstack backtrace:\n   0: rust_begin_unwind\n"
                "   1: horizon::thing\n             at src/x.rs:1\n   2: core::panicking::panic\nafter\n")
        out = '\n'.join(run(text))
        self.assertIn("panicked at", out)
        self.assertIn('at horizon::thing', out)
        self.assertNotIn('at core::panicking', out)

    def test_truncation(self):
        long = 'Horizon: ' + 'x' * 10000
        out = run(f'[10:00:00] [Thread-0/ERROR]: {"y" * 500}\n{long}\n')
        self.assertTrue(all(len(x) <= 330 for x in out))
        self.assertTrue(any(x.endswith('…(+210 chars)') or '…(+' in x for x in out))
        self.assertEqual(logsum.cut('a' * 305), 'a' * 300 + '…(+5 chars)')
        self.assertEqual(logsum.cut('a' * 300), 'a' * 300)

    def test_last_ten_lines(self):
        text = ''.join(f'line {n}\n' for n in range(50))
        out = run(text)
        self.assertEqual(out[-10:], [f'line {n}' for n in range(40, 50)])

    def test_max_limits_output(self):
        text = NARRATOR + WARNINGS + NOISE + BOOT
        out = run(text, max_lines=25)
        self.assertLessEqual(len(out), 25)

    def test_grep_merges_and_cuts(self):
        text = NOISE + 'unrelated\n' + 'Horizon: use kind=7 ' + 'z' * 400 + '\n'
        out = run(text, grep_rx=r'use kind')
        self.assertTrue(out[0].startswith('== grep'))
        self.assertNotIn('unrelated', '\n'.join(out))
        self.assertTrue(any(x.startswith('L1 x5 ') for x in out))
        self.assertTrue(any('…(+' in x for x in out))

    def test_since_starts_at_first_match(self):
        out = run(BOOT, since=r'Reloading ResourceManager')
        text = '\n'.join(out)
        self.assertIn('L1 resource reload', text)
        self.assertIn('java boot: not reached', text)
        self.assertEqual(run(BOOT, since='nothing here'), ["--since 'nothing here': no line matches"])

    def test_cli(self):
        with tempfile.TemporaryDirectory() as d:
            p = Path(d) / 'g.log'
            p.write_text(BOOT)
            root = Path(__file__).resolve().parent.parent
            r = subprocess.run([sys.executable, '-I', str(root / 'tools' / 'logsum.py'), str(p), '--max', '30'],
                               capture_output=True, text=True, check=True)
            self.assertIn('== Phases ==', r.stdout)


if __name__ == '__main__':
    unittest.main()
