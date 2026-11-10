import subprocess
import tempfile
import unittest
from pathlib import Path

from tools import mod_replaces as mr
from tools import mod_routing as mrt
from tests.test_mod_entity_facts import SOURCES as BASE
from tests.test_mod_replaces import classes_under, javac, jar

RT = 'net/minecraft/world/rt/'
TARGET = 'net.minecraft.world.rt.Target'
SOURCES = dict(BASE)
SOURCES.update({
    RT + 'Target.java': '''package net.minecraft.world.rt;
        public class Target {
          public void head(int a) {}
          public void headLocal(int a) {}
          public int tail(int a) { if (a > 0) return 1; return 2; }
          public void wrapped() { helper(); }
          public void mixed() { helper(); }
          public void varArg(int a) {}
          public void varNo(int a) {}
          public void run() { Runnable r = () -> helper(); r.run(); }
          public void hook() {}
          public void body() { bridged(); net.minecraft.world.entity.Entity e = null; e.setPos(1, 2, 3); pure(); head(1); }
          public void bridged() {}
          public void pure() { pure2(); }
          public void pure2() {}
          public void weak() {}
          public void helper() {}
          public static void util() {}
        }''',
    RT + 'S.java': 'package net.minecraft.world.rt; public class S extends Target { public void hook() { super.hook(); } }',
    RT + 'T.java': 'package net.minecraft.world.rt; public class T extends Target { public void hook() {} }',
    RT + 'U.java': 'package net.minecraft.world.rt; public class U extends S { public void hook() { super.hook(); } }',
    RT + 'V.java': 'package net.minecraft.world.rt; public class V extends T { public void hook() { super.hook(); } }',
    'org/spongepowered/asm/mixin/injection/ModifyVariable.java': ('package org.spongepowered.asm.mixin.injection; import java.lang.annotation.*; @Retention(RetentionPolicy.RUNTIME) '
                                                                  'public @interface ModifyVariable { String[] method(); At at(); boolean argsOnly() default false; }'),
    'com/llamalad7/mixinextras/injector/wrapoperation/Operation.java': 'package com.llamalad7.mixinextras.injector.wrapoperation; public interface Operation<T> { T call(Object... args); }',
    'com/llamalad7/mixinextras/injector/wrapoperation/WrapOperation.java': ('package com.llamalad7.mixinextras.injector.wrapoperation; import org.spongepowered.asm.mixin.injection.At; '
                                                                           'public @interface WrapOperation { String[] method(); At[] at(); }'),
    'fixture/mixin/TMixin.java': '''package fixture.mixin; import org.spongepowered.asm.mixin.*; import org.spongepowered.asm.mixin.injection.*; import org.spongepowered.asm.mixin.injection.callback.*;
        import com.llamalad7.mixinextras.injector.wrapoperation.*;
        @Mixin(net.minecraft.world.rt.Target.class) public class TMixin {
          @Inject(method = "head", at = @At("HEAD"), cancellable = true) void headInject(int a, CallbackInfo ci) {}
          @Inject(method = "headLocal", at = @At("HEAD")) void headLocal(int a, CallbackInfo ci, int local) {}
          @Inject(method = "tail", at = @At("TAIL")) void tailInject(int a, CallbackInfoReturnable<Integer> ci) {}
          @WrapOperation(method = "wrapped", at = @At("INVOKE")) void wrap(Operation<Object> op) {}
          @Inject(method = "mixed", at = @At("HEAD")) void mixHead(CallbackInfo ci) {}
          @WrapOperation(method = "mixed", at = @At("INVOKE")) void mixWrap(Operation<Object> op) {}
          @ModifyVariable(method = "varArg", at = @At("HEAD"), argsOnly = true) int va(int a) { return a; }
          @ModifyVariable(method = "varNo", at = @At("HEAD")) int vn(int a) { return a; }
          @Inject(method = "lambda$run$0", at = @At("HEAD")) void inLambda(CallbackInfo ci) {}
          @Inject(method = "hook", at = @At("HEAD")) void hookHead(CallbackInfo ci) {}
          @WrapOperation(method = "body", at = @At("INVOKE")) void bodyWrap(Operation<Object> op) {}
          @Inject(method = "weak", at = @At("HEAD")) void weakHead(CallbackInfo ci) {}
        }''',
})
KEY = 'net/minecraft/world/rt/Target#'
ENTITY_SET_POS = 'net/minecraft/world/entity/Entity#setPos(DDD)V'
LEDGER = {
    'methods': {},
    'fns': [{'file': 'crates/a.rs', 'line': 1}],
    'ported': {KEY + n: [0] for n in ('head', 'headLocal', 'tail', 'wrapped', 'mixed', 'varArg', 'varNo', 'run', 'hook', 'body', 'bridged', 'pure2')} | {'net/minecraft/world/entity/Entity#setPos': [0]},
    'mentioned': {KEY + 'weak': {'note': 'a comment'}},
}
BRIDGES = {('net/minecraft/world/rt/Target', 'bridged', '()V')}


def mixin(handler, method, at, kind='inject', config='fix.mixins.json'):
    return {'mod': 'fix', 'config': config, 'mixin': 'fixture.mixin.TMixin', 'handler': handler, 'kind': kind, 'target_class': [TARGET],
            'target_method': [method], 'at': at, 'side': 'common', 'access': False}


ENTRIES = [
    mixin('headInject', 'head', ['HEAD']), mixin('headLocal', 'headLocal', ['HEAD']), mixin('tailInject', 'tail', ['TAIL']),
    mixin('wrap', 'wrapped', ['INVOKE:Lnet/minecraft/world/rt/Target;helper()V'], 'wrap_operation'),
    mixin('mixHead', 'mixed', ['HEAD']), mixin('mixWrap', 'mixed', ['INVOKE:Lnet/minecraft/world/rt/Target;helper()V'], 'wrap_operation'),
    mixin('va', 'varArg', ['HEAD'], 'modify_variable'), mixin('vn', 'varNo', ['HEAD'], 'modify_variable'),
    mixin('inLambda', 'lambda$run$0', ['HEAD']), mixin('hookHead', 'hook', ['HEAD']),
    mixin('bodyWrap', 'body', ['INVOKE:Lnet/minecraft/world/rt/Target;pure()V'], 'wrap_operation'), mixin('weakHead', 'weak', ['HEAD']),
    mixin('wm', 'pure2', [], 'wrap_method'),
]


@unittest.skipUnless(javac(), 'needs javac')
class Routing(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.tmp = tempfile.TemporaryDirectory()
        root = Path(cls.tmp.name)
        src, out = root / 'src', root / 'classes'
        for name, text in SOURCES.items():
            (src / name).parent.mkdir(parents=True, exist_ok=True)
            (src / name).write_text(text)
        subprocess.run([javac(), '-proc:none', '-nowarn', '-d', str(out), *map(str, src.rglob('*.java'))], check=True, capture_output=True)
        install = root / 'install'
        libs = install / 'libraries' / 'net' / 'neoforged'
        jar(libs / 'minecraft-client-patched' / '1' / 'minecraft-client-patched-1.jar', classes_under(out, 'net/minecraft/'))
        jar(libs / 'minecraft-server-patched' / '1' / 'minecraft-server-patched-1.jar', {'placeholder.txt': b''})
        jar(libs / 'neoforge' / '1' / 'neoforge-1-universal.jar', classes_under(out, 'net/neoforged/neoforge/'))
        jar(libs / 'fancymodloader' / 'loader' / '1' / 'loader-1.jar', classes_under(out, 'net/neoforged/fml/', 'net/neoforged/neoforgespi/'))
        jar(libs / 'bus' / '1' / 'bus-1.jar', classes_under(out, 'net/neoforged/bus/'))
        files = classes_under(out, 'fixture/', 'com/llamalad7/')
        files['META-INF/neoforge.mods.toml'] = '[[mods]]\nmodId = "fix"\n[[mixins]]\nconfig = "fix.mixins.json"\n'
        files['fix.mixins.json'] = '{"package": "fixture.mixin", "mixins": []}'
        jar(install / 'mods' / 'fix.jar', files)
        cls.install, cls.root = install, root
        cls.result = cls.analyse({})
        cls.report = cls.result['routing']
        cls.routes = cls.report['routes']

    @classmethod
    def analyse(cls, verdicts):
        return mr.analyse(cls.install, cls.root / 'out', LEDGER, entries=ENTRIES, host=[], log=lambda *_: None, verdicts=verdicts, bridges=BRIDGES)

    @classmethod
    def tearDownClass(cls):
        cls.tmp.cleanup()

    def route(self, name, desc):
        return self.routes[f'{KEY}{name}{desc}']

    def test_a_head_inject_with_the_targets_parameters_is_an_edge_and_a_captured_local_makes_a_body(self):
        head = self.route('head', '(I)V')
        self.assertEqual((head['kind'], head['family'], head['args'], head['ret'], head['static']), ('edge', 'value', ['I'], 'V', False))
        self.assertEqual((head['changes'][0]['cancellable'], head['changes'][0]['locals']), (True, False))
        local = self.route('headLocal', '(I)V')
        self.assertEqual(local['kind'], 'body')
        self.assertTrue(local['changes'][0]['locals'])

    def test_a_tail_inject_is_an_edge_that_counts_the_vanilla_returns(self):
        tail = self.route('tail', '(I)I')
        self.assertEqual((tail['kind'], tail['tail'], tail['returns']), ('edge', True, 2))
        self.assertFalse(self.route('head', '(I)V')['tail'])

    def test_a_wrap_operation_is_a_body_and_so_is_a_key_that_also_has_a_head_inject(self):
        self.assertEqual(self.route('wrapped', '()V')['kind'], 'body')
        mixed = self.route('mixed', '()V')
        self.assertEqual((mixed['kind'], sorted(c['kind'] for c in mixed['changes'])), ('body', ['inject', 'wrap_operation']))

    def test_modify_variable_is_an_edge_only_at_head_with_args_only(self):
        self.assertEqual(self.route('varArg', '(I)V')['kind'], 'edge')
        self.assertEqual(self.route('varNo', '(I)V')['kind'], 'body')

    def test_wrap_method_is_an_edge(self):
        self.assertEqual(self.route('pure2', '()V')['kind'], 'edge')

    def test_a_mixin_into_a_lambda_routes_the_enclosing_method_as_a_body(self):
        run = self.route('run', '()V')
        self.assertEqual((run['kind'], run['changes'][0]['lambda']), ('body', 'lambda$run$0()V'))
        self.assertEqual(self.report['lambdas'], {f'{KEY}lambda$run$0()V': f'{KEY}run()V'})
        self.assertNotIn(f'{KEY}lambda$run$0()V', self.routes)

    def test_overriders_say_whether_they_reach_the_base_body_through_super(self):
        found = {o['class'].rsplit('/', 1)[-1]: o['super_call'] for o in self.route('hook', '()V')['overriders']}
        self.assertEqual(found, {'S': True, 'T': False, 'U': True, 'V': False})

    def test_the_closure_of_a_body_route_sorts_bridged_view_routed_and_pure_callees(self):
        closure = self.route('body', '()V')['closure']
        self.assertEqual(closure['bridged'], [f'{KEY}bridged()V'])
        self.assertEqual(closure['needs_bridge'], [ENTITY_SET_POS])
        self.assertEqual(closure['routed'], [f'{KEY}head(I)V', f'{KEY}pure2()V'])
        self.assertEqual(closure['pure'], [f'{KEY}pure()V'])
        self.assertEqual(self.route('head', '(I)V')['closure'], {'bridged': [], 'pure': [], 'routed': [], 'needs_bridge': []})

    def test_a_weak_status_is_undecided_until_the_verdict_map_answers(self):
        self.assertEqual(self.report['undecided'], {f'{KEY}weak()V': {'status': 'mentioned', 'mods': ['fix']}})
        self.assertNotIn(f'{KEY}weak()V', self.routes)
        ported = self.analyse({'Target.weak': 'port'})['routing']
        self.assertEqual(ported['undecided'], {})
        self.assertEqual(ported['routes'][f'{KEY}weak()V']['verdict'], 'port')
        skipped = self.analyse({'Target.weak': 'missing'})
        self.assertEqual((skipped['routing']['undecided'], skipped['methods'][f'{KEY}weak']['changes'][0]['note']), ({}, 'native lacks it'))
        self.assertNotIn(f'{KEY}weak()V', skipped['routing']['routes'])

    def test_the_tsv_and_the_reference_list_carry_no_handler_names(self):
        rows = [line.split('\t') for line in mrt.tsv(self.report).splitlines()]
        self.assertIn(['net/minecraft/world/rt/Target', 'tail', '(I)I', 'edge', '1', '2'], rows)
        listing = mrt.routes_tsv(self.report, 'pack')
        self.assertEqual(listing.splitlines()[0], '# routes of pack, mod_routing format 1')
        self.assertIn(f'{KEY}head(I)V\tedge\tvalue\t0\t1\tfix\t0', listing)
        self.assertNotIn('headInject', listing)


if __name__ == '__main__':
    unittest.main()
