package dev.horizonlab.javamods.boot;

/** Whose particles Java keeps: scopes, a mod's frame on the stack, a mod's class. */
public final class ClientParticlesCheck {
    private static void check(boolean ok, String what) {
        if (!ok) throw new AssertionError(what);
    }

    public static void main(String[] args) {
        check(!ClientParticles.sourced(), "no scope, no mod frame");
        ClientParticles.enter();
        check(ClientParticles.sourced(), "inside a scope");
        ClientParticles.enter();
        ClientParticles.exit();
        check(ClientParticles.sourced(), "a nested scope needs both exits");
        ClientParticles.exit();
        check(!ClientParticles.sourced(), "after the scope");
        check(fakemod.Caller.call(ClientParticles::sourced), "a mod's frame on the stack");
        check(ClientParticles.keeps(fakemod.FakeProvider.class), "a mod's class is kept");
        check(!ClientParticles.keeps(String.class), "a vanilla class outside a scope is dropped");
    }
}
