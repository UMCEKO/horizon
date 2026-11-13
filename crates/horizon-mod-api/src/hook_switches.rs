//! Per-hook switches of installed trait mods: one subscription turned off at run time, as `set_mod_enabled` turns off a whole mod.
use std::sync::atomic::{AtomicBool, Ordering};

/// Whether each installed mod's subscription to each hook is switched off.
pub(crate) struct HookSwitches {
    mods: usize,
    off: Box<[AtomicBool]>,
}

impl HookSwitches {
    pub(crate) fn new(hooks: usize, mods: usize) -> Self {
        Self {
            mods,
            off: (0..hooks * mods).map(|_| AtomicBool::new(false)).collect(),
        }
    }

    /// Whether mod `index`'s subscription to `hook` is on (the default).
    pub(crate) fn on(&self, hook: usize, index: usize) -> bool {
        self.off
            .get(hook * self.mods + index)
            .is_none_or(|off| !off.load(Ordering::Relaxed))
    }

    fn set(&self, hook: usize, index: usize, on: bool) {
        if let Some(off) = self.off.get(hook * self.mods + index) {
            off.store(!on, Ordering::Relaxed);
        }
    }
}

/// Switches every installed mod `id`'s subscription to `hook` on or off; whether one subscribes to it.
pub(crate) fn set_hook_enabled(id: &str, hook: crate::generated::HookId, on: bool) -> bool {
    let Some(installed) = super::installed() else {
        return false;
    };
    let mut found = false;
    for (index, entry) in installed.mods.iter().enumerate() {
        if entry.id == id && installed.subscribers.contains(hook as usize, index) {
            installed.switches.set(hook as usize, index, on);
            found = true;
        }
    }
    found
}
