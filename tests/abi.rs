//! External-consumer view of the crate's 0.1.0 surface.
//!
//! These tests link against `blaze_cache_wasm` the way a crates.io consumer does, so a release
//! that drops or renames the flat export fails here as well as in the crate's own unit tests.

use blaze_cache_wasm::{__blaze_abi_version, ABI_VERSION};

#[test]
fn public_abi_version_is_zero() {
    assert_eq!(ABI_VERSION, 0);
}

#[test]
fn exported_version_matches_the_public_constant() {
    assert_eq!(__blaze_abi_version(), ABI_VERSION);
}

#[test]
fn export_has_the_flat_extern_c_signature() {
    let export: extern "C" fn() -> u32 = __blaze_abi_version;
    assert_eq!(export(), 0);
}
