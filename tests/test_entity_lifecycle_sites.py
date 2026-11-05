"""Every native entity enters and leaves a level through game/entity_life.rs (SPEC-1b)."""

import re
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GAME = ROOT / "crates" / "horizon-client" / "src"

VECTORS = (
    r"(?:mobs|dropped|orbs|creatures\.projectiles|armor_stands|item_frames|paintings"
    r"|boats|carts|others|crystals|cushions)"
)
MUTATION = re.compile(
    rf"self\.(?:game\.)?{VECTORS}\.(?:push|insert|extend|append|remove|swap_remove|retain"
    r"|clear|drain|pop|truncate|retain_mut|dedup)\b|self\.dragon = |self\.dragon\.take\(\)"
)
FUNCTION = re.compile(r"\bfn (\w+)")

# (file under game/, function) -> why a direct mutation is not an add or a remove.
ALLOWED = {
    ("entity_life.rs", "*"): "the one place that adds and removes",
    ("creatures.rs", "tick_creature_entities"): "puts back the mob vector the loop took",
    ("creatures.rs", "tick_projectiles"): "puts back the projectile vector the loop took",
    ("entity.rs", "boat"): "a test fixture filling the lists",
}
ALLOWED_FILES = ("game_tests.rs", "command_harness.rs")


def sources():
    yield GAME / "game.rs"
    yield from sorted((GAME / "game").rglob("*.rs"))


def enclosing(lines, index):
    for j in range(index, -1, -1):
        found = FUNCTION.search(lines[j])
        if found:
            return found.group(1)
    return ""


class EntityLifecycleSites(unittest.TestCase):
    def test_no_direct_mutation_of_an_entity_vector_outside_the_allowlist(self):
        bad = []
        for path in sources():
            rel = path.relative_to(GAME / "game") if path.parent != GAME else Path(path.name)
            name = rel.as_posix()
            if path.name in ALLOWED_FILES or path.name.endswith("_fixture.rs"):
                continue
            lines = path.read_text().split("\n")
            for i, line in enumerate(lines):
                code = line.split("//")[0]
                if not MUTATION.search(code):
                    continue
                function = enclosing(lines, i)
                if (name, "*") in ALLOWED or (name, function) in ALLOWED:
                    continue
                bad.append(f"{name}:{i + 1} in {function}: {line.strip()}")
        self.assertEqual(bad, [], "use level_add / level_remove (game/entity_life.rs)")

    def test_every_allowlist_entry_still_matches(self):
        for (name, function), why in ALLOWED.items():
            self.assertTrue(why)
            path = GAME / "game" / name
            self.assertTrue(path.exists(), name)
            if function == "*":
                continue
            lines = path.read_text().split("\n")
            hits = [
                i
                for i, line in enumerate(lines)
                if MUTATION.search(line.split("//")[0]) and enclosing(lines, i) == function
            ]
            self.assertTrue(hits, f"{name}:{function} no longer mutates a vector")


if __name__ == "__main__":
    unittest.main()
