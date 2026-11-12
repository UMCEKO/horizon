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


def problems(text, packs=PACK, deferred=(), bridges=(BRIDGE,), test_routes=(), gui=(), verdicts=()):
    return jr.lint(jr.scan_text(text, 'f.rs'), packs, list(deferred), set(test_routes), set(bridges), set(gui), list(verdicts))


class Lint(unittest.TestCase):
    def test_the_gate_list_is_fresh(self):
        self.assertEqual(0, jr.cmd_gates(type('A', (), {'check': True})()), 'run python3 tools/java_routing.py gates')

    def test_every_reference_route_is_gated_deferred_or_has_a_verdict(self):
        gates, packs, deferred, test_routes, bridges, gui_keys = jr.gather()
        self.assertEqual([], jr.lint(gates, packs, deferred, test_routes, bridges, gui_keys, jr.read_verdicts()))

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
        self.assertTrue(any('remove the deferral' in e for e in problems(GOOD, deferred=[(KEY, 'owner: W10')])))
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
        self.assertEqual([], problems(GOOD, packs, deferred=[('a/B#live()V', 'owner: W5')]))
        self.assertEqual(PACK[KEY]['mods'], jr.merge_packs(PACK, {})[KEY]['mods'])

    def test_a_route_inert_by_its_guards_needs_no_gate_and_counts_as_covered(self):
        guards = {'recv': None, 'recv_item': None, 'arg_item': None, 'arg_class': {'0': ['mod/Shape']}, 'arg_block': None, 'block_at': None, 'be_at': None}
        inert = {'kind': 'edge', 'family': 'static', 'tail': False, 'returns': 1, 'changes': [{'mod': 'm'}], 'closure': {'needs_bridge': []}, 'inert': True, 'guards': guards}
        routing = {'routes': {'a/B#join()V': inert}}
        self.assertEqual({}, jr.routing_packs(routing))
        table = jr.report_table(routing, gates=[], deferred=[], verdicts=[])
        self.assertEqual(table[0], '1 routes, 1 covered: 1 inert')
        self.assertIn('| `a/B#join()V` | edge | static | m | inert (guard) | arg_class[0]: Shape |  |', table)

    def test_a_constructor_route_takes_no_gate_or_deferral(self):
        ctor = 'a/B#<init>(I)V'
        built = {'kind': 'edge', 'family': 'worldgen', 'tail': False, 'returns': 1, 'changes': [{'mod': 'm'}], 'closure': {'needs_bridge': []}, 'inert': False,
                 'construct': {'by': 'deferred', 'reason': 'r'}}
        routing = {'routes': {ctor: built}}
        self.assertEqual({}, jr.routing_packs(routing))
        self.assertEqual([], problems(GOOD, jr.merge_packs(PACK, {ctor: {'kind': 'edge', 'family': 'worldgen', 'tail': False, 'returns': 1, 'mods': {'m'}}})))
        self.assertTrue(any(e.startswith(f'{ctor}: a constructor') for e in problems(GOOD, deferred=[(ctor, 'worldgen: W10')])))
        self.assertTrue(any('constructor (deferred)' in line for line in jr.report_table(routing, gates=[], deferred=[])))

    def test_lint_reads_each_install_routing_it_is_given(self):
        import json
        import tempfile
        with tempfile.NamedTemporaryFile('w', suffix='.json') as f:
            json.dump({'routes': {'a/B#live()V': {'kind': 'body', 'family': 'static', 'tail': False, 'returns': 1, 'changes': [{'mod': 'm'}]}}}, f)
            f.flush()
            args = type('A', (), {'ledger': False, 'routing': [f.name]})()
            self.assertEqual(1, jr.cmd_lint(args))
        self.assertEqual(0, jr.cmd_lint(type('A', (), {'ledger': False, 'routing': None})()))

    def test_owner_reasons_accepted(self):
        for reason in ('owner: W5', 'owner: W9 3-2', 'spec: w12-coverage SPEC-12'):
            self.assertTrue(jr.REASON.match(reason), reason)
        for reason in ('client: phase C', 'owner: W99', 'spec: SPEC-3'):
            self.assertFalse(jr.REASON.match(reason), reason)

    def test_verdict_and_gate_conflict(self):
        errors = problems(GOOD, verdicts=[(KEY, 'java-client', 'trace: t.jfr')])
        self.assertEqual([KEY + ': has a verdict and a gate or deferral'], errors)

    def test_verdict_needs_evidence(self):
        errors = problems(GOOD, packs={**PACK, 'a/B#c()V': PACK[KEY]}, verdicts=[('a/B#c()V', 'java-client', '')])
        self.assertEqual(['a/B#c()V: verdict java-client without evidence'], errors)

    def test_m2_line(self):
        keys = ['a/A#g1()V', 'a/A#g2()V', 'a/A#v()V', 'a/A#w()V', 'a/A#u()V']
        deferred = [('a/A#w()V', 'owner: W5')]
        verdicts = [('a/A#v()V', 'java-client', 'trace: t.jfr')]
        line, open_ = jr.m2('t', keys, {'a/A#g1()V', 'a/A#g2()V'}, deferred, verdicts)
        self.assertEqual('M2 t: 5 routes; covered 3 (2 gated + 1 verdict); owned 1 (W5 1); open 1', line)
        self.assertEqual(1, open_)
        pack = {key: PACK[KEY] for key in keys}
        self.assertEqual(['a/A#u()V: unrouted (no route! gate, no row in deferred.tsv or verdicts.tsv)'],
                         [e for e in problems('', packs=pack, deferred=deferred, verdicts=verdicts) if 'g1' not in e and 'g2' not in e])

    def test_owners_converter(self):
        rows = [['a/A#s()V', 'route', 'edge', 'lootr', 'SPEC-6'],
                ['a/A#w()V', 'route', 'body', 'lithostitched', 'W10'],
                ['a/A#c()V', 'route', 'edge', 'jade', 'java-client'],
                ['a/A#g()V', 'route', 'edge', 'carryon', 'gated']]
        deferred, verdicts = jr.owners_tables(rows)
        self.assertEqual('# key, reason; the reasons are fixed (tools/java_routing.py)\n'
                         'a/A#s()V\tspec: w12-coverage SPEC-6\n'
                         'a/A#w()V\towner: W10\n', deferred)
        self.assertEqual('# key, verdict, evidence (tools/java_routing.py owners)\n'
                         'a/A#c()V\tjava-client\ttrace: java_client_routes.jfr\n', verdicts)

    def test_trace_check_parses(self):
        printed = '''jdk.MethodTrace {
  method = net.minecraft.client.KeyMapping.click(InputConstants$Key)
  eventThread = "Render thread" (javaThreadId = 1)
jdk.MethodTrace {
  method = net.minecraft.client.Minecraft.startAttack()
  eventThread = "Server thread" (javaThreadId = 2)
'''
        keys = {'net/minecraft/client/KeyMapping#click(Lcom/mojang/blaze3d/platform/InputConstants$Key;)V': 'java-client',
                'net/minecraft/client/Minecraft#startAttack()Z': 'java-client'}
        self.assertEqual([('net/minecraft/client/KeyMapping#click(Lcom/mojang/blaze3d/platform/InputConstants$Key;)V', True),
                          ('net/minecraft/client/Minecraft#startAttack()Z', False)], jr.trace_check(keys, jr.trace_events(printed)))

    def test_the_parser_reads_the_body_fn_across_strings(self):
        gate = jr.scan_text(GOOD, 'f.rs')[0]
        self.assertEqual(('set_selected_slot', 'set_selected_slot_body', True, True), (gate['fn'], gate['body'], gate['first'], gate['game']))


if __name__ == '__main__':
    unittest.main()
