import unittest

from tools import java_routing as jr

KEY = 'net/minecraft/world/entity/player/Inventory#setSelectedSlot(I)V'
TAILED = 'net/minecraft/world/entity/player/Inventory#tailed(I)I'
GOOD = '''impl Game<'_> {
    pub fn set_selected_slot(&mut self, slot: i32) {
        route!(self, "%s", Recv::Player, (slot));
        self.set_selected_slot_body(slot);
    }
    pub fn set_selected_slot_body(&mut self, slot: i32) { if slot > 0 { return route::early(()); } }
}
''' % KEY
PACK = {KEY: {'kind': 'edge', 'family': 'player', 'tail': False, 'returns': 1, 'mods': {'carryon'}}}
BRIDGE = ('net/minecraft/world/entity/player/Inventory', 'setSelectedSlot', '(I)V')


def problems(text, packs=PACK, deferred=(), bridges=(BRIDGE,), test_routes=(), gui=()):
    return jr.lint(jr.scan_text(text, 'f.rs'), packs, list(deferred), set(test_routes), set(bridges), set(gui))


class Lint(unittest.TestCase):
    def test_the_gate_list_is_fresh(self):
        self.assertEqual(0, jr.cmd_gates(type('A', (), {'check': True})()), 'run python3 tools/java_routing.py gates')

    def test_every_reference_route_is_gated_or_deferred(self):
        gates, packs, deferred, test_routes, bridges, gui_keys = jr.gather()
        self.assertEqual([], jr.lint(gates, packs, deferred, test_routes, bridges, gui_keys))

    def test_a_good_gate_passes(self):
        self.assertEqual([], problems(GOOD))

    def test_a_gate_not_first_fails(self):
        late = GOOD.replace('route!(self', 'let _x = 1; route!(self')
        self.assertTrue(any('gate not first in a Game method' in e for e in problems(late)))

    def test_a_missing_gate_is_an_unrouted_key(self):
        bare = GOOD.replace(GOOD[GOOD.index('        route!'):GOOD.index('        self.set')], '')
        self.assertTrue(any('unrouted' in e for e in problems(bare)))

    def test_a_gate_outside_impl_game_fails(self):
        other = GOOD.replace("impl Game<'_>", 'impl Other')
        self.assertTrue(any('gate not first in a Game method' in e for e in problems(other)))

    def test_a_tail_key_needs_early(self):
        packs = {TAILED: {'kind': 'body', 'family': 'player', 'tail': True, 'returns': 2, 'mods': {'m'}}}
        text = GOOD.replace(KEY, TAILED).replace('route::early(())', '()')
        self.assertTrue(any('route::early' in e for e in problems(text, packs)))
        self.assertEqual([], problems(GOOD.replace(KEY, TAILED), packs))

    def test_an_edge_gate_needs_a_route_bridge(self):
        self.assertTrue(any('without a route bridge' in e for e in problems(GOOD, bridges=())))

    def test_a_stale_gate_is_reported_unless_the_test_mod_routes_it(self):
        self.assertTrue(any('stale gate' in e for e in problems(GOOD, packs={})))
        self.assertEqual([], problems(GOOD, packs={}, test_routes=[KEY]))

    def test_a_bad_reason_fails(self):
        self.assertTrue(any('bad deferral reason' in e for e in problems(GOOD, deferred=[('x#y()V', 'because')])))
        self.assertEqual([], problems(GOOD, deferred=[('x#y()V', 'seam: a/b.rs:3')]))

    def test_gated_and_deferred_is_an_error_unless_a_partial_seam(self):
        self.assertTrue(any('remove the deferral' in e for e in problems(GOOD, deferred=[(KEY, 'worldgen: W10')])))
        self.assertEqual([], problems(GOOD, deferred=[(KEY, 'seam: a/b.rs:3')]))

    def test_gui_markers_pair_with_rows(self):
        marker = 'fn draw() { gui_route!("a/B#c()V"); }'
        self.assertTrue(any('without a row' in e for e in problems(marker, packs={})))
        self.assertTrue(any('without a gui_route!' in e for e in problems(GOOD, gui=['a/B#c()V'])))
        self.assertEqual([], problems(marker, packs={}, gui=['a/B#c()V']))

    def test_the_partial_count_reaches_the_gate_list(self):
        text = jr.gates_tsv(jr.scan_text(GOOD, 'f.rs'), [(KEY, 'seam: a.rs:1'), (KEY, 'seam: b.rs:2')])
        self.assertIn('\tgame\t2\n', text)

    def test_an_installs_ungated_route_fails_the_lint(self):
        live = {'routes': {'a/B#live()V': {'kind': 'body', 'family': 'static', 'tail': False, 'returns': 1, 'changes': [{'mod': 'lootr'}]}}}
        packs = jr.merge_packs(PACK, jr.routing_packs(live))
        self.assertTrue(any(e.startswith('a/B#live()V: unrouted') for e in problems(GOOD, packs)))
        self.assertEqual([], problems(GOOD, packs, deferred=[('a/B#live()V', 'client: phase C')]))
        self.assertEqual(PACK[KEY]['mods'], jr.merge_packs(PACK, {})[KEY]['mods'])

    def test_lint_reads_each_install_routing_it_is_given(self):
        import json
        import tempfile
        with tempfile.NamedTemporaryFile('w', suffix='.json') as f:
            json.dump({'routes': {'a/B#live()V': {'kind': 'body', 'family': 'static', 'tail': False, 'returns': 1, 'changes': [{'mod': 'm'}]}}}, f)
            f.flush()
            args = type('A', (), {'ledger': False, 'routing': [f.name]})()
            self.assertEqual(1, jr.cmd_lint(args))
        self.assertEqual(0, jr.cmd_lint(type('A', (), {'ledger': False, 'routing': None})()))

    def test_the_parser_reads_the_body_fn_across_strings(self):
        gate = jr.scan_text(GOOD, 'f.rs')[0]
        self.assertEqual(('set_selected_slot', 'set_selected_slot_body', True, True), (gate['fn'], gate['body'], gate['first'], gate['game']))


if __name__ == '__main__':
    unittest.main()
