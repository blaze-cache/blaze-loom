//! The flat raw ABI — every `#[no_mangle]` export of this crate, and the error enum they
//! share. This module is the crate's entire boundary: nothing else reaches across it.
//!
//! # The `unsafe_code` exception
//!
//! The crate is `#![deny(unsafe_code)]`. The **only** thing allowed here is the
//! `#[no_mangle]` attribute itself — a symbol-collision hazard Rust classifies as unsafe —
//! one per export, each marked `#[allow(unsafe_code)]`. No `unsafe` block or call appears
//! anywhere in this file or in the engine behind it: every host `(ptr, len)` is validated
//! against [`crate::arena`] in safe Rust before a byte is read or written, and a bad pointer
//! degrades to [`ErrorCode::BadArg`] — never a trap, never a fabricated success.
//!
//! # Borrow order
//!
//! Both globals are thread-locals (`RefCell`), so "locking" means "borrowing", and no
//! borrow can ever deadlock — the only failure mode would be a double borrow, and the order
//! `REGISTRY` → `ARENA` (never the reverse) makes that impossible: every arena helper is
//! self-contained and never asks for the registry, while registry work may borrow the arena
//! from inside. An export reads its inputs with standalone arena borrows, then runs its
//! engine work under one [`cache::with_registry`] borrow — `get_sync` writes its outputs
//! inside that same borrow (the hot path must not clone a `Bytes` value just to move it
//! out), `stats` writes outside it. [`__blaze_arena_alloc`] and [`__blaze_generation`]
//! touch one global at most; [`__blaze_tick`] touches neither.
//!
//! # Epoch contract
//!
//! The six arena-touching exports (`open`, `get_sync`, `set`, `has`, `delete`, `stats`) call
//! [`crate::arena::end_epoch`] on the way in, arming the lazy reset that the next
//! [`__blaze_arena_alloc`] consumes. `close`, `generation` and `tick` do not touch the arena
//! and do not arm it. The host's side of the contract lives in [`crate::arena`]'s docs.

use crate::arena;
use crate::cache::{self, StoredValue};
use crate::clock;
use crate::engine::CacheStats;
use crate::ABI_VERSION;

/// The stable `i32` error enum from the plan (§ "Error codes"). Exports return the
/// **negated** code, so [`ErrorCode::Ok`] is `0` on the wire and every failure is negative.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ErrorCode {
    /// Operation succeeded.
    Ok = 0,
    /// Key absent or expired.
    Miss = 1,
    /// Out-of-bounds pointer/length, malformed config or tags, unknown value kind — or an
    /// id that was **never issued** (the plan's "bad id"; ids are monotonic from `1`).
    BadArg = 2,
    /// Generation advanced or `memory.buffer` swapped since the view was taken. No 0.2.0
    /// export can produce it: the generation check is the 0.3.0 runtime's job.
    #[allow(dead_code)]
    StaleView = 3,
    /// The id **was** issued and has since been `close`d (a tombstone still holds its
    /// generation, which is why this is distinct from [`ErrorCode::BadArg`]).
    Closed = 4,
    /// Arena/id-space allocation failed. `__blaze_arena_alloc` reports the same condition as
    /// a `0` return instead, per its signature.
    Oom = 5,
}

impl ErrorCode {
    /// The positive code (`Ok == 0`, `Miss == 1`, …) — what the plan's table lists.
    pub fn code(self) -> i32 {
        self as i32
    }

    /// What an export returns for this code: the negated value, with `Ok` staying `0`.
    pub fn as_return(self) -> i32 {
        -(self as i32)
    }
}

/// The plan's fixed stats layout: seven little-endian `u64` fields, in order
/// `hits, misses, evictions, expirations, invalidations, size, capacity` (`size` and
/// `capacity` are entry counts, widened from `usize`).
const STATS_LEN: usize = 56;

fn stats_bytes(stats: &CacheStats) -> [u8; STATS_LEN] {
    let fields: [u64; 7] = [
        stats.hits,
        stats.misses,
        stats.evictions,
        stats.expirations,
        stats.invalidations,
        stats.size as u64,
        stats.capacity as u64,
    ];
    let mut out = [0u8; STATS_LEN];
    for (field, chunk) in fields.iter().zip(out.chunks_exact_mut(8)) {
        chunk.copy_from_slice(&field.to_le_bytes());
    }
    out
}

/// The binding version. Still the first call the host makes; a mismatch throws in the host
/// before any other export is touched.
#[allow(unsafe_code)] // the #[no_mangle] exception
#[no_mangle]
pub extern "C" fn __blaze_abi_version() -> u32 {
    ABI_VERSION
}

/// Carve out `len` scratch bytes for the host to write into. `0` on failure (request larger
/// than what remains of the arena) — never a realloc, so every pointer ever returned stays
/// valid. Consumes a pending epoch reset rather than arming one: this call *is* the boundary
/// between operations.
#[allow(unsafe_code)] // the #[no_mangle] exception
#[no_mangle]
pub extern "C" fn __blaze_arena_alloc(len: u32) -> u32 {
    arena::alloc(len)
}

/// Open a cache from exactly 8 bytes of config: `capacity: u32` (clamped to ≥1), then
/// `eviction: u32` (`0` = Sieve, the only policy the pinned engine ships). Returns the id
/// (≥ 1) or `-code`.
#[allow(unsafe_code)] // the #[no_mangle] exception
#[no_mangle]
pub extern "C" fn __blaze_cache_open(config_ptr: u32, config_len: u32) -> i32 {
    arena::end_epoch();
    let config = match arena::read(config_ptr, config_len) {
        Ok(config) => config,
        Err(()) => return ErrorCode::BadArg.as_return(),
    };
    if config.len() != 8 {
        return ErrorCode::BadArg.as_return();
    }
    let capacity = u32::from_le_bytes([config[0], config[1], config[2], config[3]]);
    let eviction = u32::from_le_bytes([config[4], config[5], config[6], config[7]]);
    if eviction != 0 {
        return ErrorCode::BadArg.as_return();
    }
    cache::with_registry(|registry| match registry.open(capacity) {
        Ok(id) => id as i32,
        Err(code) => code.as_return(),
    })
}

/// Look up `key`.
///
/// Returns `0` and writes the value to `out_ptr` (plus its byte length, little-endian `u32`,
/// to `out_len_ptr`) — or `-1` on a miss, writing nothing. `out_ptr == 0 && out_cap == 0` is
/// probe mode: only the length is written, so the host can size its buffer first.
///
/// Every pointer argument is validated **before** the lookup, so argument validity never
/// depends on cache state: a bad pointer is `BAD_ARG` whether or not the key exists. A value
/// larger than `out_cap` is discovered at write time (after the lookup, which the engine
/// counts as the hit it was) and also returns `BAD_ARG` — the host resizes and retries.
#[allow(unsafe_code)] // the #[no_mangle] exception
#[no_mangle]
pub extern "C" fn __blaze_cache_get_sync(
    id: u32,
    key_ptr: u32,
    key_len: u32,
    out_ptr: u32,
    out_cap: u32,
    out_len_ptr: u32,
) -> i32 {
    arena::end_epoch();
    let key = match arena::read(key_ptr, key_len) {
        Ok(key) => key,
        Err(()) => return ErrorCode::BadArg.as_return(),
    };
    // Probe mode is `(0, 0)` — a zero-length region is always in bounds, so the check below
    // passes it for free; a non-zero `out_cap` at the null pointer is rejected like any
    // other out-of-bounds buffer.
    if let Err(()) = arena::check_out(out_ptr, out_cap) {
        return ErrorCode::BadArg.as_return();
    }
    if let Err(()) = arena::check_out(out_len_ptr, 4) {
        return ErrorCode::BadArg.as_return();
    }

    // The engine lookup and the output writes share one `REGISTRY` borrow — the hot path
    // must not clone a `Bytes` value just to move it out of the registry first.
    cache::with_registry(|registry| {
        let hit = match registry.get(id, &key) {
            Ok(hit) => hit,
            Err(code) => return code.as_return(),
        };
        let Some(value) = hit else {
            return ErrorCode::Miss.as_return();
        };

        let probe = out_ptr == 0 && out_cap == 0;
        let len = match value {
            StoredValue::Ref(handle) => {
                if !probe {
                    // The handle's 4 LE bytes are the whole value; the host's `Map` is
                    // authoritative for what they point at (ADR-0003).
                    if let Err(()) = arena::write_out(out_ptr, out_cap, &handle.to_le_bytes()) {
                        return ErrorCode::BadArg.as_return();
                    }
                }
                4
            }
            StoredValue::Bytes(bytes) => {
                if !probe {
                    if let Err(()) = arena::write_out(out_ptr, out_cap, bytes) {
                        return ErrorCode::BadArg.as_return();
                    }
                }
                bytes.len() as u32
            }
        };
        if let Err(()) = arena::write_u32(out_len_ptr, len) {
            return ErrorCode::BadArg.as_return();
        }
        ErrorCode::Ok.as_return()
    })
}

/// Insert or overwrite `key`.
///
/// `kind` selects the value encoding: `0` = `Ref` (a `u32` handle in `value_ref_or_ptr`,
/// `value_len` ignored) or `1` = `Bytes` (arena pointer + length); anything else is
/// `BAD_ARG`. `ttl_ms == 0` means no TTL. Tags arrive as `[u32 byte_len][UTF-8 bytes]…`
/// records; a truncated record or invalid UTF-8 is `BAD_ARG`. Every input is parsed before
/// the engine is touched, so a rejected `set` mutates nothing. Success bumps the generation.
#[allow(unsafe_code)] // the #[no_mangle] exception
#[no_mangle]
pub extern "C" fn __blaze_cache_set(
    id: u32,
    key_ptr: u32,
    key_len: u32,
    kind: u32,
    value_ref_or_ptr: u32,
    value_len: u32,
    ttl_ms: u64,
    tags_ptr: u32,
    tags_len: u32,
) -> i32 {
    arena::end_epoch();
    let key = match arena::read(key_ptr, key_len) {
        Ok(key) => key,
        Err(()) => return ErrorCode::BadArg.as_return(),
    };
    let value = match kind {
        0 => StoredValue::Ref(value_ref_or_ptr),
        1 => match arena::read(value_ref_or_ptr, value_len) {
            Ok(bytes) => StoredValue::Bytes(bytes),
            Err(()) => return ErrorCode::BadArg.as_return(),
        },
        _ => return ErrorCode::BadArg.as_return(),
    };
    let tags = match arena::read_tags(tags_ptr, tags_len) {
        Ok(tags) => tags,
        Err(()) => return ErrorCode::BadArg.as_return(),
    };

    cache::with_registry(
        |registry| match registry.set(id, &key, value, ttl_ms, tags) {
            Ok(()) => ErrorCode::Ok.as_return(),
            Err(code) => code.as_return(),
        },
    )
}

/// Pure presence probe: `0` (present) or `-1` (absent). No stats, no expiry mutation.
#[allow(unsafe_code)] // the #[no_mangle] exception
#[no_mangle]
pub extern "C" fn __blaze_cache_has(id: u32, key_ptr: u32, key_len: u32) -> i32 {
    arena::end_epoch();
    let key = match arena::read(key_ptr, key_len) {
        Ok(key) => key,
        Err(()) => return ErrorCode::BadArg.as_return(),
    };
    cache::with_registry(|registry| match registry.has(id, &key) {
        Ok(true) => ErrorCode::Ok.as_return(),
        Ok(false) => ErrorCode::Miss.as_return(),
        Err(code) => code.as_return(),
    })
}

/// Remove `key`: `0` when it existed (generation bumps), `-1` on a miss (nothing mutated).
#[allow(unsafe_code)] // the #[no_mangle] exception
#[no_mangle]
pub extern "C" fn __blaze_cache_delete(id: u32, key_ptr: u32, key_len: u32) -> i32 {
    arena::end_epoch();
    let key = match arena::read(key_ptr, key_len) {
        Ok(key) => key,
        Err(()) => return ErrorCode::BadArg.as_return(),
    };
    cache::with_registry(|registry| match registry.delete(id, &key) {
        Ok(true) => ErrorCode::Ok.as_return(),
        Ok(false) => ErrorCode::Miss.as_return(),
        Err(code) => code.as_return(),
    })
}

/// Write the 56-byte stats struct to `out_ptr`; `out_cap` must hold all of it.
#[allow(unsafe_code)] // the #[no_mangle] exception
#[no_mangle]
pub extern "C" fn __blaze_cache_stats(id: u32, out_ptr: u32, out_cap: u32) -> i32 {
    arena::end_epoch();
    if (out_cap as usize) < STATS_LEN {
        return ErrorCode::BadArg.as_return();
    }
    if let Err(()) = arena::check_out(out_ptr, out_cap) {
        return ErrorCode::BadArg.as_return();
    }
    // `CacheStats` is owned, so the registry borrow ends before the arena write — only
    // `get_sync` (whose value is a borrow) needs the nested form.
    let stats = match cache::with_registry(|registry| registry.stats(id)) {
        Ok(stats) => stats,
        Err(code) => return code.as_return(),
    };
    match arena::write_out(out_ptr, out_cap, &stats_bytes(&stats)) {
        Ok(()) => ErrorCode::Ok.as_return(),
        Err(()) => ErrorCode::BadArg.as_return(),
    }
}

/// Consume an id: drop its cache, keep the tombstone, bump the generation. The id is dead
/// afterwards — every later op returns `-4` — but `__blaze_generation` still reports the
/// post-close value. Does not touch the arena (no epoch is armed).
#[allow(unsafe_code)] // the #[no_mangle] exception
#[no_mangle]
pub extern "C" fn __blaze_cache_close(id: u32) -> i32 {
    cache::with_registry(|registry| match registry.close(id) {
        Ok(()) => ErrorCode::Ok.as_return(),
        Err(code) => code.as_return(),
    })
}

/// The mutation counter for `id`: starts at `1` when the cache opens, bumps on `set`,
/// `delete`-that-removed and `close`. `0` means the id was never issued — which is how a
/// host tells "fresh cache" from "bogus id" with one call. Does not touch the arena.
#[allow(unsafe_code)] // the #[no_mangle] exception
#[no_mangle]
pub extern "C" fn __blaze_generation(id: u32) -> u64 {
    cache::with_registry(|registry| registry.generation(id))
}

/// Advance the host-driven clock to `now_ms` (monotonic milliseconds — `performance.now()`
/// or `Date.now()` both work). wasm32 has no clock of its own (ADR-0002), so the host calls
/// this before ABI calls that can expire entries; a no-op off-wasm. Does not touch the arena.
#[allow(unsafe_code)] // the #[no_mangle] exception
#[no_mangle]
pub extern "C" fn __blaze_tick(now_ms: f64) {
    clock::set_now_ms(now_ms);
}

#[cfg(test)]
mod tests {
    use super::{
        __blaze_cache_open, __blaze_cache_set, __blaze_cache_stats, stats_bytes, ErrorCode,
        STATS_LEN,
    };
    use crate::engine::CacheStats;

    #[test]
    fn error_codes_match_the_plan_table() {
        assert_eq!(ErrorCode::Ok.code(), 0);
        assert_eq!(ErrorCode::Miss.code(), 1);
        assert_eq!(ErrorCode::BadArg.code(), 2);
        assert_eq!(ErrorCode::StaleView.code(), 3);
        assert_eq!(ErrorCode::Closed.code(), 4);
        assert_eq!(ErrorCode::Oom.code(), 5);
    }

    #[test]
    fn exports_return_negated_codes_with_ok_staying_zero() {
        assert_eq!(ErrorCode::Ok.as_return(), 0);
        assert_eq!(ErrorCode::Miss.as_return(), -1);
        assert_eq!(ErrorCode::BadArg.as_return(), -2);
        assert_eq!(ErrorCode::StaleView.as_return(), -3);
        assert_eq!(ErrorCode::Closed.as_return(), -4);
        assert_eq!(ErrorCode::Oom.as_return(), -5);
    }

    #[test]
    fn stats_bytes_are_seven_le_u64s_in_plan_order() {
        let stats = CacheStats {
            hits: 1,
            misses: 2,
            evictions: 3,
            expirations: 4,
            invalidations: 5,
            size: 6,
            capacity: 7,
        };
        let bytes = stats_bytes(&stats);
        assert_eq!(bytes.len(), STATS_LEN);
        for (i, expected) in [1u64, 2, 3, 4, 5, 6, 7].iter().enumerate() {
            let mut word = [0u8; 8];
            word.copy_from_slice(&bytes[i * 8..(i + 1) * 8]);
            assert_eq!(u64::from_le_bytes(word), *expected, "field {i}");
        }
    }

    #[test]
    fn stats_layout_is_56_bytes() {
        assert_eq!(STATS_LEN, 56, "the plan pins the stats struct at 56 bytes");
    }

    /// The plan pins `eviction = 0` (Sieve) as the only policy; anything else is
    /// `BAD_ARG`. Needs a non-zero config byte, so it lives here (unit test) rather than
    /// in the zeros-only integration suite.
    #[test]
    fn open_rejects_unknown_eviction_policy() {
        // Wipe the arena first so this test's allocations start fresh.
        let _ = __blaze_cache_stats(u32::MAX, 0, 0);

        // capacity = 2, eviction = 1 → rejected.
        let cfg = crate::arena::alloc(8);
        assert_ne!(cfg, 0);
        crate::arena::write_for_test(cfg, &[2, 0, 0, 0, 1, 0, 0, 0]);
        assert_eq!(
            __blaze_cache_open(cfg, 8),
            ErrorCode::BadArg.as_return(),
            "eviction 1 must be rejected"
        );

        // capacity = 2, eviction = 0 → accepted (the non-zero capacity path).
        let cfg = crate::arena::alloc(8);
        assert_ne!(cfg, 0);
        crate::arena::write_for_test(cfg, &[2, 0, 0, 0, 0, 0, 0, 0]);
        let id = __blaze_cache_open(cfg, 8);
        assert!(id > 0, "a valid config must open, got {id}");
    }

    /// A tag record whose declared length runs past the buffer is `BAD_ARG`.
    #[test]
    fn set_rejects_overrunning_tag_content() {
        let _ = __blaze_cache_stats(u32::MAX, 0, 0);
        let id = __blaze_cache_open(
            {
                let cfg = crate::arena::alloc(8);
                crate::arena::write_for_test(cfg, &[0, 0, 0, 0, 0, 0, 0, 0]);
                cfg
            },
            8,
        );
        assert!(id > 0);
        let key = crate::arena::alloc(4);
        assert_ne!(key, 0);

        // Header says 99 bytes of content, buffer holds 4 → overrun.
        let tags = crate::arena::alloc(8);
        assert_ne!(tags, 0);
        crate::arena::write_for_test(tags, &99u32.to_le_bytes());
        assert_eq!(
            __blaze_cache_set(id as u32, key, 4, 0, 7, 0, 0, tags, 8),
            ErrorCode::BadArg.as_return(),
            "a tag content overrun must be rejected"
        );
    }

    /// Tag bytes must be UTF-8.
    #[test]
    fn set_rejects_non_utf8_tags() {
        let _ = __blaze_cache_stats(u32::MAX, 0, 0);
        let id = __blaze_cache_open(
            {
                let cfg = crate::arena::alloc(8);
                crate::arena::write_for_test(cfg, &[0, 0, 0, 0, 0, 0, 0, 0]);
                cfg
            },
            8,
        );
        assert!(id > 0);
        let key = crate::arena::alloc(4);
        assert_ne!(key, 0);

        // One 1-byte tag whose byte is not valid UTF-8.
        let tags = crate::arena::alloc(8);
        assert_ne!(tags, 0);
        crate::arena::write_for_test(tags, &[1, 0, 0, 0, 0xFF, 0, 0, 0]);
        assert_eq!(
            __blaze_cache_set(id as u32, key, 4, 0, 7, 0, 0, tags, 8),
            ErrorCode::BadArg.as_return(),
            "non-UTF-8 tag bytes must be rejected"
        );
    }
}
