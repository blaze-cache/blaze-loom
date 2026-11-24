//! Independent WebAssembly binding layer for Blaze Cache.
//!
//! This crate is intentionally standalone: it does **not** depend on the existing
//! `crates/blaze-cache-wasm` crate or on the `wasm-bindgen` glue currently shipped by the
//! Blaze Cache monorepo. The ABI strategy for this crate is a 0.1.0 design decision — see
//! [`docs/plan/0.1.0.md`](../../docs/plan/0.1.0.md).
//!
//! The crate is `deny(unsafe_code)` from the start, with one documented `#[no_mangle]` export
//! attribute exception: whatever ABI is chosen, host-supplied pointers and lengths must be
//! validated in safe Rust before a slice or view is made.
//!
//! # Status
//!
//! 0.1.0 is a foundation release. It establishes the crate, toolchain, safety policy and
//! project plan; no public cache API is promised yet.

#![deny(unsafe_code)]

/// The binding ABI version. Bumped whenever the exported surface changes incompatibly.
///
/// `0` corresponds to the frozen `loom-abi-v1` contract described in
/// [`docs/decisions/0001-abi-strategy.md`](../../docs/decisions/0001-abi-strategy.md).
pub const ABI_VERSION: u32 = 0;

/// The single flat-ABI export available at 0.1.0: a JS host checks this before making any
/// other call and fails loudly on skew. No generated glue and no Rust dependency is involved.
#[allow(unsafe_code)] // the only unsafe-attributed item: a flat raw-ABI export
#[no_mangle]
pub extern "C" fn __blaze_abi_version() -> u32 {
    ABI_VERSION
}

#[cfg(test)]
mod tests {
    use super::{__blaze_abi_version, ABI_VERSION};

    #[test]
    fn abi_version_is_pinned() {
        assert_eq!(ABI_VERSION, 0);
    }

    #[test]
    fn abi_version_is_documented_zero() {
        assert_eq!(ABI_VERSION, 0, "0.1.0 has a foundation ABI only");
        assert_eq!(
            __blaze_abi_version(),
            ABI_VERSION,
            "the flat export agrees with the constant"
        );
    }
}
