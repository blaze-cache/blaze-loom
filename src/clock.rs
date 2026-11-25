//! Host-driven monotonic clock — wasm32 has no clock of its own.
//!
//! Two facts force this design:
//!
//! * `std::time::Instant::now()` **traps** on `wasm32-unknown-unknown` (the target has no
//!   clock import; the call compiles to `unreachable`).
//! * `web-time`'s wasm implementation is built on `wasm-bindgen` + `js-sys`, which would
//!   pour thousands of `__wbindgen_*` imports into the module — exactly the generated glue
//!   ADR-0001 rules out, and fatal to `WebAssembly.instantiate(bytes, {})`.
//!
//! So on wasm, time is **host-provided**: the host calls `__blaze_tick(now_ms)` before each
//! ABI call and [`Instant::now`] returns the last ticked value. The engine's own
//! `use web_time::Instant;` lines are mechanically rewritten to `use crate::clock::Instant;`
//! by `scripts/sync-core.sh` (recorded in `docs/decisions/engine-provenance.md`).
//!
//! Off-wasm this is plain [`std::time::Instant`] — which is also what `web_time::Instant`
//! re-exports there — so native behaviour is unchanged.

#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub use std::time::Instant;

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
mod wasm {
    use core::cell::Cell;
    use core::ops::{Add, Sub};
    use core::time::Duration;

    thread_local! {
        static NOW_NS: Cell<u64> = const { Cell::new(0) };
    }

    /// A monotonic instant backed by the host-ticked clock, in nanoseconds since an
    /// arbitrary epoch. Comparisons and arithmetic are plain `u64`, so ordering is exact.
    #[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
    pub struct Instant(u64);

    impl Instant {
        /// The last time the host ticked; `0` until it does (time is frozen, not broken).
        pub fn now() -> Instant {
            Instant(NOW_NS.with(Cell::get))
        }

        pub fn duration_since(&self, earlier: Instant) -> Duration {
            Duration::from_nanos(self.0.saturating_sub(earlier.0))
        }

        pub fn checked_duration_since(&self, earlier: Instant) -> Option<Duration> {
            self.0.checked_sub(earlier.0).map(Duration::from_nanos)
        }

        pub fn saturating_duration_since(&self, earlier: Instant) -> Duration {
            self.duration_since(earlier)
        }

        pub fn elapsed(&self) -> Duration {
            Self::now().saturating_duration_since(*self)
        }

        pub fn checked_add(&self, duration: Duration) -> Option<Instant> {
            let ns = u64::try_from(duration.as_nanos()).ok()?;
            self.0.checked_add(ns).map(Instant)
        }

        pub fn checked_sub(&self, duration: Duration) -> Option<Instant> {
            let ns = u64::try_from(duration.as_nanos()).ok()?;
            self.0.checked_sub(ns).map(Instant)
        }
    }

    impl Add<Duration> for Instant {
        type Output = Instant;
        fn add(self, duration: Duration) -> Instant {
            let ns = u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX);
            Instant(self.0.saturating_add(ns))
        }
    }

    impl Sub<Duration> for Instant {
        type Output = Instant;
        fn sub(self, duration: Duration) -> Instant {
            let ns = u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX);
            Instant(self.0.saturating_sub(ns))
        }
    }

    impl Sub<Instant> for Instant {
        type Output = Duration;
        fn sub(self, earlier: Instant) -> Duration {
            self.duration_since(earlier)
        }
    }

    /// Advances the clock; called by [`super::set_now_ms`].
    pub(super) fn set_now_ns(ns: u64) {
        NOW_NS.with(|cell| cell.set(ns));
    }
}

#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub use wasm::Instant;

/// Advances the wasm clock to `now_ms` (milliseconds, monotonic — `performance.now()` or
/// `Date.now()` both work). Called by the flat export `__blaze_tick`; a no-op off-wasm,
/// where [`Instant::now`] is the real clock.
#[cfg(all(target_family = "wasm", target_os = "unknown"))]
pub fn set_now_ms(now_ms: f64) {
    // `as` on f64→u64 saturates: negative → 0, non-finite/huge → u64::MAX.
    let ns = (now_ms * 1_000_000.0) as u64;
    wasm::set_now_ns(ns);
}

/// See [`set_now_ms`]; no-op off-wasm.
#[cfg(not(all(target_family = "wasm", target_os = "unknown")))]
pub fn set_now_ms(_now_ms: f64) {}
