//! The `player_bed_leave` hook: declarations the mod API generator reads (see `hooks/mod.rs`).
// catalog-order: 0279

pub trait Hooks {
    /// A server player woke or stood up from the bed at `pos`, after their
    /// sleeping state changed. `wake_immediately` and `update_level` are the flags of vanilla's
    /// `stopSleepInBed` call: waking every player in the morning is (false, false), leaving the
    /// bed (false, true) and a bed that is gone (true, true).
    #[hook(
        side = "server",
        contract = "listen",
        neoforge = "PlayerWakeUpEvent",
        cancellable = false
    )]
    fn player_bed_leave();
}
