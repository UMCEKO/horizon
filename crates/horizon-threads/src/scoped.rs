//! A borrow that code without access to it can read while a closure runs.
use std::cell::Cell;

/// Holds a reference for the span of [`ScopedRef::scope`]; not `Send`, so the reads stay on the thread that set it.
pub struct ScopedRef<T> {
    slot: Cell<*const T>,
}

impl<T> Default for ScopedRef<T> {
    fn default() -> Self {
        Self {
            slot: Cell::new(std::ptr::null()),
        }
    }
}

impl<T> ScopedRef<T> {
    /// Runs `body` with `value` readable through [`ScopedRef::with`], and unset again after it (unwinding too).
    pub fn scope<R>(&self, value: &T, body: impl FnOnce() -> R) -> R {
        struct Restore<'a, T>(&'a Cell<*const T>, *const T);
        impl<T> Drop for Restore<'_, T> {
            fn drop(&mut self) {
                self.0.set(self.1);
            }
        }
        let _restore = Restore(&self.slot, self.slot.replace(value));
        body()
    }

    /// Reads the innermost value of a running [`ScopedRef::scope`], or `None` outside one.
    pub fn with<R>(&self, read: impl FnOnce(Option<&T>) -> R) -> R {
        // SAFETY: the slot holds a pointer only while the `scope` that set it runs, on this thread, with the reference alive.
        read(unsafe { self.slot.get().as_ref() })
    }
}

#[cfg(test)]
mod tests {
    use super::ScopedRef;

    #[test]
    fn reads_see_the_innermost_scope_and_nothing_outside_one() {
        let cell = ScopedRef::<u32>::default();
        assert_eq!(cell.with(|v| v.copied()), None);
        let seen = cell.scope(&1, || {
            let inner = cell.scope(&2, || cell.with(|v| v.copied()));
            (inner, cell.with(|v| v.copied()))
        });
        assert_eq!(seen, (Some(2), Some(1)));
        assert_eq!(cell.with(|v| v.copied()), None);
    }

    #[test]
    fn a_panicking_body_leaves_the_slot_unset() {
        let cell = ScopedRef::<u32>::default();
        let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            cell.scope(&7, || panic!("body"));
        }));
        assert!(caught.is_err());
        assert_eq!(cell.with(|v| v.copied()), None);
    }
}
