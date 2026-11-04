"""The static bridge check's two fixes: equal descriptors need no converter, and client classes resolve."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "tools"))
import java_mods  # noqa: E402

ENTITY = "Lnet/minecraft/world/entity/Entity;"


class BridgeCheck(unittest.TestCase):
    def test_a_string_return_that_equals_the_native_needs_no_converter(self):
        self.assertIsNone(java_mods.return_problem("Ljava/lang/String;", "Ljava/lang/String;", set()))
        self.assertIsNone(java_mods.return_problem("[B", "[B", set()))

    def test_an_object_return_needs_its_converter_and_a_primitive_must_agree(self):
        self.assertIn("toEntity", java_mods.return_problem(ENTITY, "I", set()))
        self.assertIsNone(java_mods.return_problem(ENTITY, "I", {("toEntity", f"(I){ENTITY}")}))
        self.assertIsNone(java_mods.return_problem("Z", "Z", set()))
        self.assertIn("against", java_mods.return_problem("I", "Z", set()))

    def test_a_class_only_the_client_jar_has_resolves_and_the_server_one_wins(self):
        server = {"a/Server": ("a/Server", [], [("m", "()V")])}
        client = [("a/Client", [], [("m", "()V")]), ("a/Server", [], [("other", "()V")])]
        java_mods.add_missing_classes(server, client)
        self.assertIn("a/Client", server)
        self.assertEqual(server["a/Server"][2], [("m", "()V")])


if __name__ == "__main__":
    unittest.main()
