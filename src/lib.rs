//! Independent WebAssembly binding layer for Blaze Cache.
//!
//! This crate is intentionally standalone: it does **not** depend on the existing
//! `crates/blaze-cache-wasm` crate or on the `wasm-bindgen` glue shipped by the Blaze Cache
//! monorepo. The binding is a flat raw ABI — `loom-abi-v2`, `ABI_VERSION = 1` — selected in
//! 0.2.0; see [`docs/plan/0.2.0.md`](../../docs/plan/0.2.0.md) and
//! [`docs/decisions/0004-abi-v2.md`](../../docs/decisions/0004-abi-v2.md).
//!
//! The crate is `deny(unsafe_code)`, with one documented exception class: each flat
//! export's `#[no_mangle]` attribute (a symbol-collision hazard Rust classifies as
//! unsafe). Host-supplied pointers and lengths are validated in safe Rust before a slice
//! or view is made — see [`abi`].
//!
//! # Status
//!
//! 0.2.0 is the first release with a working cache behind the flat ABI: the vendored
//! [`engine`] module answers `get`/`set`/`has`/`delete`/`stats` through exports whose
//! contract is `loom-abi-v2` — see
//! [`docs/decisions/blaze-v2.loom`](../../docs/decisions/blaze-v2.loom).

#![deny(unsafe_code)]

/// The flat raw-ABI surface: every `#[no_mangle]` export plus the shared [`abi::ErrorCode`].
/// One module is the crate's entire boundary — see its docs for the `unsafe_code`
/// exception, the borrow order and the epoch contract.
pub mod abi;

/// The scratch arena behind every host `(ptr, len)` — private: it is an implementation
/// detail of the flat ABI, not part of the surface.
mod arena;

/// Cache ids, slots and the registry — private: same reasoning as [`arena`].
mod cache;

/// Host-driven monotonic clock (wasm has no clock of its own; `std::time::Instant` traps
/// there and `web-time` would pull in wasm-bindgen — ADR-0002).
pub mod clock;

/// The vendored `blaze-cache-core` engine — a pinned upstream snapshot (ADR-0002,
/// [`engine-provenance.md`](../../docs/decisions/engine-provenance.md)).
pub mod engine;

/// The binding ABI version. Bumped whenever the exported surface changes incompatibly.
///
/// `1` corresponds to the `loom-abi-v2` contract described in
/// [`docs/decisions/0004-abi-v2.md`](../../docs/decisions/0004-abi-v2.md) and specified by
/// [`docs/decisions/blaze-v2.loom`](../../docs/decisions/blaze-v2.loom).
pub const ABI_VERSION: u32 = 1;

/// Re-exported so `blaze_cache_wasm::__blaze_abi_version` keeps resolving; the symbol
/// itself is defined in [`abi`], alongside the rest of the flat surface.
pub use abi::__blaze_abi_version;

#[cfg(test)]
mod tests {
    use super::{__blaze_abi_version, ABI_VERSION};

    #[test]
    fn abi_version_is_pinned() {
        assert_eq!(ABI_VERSION, 1, "0.2.0 is loom-abi-v2");
    }

    #[test]
    fn abi_version_matches_the_flat_export() {
        assert_eq!(
            __blaze_abi_version(),
            ABI_VERSION,
            "the flat export agrees with the constant"
        );
    }
}
