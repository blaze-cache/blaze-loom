//! External-consumer view of the flat ABI: these tests link against `blaze_cache_wasm` the
//! way a JS host does — through the `extern "C"` exports only, never through Rust APIs.
//!
//! The arena and the registry are thread-locals (a `wasm32` instance has no threads, so
//! "one per thread" is "one per instance"), so tests run in parallel without interfering:
//! each test thread owns its arena and its id space. `begin()` additionally arms the epoch
//! reset, so a test starts from a wiped arena no matter what an earlier test on the same
//! thread left behind.
//!
//! # What native tests can and cannot see
//!
//! There is no read-back export, so a native test observes only **return codes** and
//! `__blaze_generation`. Byte contents — stats fields, value bytes, tag strings — are
//! `js/harness.mjs`'s job. What native tests *can* rely on is the arena's zeroing: every
//! region handed out on a fresh epoch is zeros, so keys are deterministic and their
//! **length** distinguishes them (8 zeros ≠ 16 zeros).
//!
//! # Epoch discipline — the helpers enforce it
//!
//! Each operation allocates **all** of its inputs, calls exactly one arena-touching
//! export, then reads its result. Offsets are only valid until the next `alloc`, which may
//! recycle them — so a pointer from before a reset must never be reused afterwards (the
//! stale-key hazard is documented in `src/arena.rs`). The `set`/`get`/`has`/`delete`
//! helpers below therefore allocate a **fresh** key for every call; tests that need raw
//! calls allocate their own inputs first, immediately before the export.

use blaze_cache_wasm::abi::{
    __blaze_arena_alloc, __blaze_cache_close, __blaze_cache_delete, __blaze_cache_get_sync,
    __blaze_cache_has, __blaze_cache_open, __blaze_cache_set, __blaze_cache_stats,
    __blaze_generation, __blaze_tick, ErrorCode,
};
use blaze_cache_wasm::{__blaze_abi_version, ABI_VERSION};

/// Never issued: ids are monotonic from `1`.
const UNKNOWN_ID: u32 = u32::MAX;
/// A pointer that is not in the arena on any target.
const DANGLING_PTR: u32 = u32::MAX;

const KIND_REF: u32 = 0;
const KIND_BYTES: u32 = 1;

/// Arm the epoch reset so this test's first `alloc` starts from a wiped arena.
fn begin() {
    // Any arena-touching export works; `stats` on a bogus id does nothing else.
    let _ = __blaze_cache_stats(UNKNOWN_ID, 0, 0);
}

/// Allocate `len` scratch bytes. Never returns `0` for `len > 0` (0 is the failure signal).
fn alloc(len: u32) -> u32 {
    let ptr = __blaze_arena_alloc(len);
    assert!(len == 0 || ptr != 0, "arena must fit {len} bytes");
    ptr
}

/// Open a cache through the real config encoding. All zeros → `capacity = 0` (clamped to
/// 1 by the core) and `eviction = 0` (Sieve).
fn open() -> u32 {
    let cfg = alloc(8);
    let id = __blaze_cache_open(cfg, 8);
    assert!(id > 0, "open must succeed, got {id}");
    id as u32
}

/// `close` returns an `i32`; keep the call sites readable.
fn close(id: u32) -> i32 {
    __blaze_cache_close(id)
}

/// One `set` per epoch with a fresh `key_len`-byte key of zeros (kind `Ref`).
fn set(id: u32, key_len: u32, handle: u32) -> i32 {
    set_kind(id, key_len, KIND_REF, handle, 0)
}

/// [`set`] with an explicit value kind and (for `Bytes`) pointer/length.
fn set_kind(id: u32, key_len: u32, kind: u32, value_ref: u32, value_len: u32) -> i32 {
    let key = alloc(key_len);
    __blaze_cache_set(id, key, key_len, kind, value_ref, value_len, 0, 0, 0)
}

/// [`set`] with a caller-owned value buffer — allocate `value_ptr` **before** calling, so
/// both buffers land in this epoch (the helper's key alloc comes after it, never before a
/// reset).
fn set_bytes(id: u32, key_len: u32, value_ptr: u32, value_len: u32) -> i32 {
    let key = alloc(key_len);
    __blaze_cache_set(id, key, key_len, KIND_BYTES, value_ptr, value_len, 0, 0, 0)
}

/// [`set`] with a caller-owned tag buffer — same rule as [`set_bytes`]: allocate `tags_ptr`
/// before calling.
fn set_with_tags(id: u32, key_len: u32, handle: u32, tags_ptr: u32, tags_len: u32) -> i32 {
    let key = alloc(key_len);
    __blaze_cache_set(id, key, key_len, KIND_REF, handle, 0, 0, tags_ptr, tags_len)
}

/// One `has` per epoch with a fresh key.
fn has(id: u32, key_len: u32) -> i32 {
    let key = alloc(key_len);
    __blaze_cache_has(id, key, key_len)
}

/// One `delete` per epoch with a fresh key.
fn delete(id: u32, key_len: u32) -> i32 {
    let key = alloc(key_len);
    __blaze_cache_delete(id, key, key_len)
}

/// One `get_sync` per epoch with a fresh key, output buffer and length word.
/// `value_cap == 0` selects probe mode (`out_ptr = out_cap = 0`).
fn get(id: u32, key_len: u32, value_cap: u32) -> i32 {
    let key = alloc(key_len);
    let (out, out_cap) = if value_cap == 0 {
        (0, 0)
    } else {
        (alloc(value_cap), value_cap)
    };
    let out_len = alloc(4);
    __blaze_cache_get_sync(id, key, key_len, out, out_cap, out_len)
}

#[test]
fn abi_version_is_1() {
    assert_eq!(ABI_VERSION, 1);
    assert_eq!(__blaze_abi_version(), 1);
}

#[test]
fn tick_is_reachable_and_never_panics() {
    __blaze_tick(0.0);
    __blaze_tick(1_700_000_000_000.0);
    begin();
    let id = open();
    assert_eq!(__blaze_generation(id), 1);
}

#[test]
fn never_issued_id_is_bad_arg_and_generation_zero() {
    begin();
    assert_eq!(__blaze_generation(UNKNOWN_ID), 0);
    assert_eq!(__blaze_generation(0), 0);

    assert_eq!(set(UNKNOWN_ID, 4, 7), ErrorCode::BadArg.as_return());
    assert_eq!(has(UNKNOWN_ID, 4), ErrorCode::BadArg.as_return());
    assert_eq!(delete(UNKNOWN_ID, 4), ErrorCode::BadArg.as_return());
    assert_eq!(get(UNKNOWN_ID, 4, 4), ErrorCode::BadArg.as_return());

    let stats = alloc(56);
    assert_eq!(
        __blaze_cache_stats(UNKNOWN_ID, stats, 56),
        ErrorCode::BadArg.as_return()
    );
    assert_eq!(close(UNKNOWN_ID), ErrorCode::BadArg.as_return());
}

#[test]
fn closed_id_is_closed_and_keeps_its_generation() {
    begin();
    let id = open();
    assert_eq!(set(id, 4, 7), 0);
    let after_set = __blaze_generation(id);
    assert!(after_set > 1, "set must bump the generation");

    assert_eq!(close(id), 0);
    let after_close = __blaze_generation(id);
    assert!(after_close > after_set, "close must bump the generation");

    // Every later op fails with CLOSED — never BAD_ARG, never Ok.
    assert_eq!(set(id, 4, 7), ErrorCode::Closed.as_return());
    assert_eq!(has(id, 4), ErrorCode::Closed.as_return());
    assert_eq!(delete(id, 4), ErrorCode::Closed.as_return());
    assert_eq!(get(id, 4, 4), ErrorCode::Closed.as_return());

    let stats = alloc(56);
    assert_eq!(
        __blaze_cache_stats(id, stats, 56),
        ErrorCode::Closed.as_return()
    );
    assert_eq!(close(id), ErrorCode::Closed.as_return(), "double close");
    assert_eq!(
        __blaze_generation(id),
        after_close,
        "the tombstone keeps reporting its post-close generation"
    );
}

/// Close consumes an id; the next `open` issues a fresh, higher one. A closed id is *not*
/// unknown: it keeps reporting its post-close generation (the plan pins `0` for unknown
/// ids only, and `close` increments the generation).
#[test]
fn open_after_close_issues_a_new_id() {
    begin();
    let first = open();
    assert_eq!(close(first), 0);
    let second = open();
    assert!(second > first, "ids are monotonic");
    assert!(
        __blaze_generation(first) > 0,
        "a closed id reports its post-close generation, not 0"
    );
    assert_eq!(__blaze_generation(second), 1);
}

/// A bad pointer degrades to `BAD_ARG` on every export — never a trap, never a success.
#[test]
fn hostile_pointers_degrade_to_bad_arg() {
    begin();
    let id = open();

    // Dangling pointer, in-bounds length.
    assert_eq!(
        __blaze_cache_has(id, DANGLING_PTR, 4),
        ErrorCode::BadArg.as_return()
    );
    assert_eq!(
        __blaze_cache_delete(id, DANGLING_PTR, 4),
        ErrorCode::BadArg.as_return()
    );
    assert_eq!(
        __blaze_cache_open(DANGLING_PTR, 8),
        ErrorCode::BadArg.as_return()
    );
    assert_eq!(
        __blaze_cache_set(id, DANGLING_PTR, 4, KIND_REF, 7, 0, 0, 0, 0),
        ErrorCode::BadArg.as_return()
    );
    assert_eq!(
        __blaze_cache_stats(id, DANGLING_PTR, 56),
        ErrorCode::BadArg.as_return()
    );

    // Null pointer with a non-zero length: the null zone is never handed out.
    assert_eq!(
        __blaze_cache_has(id, 0, 4),
        ErrorCode::BadArg.as_return(),
        "ptr 0 with len > 0 must be rejected"
    );
    assert_eq!(
        __blaze_cache_open(0, 8),
        ErrorCode::BadArg.as_return(),
        "ptr 0 with len > 0 must be rejected"
    );

    // A real region with a length that overruns the arena.
    let tiny = alloc(4);
    assert_eq!(
        __blaze_cache_has(id, tiny, 1_000_000),
        ErrorCode::BadArg.as_return(),
        "a length past the arena must be rejected"
    );

    // Output pointers are validated before the lookup: no length word, no value.
    let key = alloc(4);
    assert_eq!(
        __blaze_cache_get_sync(id, key, 4, 0, 0, 0),
        ErrorCode::BadArg.as_return(),
        "out_len_ptr must be valid even in probe mode"
    );
    assert_eq!(
        __blaze_cache_get_sync(id, key, 4, DANGLING_PTR, 4, DANGLING_PTR),
        ErrorCode::BadArg.as_return(),
        "dangling out pointers must be rejected before the lookup"
    );

    // An allocation larger than the arena fails with 0 instead of growing (and moving).
    assert_eq!(__blaze_arena_alloc(u32::MAX), 0, "OOM reports 0");
}

/// The config must be exactly 8 bytes; anything else is `BAD_ARG`.
#[test]
fn bad_config_is_rejected() {
    begin();
    let too_short = alloc(4);
    assert_eq!(
        __blaze_cache_open(too_short, 4),
        ErrorCode::BadArg.as_return(),
        "a 4-byte config must be rejected"
    );
    let too_long = alloc(16);
    assert_eq!(
        __blaze_cache_open(too_long, 16),
        ErrorCode::BadArg.as_return(),
        "a 16-byte config must be rejected"
    );
    assert_eq!(
        __blaze_cache_open(0, 0),
        ErrorCode::BadArg.as_return(),
        "an empty config must be rejected"
    );
    // The all-zero config (capacity 0, eviction 0) is valid: capacity clamps to 1.
    let zeroed = alloc(8);
    assert!(
        __blaze_cache_open(zeroed, 8) > 0,
        "capacity 0 must clamp and open"
    );
}

/// Tag buffers are `[u32 byte_len][UTF-8 bytes]…`; a truncated record is `BAD_ARG`.
#[test]
fn malformed_tags_are_rejected() {
    begin();
    let id = open();

    // A header that runs past the end of the buffer (a lone empty tag, `tags_len = 4`,
    // would be valid — that case lives in the unit tests with non-zero bytes).
    for truncated_len in [1u32, 2, 3, 5, 6, 7] {
        let tags = alloc(8);
        assert_eq!(
            set_with_tags(id, 4, 7, tags, truncated_len),
            ErrorCode::BadArg.as_return(),
            "tags_len={truncated_len} must cut a record"
        );
    }
    // A dangling tag pointer with a non-zero length.
    assert_eq!(
        set_with_tags(id, 4, 7, DANGLING_PTR, 8),
        ErrorCode::BadArg.as_return()
    );
    // The empty buffer means "no tags" and is valid.
    assert_eq!(set(id, 4, 7), 0);
}

/// `set`'s value kind must be 0 (Ref) or 1 (Bytes).
#[test]
fn set_rejects_unknown_kind_and_bad_value_pointer() {
    begin();
    let id = open();

    for kind in [2u32, 3, 99, u32::MAX] {
        assert_eq!(
            set_kind(id, 4, kind, 7, 0),
            ErrorCode::BadArg.as_return(),
            "kind {kind} must be rejected"
        );
    }

    // Kind 1 reads bytes: the value pointer must be in bounds too.
    let key = alloc(4);
    assert_eq!(
        __blaze_cache_set(id, key, 4, KIND_BYTES, DANGLING_PTR, 4, 0, 0, 0),
        ErrorCode::BadArg.as_return(),
        "kind 1 with a dangling value pointer must be rejected"
    );
    assert_eq!(
        __blaze_cache_set(id, key, 4, KIND_BYTES, 0, 4, 0, 0, 0),
        ErrorCode::BadArg.as_return(),
        "kind 1 with null + non-zero length must be rejected"
    );

    // Both legal kinds succeed (a fresh key for each — the raw calls above armed the
    // epoch, so `set`'s key alloc recycles first).
    assert_eq!(set(id, 4, 7), 0);
    let value = alloc(4);
    assert_eq!(set_bytes(id, 4, value, 4), 0);
}

/// A missing key is `-1` for `get`/`has`/`delete`, and a hit is `0`.
#[test]
fn miss_semantics_distinguish_lengths() {
    begin();
    let id = open();

    // Absent: a 4-byte key of zeros was never stored.
    assert_eq!(get(id, 4, 0), ErrorCode::Miss.as_return(), "probe a miss");
    assert_eq!(has(id, 4), ErrorCode::Miss.as_return());
    assert_eq!(delete(id, 4), ErrorCode::Miss.as_return());

    // Present: the same 4-byte zeros key, stored.
    assert_eq!(set(id, 4, 7), 0);
    assert_eq!(get(id, 4, 4), 0, "hit writes into a 4-byte out buffer");
    assert_eq!(has(id, 4), 0);
    assert_eq!(delete(id, 4), 0);

    // Deleted → miss again.
    assert_eq!(get(id, 4, 0), ErrorCode::Miss.as_return());
    assert_eq!(has(id, 4), ErrorCode::Miss.as_return());
    assert_eq!(
        delete(id, 4),
        ErrorCode::Miss.as_return(),
        "deleting a missing key is a miss, not an error"
    );

    // A different length is a different (missing) key: 16 zeros ≠ 4 zeros.
    assert_eq!(has(id, 16), ErrorCode::Miss.as_return());
    assert_eq!(get(id, 16, 0), ErrorCode::Miss.as_return());
}

/// The extremes of the key space: empty, impossible, and as large as the arena holds.
#[test]
fn zero_and_max_length_keys() {
    begin();
    let id = open();

    // key_len = 0 is a real (empty) key — never stored here, so it probes as a miss.
    assert_eq!(get(id, 0, 0), ErrorCode::Miss.as_return());

    // key_len = u32::MAX can never be in bounds → BAD_ARG, never a trap or a wrap-around.
    let tiny = alloc(4);
    assert_eq!(
        __blaze_cache_has(id, tiny, u32::MAX),
        ErrorCode::BadArg.as_return(),
        "a u32::MAX key length must be rejected"
    );
    assert_eq!(
        __blaze_cache_set(id, tiny, u32::MAX, KIND_REF, 7, 0, 0, 0, 0),
        ErrorCode::BadArg.as_return(),
        "a u32::MAX key length must be rejected by set too"
    );

    // A maximal-but-real key: as much of the 1 MiB arena as still fits after the bump.
    // It must be accepted (and miss — never stored). The length word is allocated in the
    // *same* epoch as the key — before `has` arms the reset — so both pointers stay valid
    // across both calls without an intervening `alloc`.
    let big_len = 1_000_000;
    let big = alloc(big_len);
    assert_ne!(big, 0, "the arena must fit a 1,000,000-byte key");
    let out_len = alloc(4);
    assert_eq!(
        __blaze_cache_has(id, big, big_len),
        ErrorCode::Miss.as_return(),
        "a huge-but-valid key reads fine and misses"
    );
    assert_eq!(
        __blaze_cache_get_sync(id, big, big_len, 0, 0, out_len),
        ErrorCode::Miss.as_return()
    );
}

/// A zero-length key is a legitimate key. The plan pins `alloc → 0 on failure`, so a
/// zero-length allocation still returns a *valid* pointer and `0` stays unambiguous; both
/// spellings of the empty input — the allocated pointer and the null pointer — work.
#[test]
fn zero_length_key_round_trips() {
    begin();
    let id = open();

    let empty = alloc(0);
    assert_ne!(
        empty, 0,
        "0 is the failure signal, even for a zero-length request"
    );
    assert_eq!(set_kind(id, 0, KIND_REF, 42, 0), 0, "via the pointer");
    assert_eq!(
        __blaze_cache_set(id, 0, 0, KIND_REF, 42, 0, 0, 0, 0),
        0,
        "via the null pointer"
    );
    assert_eq!(get(id, 0, 4), 0, "the empty key must be retrievable");
    assert_eq!(has(id, 0), 0);
    assert_eq!(delete(id, 0), 0);
    assert_eq!(get(id, 0, 4), ErrorCode::Miss.as_return());
}

/// `get_sync`'s out buffer must fit the value; probe mode `(0, 0)` sizes it first.
#[test]
fn get_sync_out_buffer_must_fit_the_value() {
    begin();
    let id = open();

    // Store a 16-byte value.
    let value = alloc(16);
    assert_eq!(set_bytes(id, 4, value, 16), 0);

    // Probe mode: only the length word is written.
    assert_eq!(get(id, 4, 0), 0, "probe mode must succeed");

    // Too small → BAD_ARG (the host resizes and retries).
    assert_eq!(
        get(id, 4, 8),
        ErrorCode::BadArg.as_return(),
        "an out buffer smaller than the value must be rejected"
    );

    // Exactly big enough → Ok.
    assert_eq!(get(id, 4, 16), 0);

    // A non-zero out_cap at the null pointer is rejected (it is not probe mode).
    let key = alloc(4);
    let out_len = alloc(4);
    assert_eq!(
        __blaze_cache_get_sync(id, key, 4, 0, 4, out_len),
        ErrorCode::BadArg.as_return(),
        "out_ptr=0 with non-zero out_cap must be rejected"
    );
}

/// A `Ref` value is returned as its 4-byte handle; its length is always 4.
#[test]
fn ref_values_are_four_bytes() {
    begin();
    let id = open();

    assert_eq!(set(id, 4, 0xDEAD_BEEF), 0);
    assert_eq!(get(id, 4, 4), 0);
    // The engine counts the hit; a second read still succeeds.
    assert_eq!(get(id, 4, 4), 0);
}

/// `stats` needs room for the full 56-byte struct.
#[test]
fn stats_needs_room_for_56_bytes() {
    begin();
    let id = open();

    assert_eq!(
        __blaze_cache_stats(id, 0, 0),
        ErrorCode::BadArg.as_return(),
        "a zero-capacity out buffer must be rejected"
    );
    let small = alloc(8);
    assert_eq!(
        __blaze_cache_stats(id, small, 8),
        ErrorCode::BadArg.as_return(),
        "a short out buffer must be rejected"
    );
    let small = alloc(55);
    assert_eq!(
        __blaze_cache_stats(id, small, 55),
        ErrorCode::BadArg.as_return(),
        "55 bytes must be rejected"
    );
    let full = alloc(56);
    assert_eq!(__blaze_cache_stats(id, full, 56), 0);
    // A larger buffer is fine too.
    let bigger = alloc(64);
    assert_eq!(__blaze_cache_stats(id, bigger, 64), 0);
}

/// The generation counts mutations: open = 1, each `set` and each successful `delete`
/// bumps it; a miss does not.
#[test]
fn generation_counts_mutations() {
    begin();
    let id = open();
    assert_eq!(__blaze_generation(id), 1, "a fresh cache starts at 1");

    assert_eq!(set(id, 4, 7), 0);
    let after_set = __blaze_generation(id);
    assert!(after_set > 1, "set must bump the generation");

    // Overwriting the same logical key bumps it again.
    assert_eq!(set(id, 4, 8), 0);
    let after_overwrite = __blaze_generation(id);
    assert!(after_overwrite > after_set, "overwrite must bump");

    // A miss does not mutate anything.
    assert_eq!(delete(id, 16), ErrorCode::Miss.as_return());
    assert_eq!(
        __blaze_generation(id),
        after_overwrite,
        "a missed delete must not bump"
    );

    // A successful delete does.
    assert_eq!(delete(id, 4), 0);
    let after_delete = __blaze_generation(id);
    assert!(after_delete > after_overwrite, "delete must bump");
}

/// The epoch contract end-to-end. One shared batch of buffers serves many ops (offsets
/// stay valid while no `alloc` happens between them), then a long alloc → op → alloc →
/// cycle proves the arena resets cleanly underneath.
#[test]
fn epoch_discipline_allows_successive_operations() {
    begin();
    let id = open();

    // One shared batch of buffers, many ops, no allocation in between: the key, the out
    // buffer and the length word are disjoint for the whole batch.
    let key = alloc(4);
    let out = alloc(4);
    let out_len = alloc(4);
    assert_eq!(__blaze_cache_set(id, key, 4, KIND_REF, 1, 0, 0, 0, 0), 0);
    assert_eq!(__blaze_cache_get_sync(id, key, 4, out, 4, out_len), 0);
    assert_eq!(__blaze_cache_has(id, key, 4), 0);
    assert_eq!(
        __blaze_cache_get_sync(id, key, 4, 0, 0, out_len),
        0,
        "probe mode reuses the same length word"
    );
    assert_eq!(__blaze_cache_delete(id, key, 4), 0);
    assert_eq!(
        __blaze_cache_get_sync(id, key, 4, 0, 0, out_len),
        ErrorCode::Miss.as_return()
    );

    // Now a long alloc → op → alloc → op cycle. Every op allocates its inputs fresh, so
    // the resets between epochs can never alias them.
    for i in 0u32..32 {
        let key_len = 4 + i % 5;
        let result = set(id, key_len, i);
        assert_eq!(result, 0, "iteration {i}: set must succeed");

        let result = get(id, key_len, 4);
        // The value is either present (0) or evicted by a later insert (-1); never a trap
        // and never a fabricated error.
        assert!(
            result == 0 || result == ErrorCode::Miss.as_return(),
            "iteration {i}: get must be Ok or Miss, got {result}"
        );
    }
}

/// A host that violates the epoch contract (calling an export with a pointer from before
/// an intervening `alloc`) still gets codes, never a trap.
#[test]
fn stale_offsets_degrade_to_codes() {
    begin();
    let id = open();

    let key = alloc(4);
    assert_eq!(__blaze_cache_set(id, key, 4, KIND_REF, 1, 0, 0, 0, 0), 0);

    // The next alloc resets the arena; `key` now points at recycled space that may have
    // been handed out again. Whatever the host does with it, the ABI answers with a code.
    let recycled = alloc(64);
    assert_ne!(recycled, 0);
    let result = __blaze_cache_has(id, key, 4);
    assert!(
        result == 0
            || result == ErrorCode::Miss.as_return()
            || result == ErrorCode::BadArg.as_return(),
        "stale reads must degrade to a code, got {result}"
    );
}

/// `close` on an id that was never issued is `BAD_ARG`, not `CLOSED`.
#[test]
fn close_distinguishes_unknown_from_closed() {
    begin();
    assert_eq!(close(UNKNOWN_ID), ErrorCode::BadArg.as_return());
    let id = open();
    assert_eq!(close(id), 0);
    assert_eq!(
        close(id),
        ErrorCode::Closed.as_return(),
        "a closed id stays closed"
    );
}
