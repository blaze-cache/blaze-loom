//! Cache ids, slots and generations — the registry the flat exports operate on.
//!
//! Locking lives in [`crate::abi`]: an export runs its whole body inside one
//! [`with_registry`] call, outermost, then touches the arena in short non-nested sections.
//! Every method here takes `&mut Registry` (or `&Registry`) explicitly, so nothing in this
//! module can re-enter a borrow — no reentrancy panics are possible by construction.
//!
//! # Id semantics (the plan's error table, made precise)
//!
//! * ids are monotonic from `1`; `0` is never issued;
//! * an id with **no slot** was never issued → every op returns [`ErrorCode::BadArg`] (the
//!   plan's "bad id");
//! * a slot whose cache was **taken by `close`** is a tombstone → every op returns
//!   [`ErrorCode::Closed`], while `generation` still reports its post-close value;
//! * a slot with a live cache proceeds normally.

use std::cell::RefCell;
use std::collections::HashMap;
use std::time::Duration;

use crate::abi::ErrorCode;
use crate::engine::{Cache, CacheStats, EntryOptions};

/// What `set` stores and `get_sync` returns: a host-owned handle, or bytes wasm owns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum StoredValue {
    /// Kind `0` — an opaque `u32` the host keeps in its own `Map` (ADR-0003). Wasm never
    /// dereferences it.
    Ref(u32),
    /// Kind `1` — bytes wasm owns: a copy of the arena region handed to `set`.
    Bytes(Vec<u8>),
}

/// The engine cache behind one id: byte keys, [`StoredValue`] values.
pub(crate) type SlotCache = Cache<Vec<u8>, StoredValue>;

/// One id: a live cache, or a `close` tombstone.
pub(crate) struct CacheSlot {
    /// `None` after `close`. The tombstone keeps both promises the plan makes — the id is
    /// consumed (every op fails) and its generation stays observable (the bump `close`
    /// performed can still be read back). Dropping tombstones is 0.3.0 housekeeping; the
    /// leak is bounded by the number of ids a host ever opens.
    pub(crate) cache: Option<SlotCache>,
    /// Starts at `1` so `generation(id) == 0` unambiguously means "never issued" — a fresh
    /// cache and an unknown id are then distinguishable with a single call.
    pub(crate) generation: u64,
}

/// All open ids plus their tombstones.
pub(crate) struct Registry {
    next_id: u32,
    slots: HashMap<u32, CacheSlot>,
}

thread_local! {
    /// This thread's registry (`None` until the first `open`).
    ///
    /// `thread_local` + `RefCell` rather than a `static Mutex`: the engine's `Cache` holds a
    /// `Box<dyn QueryIndex<K>>`, which is not `Send`, so it cannot live behind a static
    /// mutex. Upstream's own wasm binding stores its globals the same way, and on `wasm32`
    /// (no threads) a thread-local is exactly "one registry per instance". `const`-init
    /// keeps the lazy-init machinery — and any import — out of the module; `RefCell` has no
    /// poisoning to recover from.
    static REGISTRY: RefCell<Option<Registry>> = const { RefCell::new(None) };
}

/// Run `f` against this thread's registry, creating it on first use.
///
/// The `RefCell` borrow lives only inside `f`, so `f`'s result must be **owned** — a
/// borrow of registry data cannot escape (the compiler enforces it). Exports therefore run
/// their whole engine-plus-arena-write body inside this one call: that is the `REGISTRY` →
/// `ARENA` nesting `abi.rs` documents, and it is panic-free because no path borrows `ARENA`
/// first and then asks for `REGISTRY`.
pub(crate) fn with_registry<R>(f: impl FnOnce(&mut Registry) -> R) -> R {
    REGISTRY.with(|cell| {
        let mut borrowed = cell.borrow_mut();
        f(borrowed.get_or_insert_with(Registry::new))
    })
}

impl Registry {
    fn new() -> Self {
        Self {
            next_id: 1,
            slots: HashMap::new(),
        }
    }

    /// Open a new cache with `capacity` entries (already clamped to ≥1 by the caller's
    /// config parsing; the engine clamps again).
    ///
    /// `Err` means the `u32` id space is exhausted (or would cross `i32::MAX`, past which an
    /// `i32` return could not distinguish an id from a negated error code) — an unreachable
    /// `OOM`, but a real one: no panic, no wrap-around id reuse.
    pub(crate) fn open(&mut self, capacity: u32) -> Result<u32, ErrorCode> {
        let id = self.next_id;
        if id > i32::MAX as u32 {
            return Err(ErrorCode::Oom);
        }
        self.next_id = id.checked_add(1).ok_or(ErrorCode::Oom)?;
        let cache = Cache::new(if capacity == 0 { 1 } else { capacity as usize });
        self.slots.insert(
            id,
            CacheSlot {
                cache: Some(cache),
                generation: 1,
            },
        );
        Ok(id)
    }

    /// A slot with a **live** cache — else the plan-mandated code for this id.
    fn live_slot_mut(&mut self, id: u32) -> Result<&mut CacheSlot, ErrorCode> {
        let slot = self.slots.get_mut(&id).ok_or(ErrorCode::BadArg)?;
        if slot.cache.is_none() {
            return Err(ErrorCode::Closed);
        }
        Ok(slot)
    }

    /// A slot with a **live** cache (shared) — else the plan-mandated code for this id.
    fn live_slot(&self, id: u32) -> Result<&CacheSlot, ErrorCode> {
        let slot = self.slots.get(&id).ok_or(ErrorCode::BadArg)?;
        if slot.cache.is_none() {
            return Err(ErrorCode::Closed);
        }
        Ok(slot)
    }

    /// Generation for an id: `0` when it was never issued, otherwise the slot's counter (a
    /// tombstone reports its post-`close` value, so a close is observable).
    pub(crate) fn generation(&self, id: u32) -> u64 {
        self.slots.get(&id).map_or(0, |slot| slot.generation)
    }

    /// Consume an id: drop the cache, keep the tombstone, bump the generation.
    pub(crate) fn close(&mut self, id: u32) -> Result<(), ErrorCode> {
        let slot = self.live_slot_mut(id)?;
        let cache = slot.cache.take().ok_or(ErrorCode::Closed)?;
        drop(cache);
        slot.generation = slot.generation.saturating_add(1);
        Ok(())
    }

    /// `get_sync`'s lookup. The returned borrow is tied to `&mut self` — i.e. to the
    /// caller's `REGISTRY` guard — so the export can write the value into the arena while
    /// still holding that guard (the arena is a different lock; order stays `REGISTRY` → `ARENA`).
    pub(crate) fn get(&mut self, id: u32, key: &[u8]) -> Result<Option<&StoredValue>, ErrorCode> {
        let slot = self.live_slot_mut(id)?;
        let cache = slot.cache.as_mut().ok_or(ErrorCode::Closed)?;
        Ok(cache.get(key))
    }

    /// Pure presence probe (`contains_key`): no stats, no expiry mutation.
    pub(crate) fn has(&mut self, id: u32, key: &[u8]) -> Result<bool, ErrorCode> {
        let slot = self.live_slot_mut(id)?;
        let cache = slot.cache.as_mut().ok_or(ErrorCode::Closed)?;
        Ok(cache.contains_key(key))
    }

    /// Upsert. Succeeds for any live id (a missing key is created), bumps the generation,
    /// and lets the engine count the overwrite as an invalidation.
    pub(crate) fn set(
        &mut self,
        id: u32,
        key: &[u8],
        value: StoredValue,
        ttl_ms: u64,
        tags: Vec<String>,
    ) -> Result<(), ErrorCode> {
        let slot = self.live_slot_mut(id)?;
        let cache = slot.cache.as_mut().ok_or(ErrorCode::Closed)?;
        let mut options = EntryOptions::new();
        if ttl_ms > 0 {
            options = options.ttl(Duration::from_millis(ttl_ms));
        }
        if !tags.is_empty() {
            options = options.tags(tags);
        }
        cache.insert_borrowed(key, value, options);
        slot.generation = slot.generation.saturating_add(1);
        Ok(())
    }

    /// Remove one key. `Ok(true)` when it existed (generation bumps); `Ok(false)` on a miss
    /// (no bump — nothing mutated).
    pub(crate) fn delete(&mut self, id: u32, key: &[u8]) -> Result<bool, ErrorCode> {
        let slot = self.live_slot_mut(id)?;
        let cache = slot.cache.as_mut().ok_or(ErrorCode::Closed)?;
        let removed = cache.remove(key).is_some();
        if removed {
            slot.generation = slot.generation.saturating_add(1);
        }
        Ok(removed)
    }

    /// The engine's 7-field stats snapshot for a live id.
    pub(crate) fn stats(&self, id: u32) -> Result<CacheStats, ErrorCode> {
        let slot = self.live_slot(id)?;
        let cache = slot.cache.as_ref().ok_or(ErrorCode::Closed)?;
        Ok(cache.stats())
    }
}
