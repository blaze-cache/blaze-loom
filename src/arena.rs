//! Fixed-capacity scratch arena for host-supplied `(ptr, len)` buffers.
//!
//! The flat ABI never lets the host hand wasm a raw pointer into JS memory. Instead the host
//! calls `__blaze_arena_alloc(len)`, gets back a pointer **inside wasm linear memory**, writes
//! its bytes there, and passes the pointer to an export. Every pointer is bounds-checked
//! against this arena before any read or write; a bad pointer degrades to `BAD_ARG`, never a
//! trap.
//!
//! # Epoch contract
//!
//! The arena is a bump allocator with a lazy per-operation reset:
//!
//! 1. The host allocates **all** buffers for one operation (`alloc` … `alloc`).
//! 2. It calls **exactly one** arena-touching export (`open`/`get_sync`/`set`/`has`/
//!    `delete`/`stats`), which arms the reset via [`end_epoch`] on the way in.
//! 3. It reads the results, then repeats from step 1.
//!
//! The next `alloc` after step 2 consumes the armed reset: the bump returns to the floor and
//! the recycled region is **zeroed**, so keys and values allocated in a fresh epoch are
//! predictable zeros (this is what makes native tests observable without a read-back export).
//!
//! The flip side of recycling: **an offset from before the reset must not be used
//! afterwards**. It may still be in bounds — and will read as zeros if a new allocation has
//! grown over it — but it may equally have been handed out again to a *different* buffer of
//! the new epoch, so reading it is meaningless (worst case: the new buffer's bytes, which
//! is how a stale key silently turns into a different key). The host contract is therefore:
//! allocate an operation's inputs after the previous operation's export, use them for that
//! one export, and drop them. `__blaze_arena_alloc` never recycles *within* an epoch, so
//! buffers allocated together stay disjoint.
//!
//! # Pointer width
//!
//! On `wasm32` a pointer is an absolute linear-memory address — exactly what
//! `new Uint8Array(memory.buffer, ptr, len)` needs. On native test targets the same `u32`
//! carries an **arena-relative offset**, because a 64-bit host address cannot round-trip
//! through `u32`. The divergence is confined to [`Arena::ptr_of`] / [`Arena::off_of`] and is
//! invisible to callers: `alloc` returns whatever the target needs, and every other function
//! takes that value back.

use std::cell::RefCell;

/// Total arena size. An allocation that would exceed it fails (`OOM` → `alloc` returns `0`)
/// instead of growing the buffer — growth would move it and invalidate live pointers.
pub(crate) const CAPACITY: usize = 1024 * 1024;

/// Alignment of every handed-out region (and of the bump).
pub(crate) const ALIGN: usize = 8;

/// The bump allocator. `buf.len()` **is** the bump: allocated regions are exactly
/// `ALIGN..buf.len()`, with `0..ALIGN` reserved as the null zone (see [`Arena::off_of`]).
pub(crate) struct Arena {
    buf: Vec<u8>,
    /// Armed by [`end_epoch`]; consumed (and honored) by the next [`Arena::alloc`].
    needs_reset: bool,
}

thread_local! {
    /// One arena per thread — which on `wasm32` (the only target that ships) means exactly
    /// one arena per instance, because the target has no threads. `thread_local` rather than
    /// a `static Mutex` is what upstream's own wasm binding uses for its module globals, and
    /// `const`-initialization keeps lazy-init machinery (and any import) out of the module.
    /// A `RefCell` also has no poisoning to recover from: a panicking export drops its guard
    /// on unwind and the next call borrows cleanly.
    static ARENA: RefCell<Arena> = const { RefCell::new(Arena::new()) };
}

impl Arena {
    const fn new() -> Self {
        Self {
            buf: Vec::new(),
            needs_reset: false,
        }
    }

    /// Arm the lazy reset. Called by every arena-touching export on the way **in** — before
    /// any of this epoch's results are written — so the flag outlives the results and the
    /// host can still read them. The next [`Arena::alloc`] does the actual recycling.
    fn end_epoch(&mut self) {
        self.needs_reset = true;
    }

    /// Consume a pending reset and guarantee capacity **before** any pointer is handed out.
    ///
    /// The `reserve_exact` runs at most once per process: afterwards `capacity >= CAPACITY`
    /// and every `resize` stays within it, so the buffer never reallocates and every pointer
    /// ever returned stays valid for the lifetime of the instance.
    fn prepare(&mut self) {
        if self.needs_reset {
            self.buf.clear();
            self.needs_reset = false;
        }
        if self.buf.capacity() < CAPACITY {
            self.buf.reserve_exact(CAPACITY);
        }
        // The null zone: offsets 0..ALIGN are never handed out, so a host-provided `0` is
        // unambiguously "no buffer" on every target.
        if self.buf.len() < ALIGN {
            self.buf.resize(ALIGN, 0);
        }
    }

    /// Carve out an `len`-byte region, aligned to [`ALIGN`]. Returns `0` on failure (request
    /// larger than what remains of [`CAPACITY`] — never a realloc, see [`CAPACITY`]).
    ///
    /// `len == 0` still returns a **valid** pointer to a zero-length region: the plan pins
    /// `0` as the failure signal, so it must stay unambiguous even for an empty request.
    /// The next allocation may land at the same offset (a zero-length region owns no
    /// bytes), which is harmless — `(ptr, 0)` reads nothing.
    pub(crate) fn alloc(&mut self, len: u32) -> u32 {
        self.prepare();
        let len = len as usize;
        let Some(start) = self
            .buf
            .len()
            .checked_add(ALIGN - 1)
            .map(|v| v & !(ALIGN - 1))
        else {
            return 0;
        };
        let Some(end) = start.checked_add(len) else {
            return 0;
        };
        if end > CAPACITY {
            return 0;
        }
        // `resize` fills only the extension — the alignment gap and any new bytes become
        // zeros (fresh-epoch predictability) while existing bytes stay put. Capacity was
        // reserved up front, so this never reallocates: every live pointer stays valid.
        self.buf.resize(end, 0);
        self.ptr_of(start)
    }

    /// Absolute pointer (wasm) or arena offset (native) for an in-arena offset.
    fn ptr_of(&self, off: usize) -> u32 {
        if cfg!(target_family = "wasm") {
            // Raw-pointer → integer casts are safe Rust; only dereferencing is `unsafe`.
            (self.buf.as_ptr() as usize + off) as u32
        } else {
            off as u32
        }
    }

    /// Inverse of [`Arena::ptr_of`]: validate `(ptr, len)` and return the arena offset.
    ///
    /// `len == 0` short-circuits — reading or writing zero bytes is safe anywhere, so an
    /// empty key/value needs no pointer at all (`(0, 0)` is the canonical empty region).
    /// Otherwise the null zone `0..ALIGN` is rejected, which is what makes `ptr = 0` with a
    /// non-zero length a `BAD_ARG` on **both** targets.
    fn off_of(&self, ptr: u32, len: usize) -> Result<usize, ()> {
        if len == 0 {
            return Ok(0);
        }
        let off = if cfg!(target_family = "wasm") {
            (ptr as usize)
                .checked_sub(self.buf.as_ptr() as usize)
                .ok_or(())?
        } else {
            ptr as usize
        };
        if off < ALIGN {
            return Err(());
        }
        let end = off.checked_add(len).ok_or(())?;
        if end > self.buf.len() {
            return Err(());
        }
        Ok(off)
    }

    /// Copy `len` bytes out of the arena. The input is always copied into wasm-owned storage
    /// (the plan's safety model) — never borrowed past this call.
    pub(crate) fn read(&self, ptr: u32, len: u32) -> Result<Vec<u8>, ()> {
        if len == 0 {
            return Ok(Vec::new());
        }
        let off = self.off_of(ptr, len as usize)?;
        let end = off.checked_add(len as usize).ok_or(())?;
        self.buf.get(off..end).map(<[u8]>::to_vec).ok_or(())
    }

    /// Parse `[u32 byte_len][UTF-8 bytes]…` records out of the arena. A record that runs past
    /// the end, or bytes that are not valid UTF-8, is `Err` → `BAD_ARG`.
    pub(crate) fn read_tags(&self, ptr: u32, len: u32) -> Result<Vec<String>, ()> {
        if len == 0 {
            return Ok(Vec::new());
        }
        let off = self.off_of(ptr, len as usize)?;
        let end = off.checked_add(len as usize).ok_or(())?;
        let raw = self.buf.get(off..end).ok_or(())?;
        let mut tags = Vec::new();
        let mut pos = 0usize;
        while pos < raw.len() {
            let Some(tag_len_end) = pos.checked_add(4) else {
                return Err(());
            };
            let Some(tag_len_bytes) = raw.get(pos..tag_len_end) else {
                return Err(());
            };
            let tag_len = u32::from_le_bytes([
                tag_len_bytes[0],
                tag_len_bytes[1],
                tag_len_bytes[2],
                tag_len_bytes[3],
            ]) as usize;
            pos = tag_len_end;
            // Checked: on wasm32 `pos + tag_len` can overflow `usize` (u32-as-usize length).
            let Some(tag_end) = pos.checked_add(tag_len) else {
                return Err(());
            };
            let Some(tag_bytes) = raw.get(pos..tag_end) else {
                return Err(());
            };
            pos = tag_end;
            tags.push(std::str::from_utf8(tag_bytes).map_err(|_| ())?.to_string());
        }
        Ok(tags)
    }

    /// Validate that `[ptr, ptr+cap)` is a usable out-buffer of `cap` bytes (without writing).
    pub(crate) fn check_out(&self, ptr: u32, cap: u32) -> Result<(), ()> {
        self.off_of(ptr, cap as usize).map(|_| ())
    }

    /// Write `bytes` into the validated out-buffer `[ptr, ptr+cap)`.
    ///
    /// `cap < bytes.len()` is `Err` → `BAD_ARG` (the host promised too little room).
    pub(crate) fn write_out(&mut self, ptr: u32, cap: u32, bytes: &[u8]) -> Result<(), ()> {
        if (cap as usize) < bytes.len() {
            return Err(());
        }
        let off = self.off_of(ptr, cap as usize)?;
        let end = off.checked_add(bytes.len()).ok_or(())?;
        let dst = self.buf.get_mut(off..end).ok_or(())?;
        dst.copy_from_slice(bytes);
        Ok(())
    }

    /// Write one little-endian `u32` (the `get_sync` length word).
    pub(crate) fn write_u32(&mut self, ptr: u32, value: u32) -> Result<(), ()> {
        self.write_out(ptr, 4, &value.to_le_bytes())
    }

    /// Test-only: write `bytes` at `ptr` directly.
    ///
    /// A JS host writes through `memory.buffer`, which native tests cannot reach (there is
    /// deliberately **no** write export — that would be an unbounded host→wasm copy path
    /// outside the plan's table). Unit tests that need non-zero input bytes — an eviction
    /// policy other than `0`, a tag length that overruns, non-UTF-8 tag bytes — use this
    /// instead, while integration tests in `tests/abi.rs` stay zeros-only.
    #[cfg(test)]
    pub(crate) fn write_for_test(&mut self, ptr: u32, bytes: &[u8]) {
        let off = self.off_of(ptr, bytes.len()).expect("test write in bounds");
        self.buf[off..off + bytes.len()].copy_from_slice(bytes);
    }
}

/// Arm the epoch reset; see [`Arena::end_epoch`]. Called by every arena-touching export.
///
/// Every helper below borrows the thread's `RefCell` only for its own body and returns an
/// owned value, so guards never escape and no two helpers can overlap a borrow — which is
/// what makes the `REGISTRY` → `ARENA` nesting in `abi.rs` panic-free by construction.
pub(crate) fn end_epoch() {
    ARENA.with(|arena| arena.borrow_mut().end_epoch());
}

/// [`Arena::alloc`] on this thread's arena. Returns `0` on failure.
pub(crate) fn alloc(len: u32) -> u32 {
    ARENA.with(|arena| arena.borrow_mut().alloc(len))
}

/// [`Arena::read`] on this thread's arena.
pub(crate) fn read(ptr: u32, len: u32) -> Result<Vec<u8>, ()> {
    ARENA.with(|arena| arena.borrow().read(ptr, len))
}

/// [`Arena::read_tags`] on this thread's arena.
pub(crate) fn read_tags(ptr: u32, len: u32) -> Result<Vec<String>, ()> {
    ARENA.with(|arena| arena.borrow().read_tags(ptr, len))
}

/// [`Arena::check_out`] on this thread's arena.
pub(crate) fn check_out(ptr: u32, cap: u32) -> Result<(), ()> {
    ARENA.with(|arena| arena.borrow().check_out(ptr, cap))
}

/// [`Arena::write_out`] on this thread's arena.
pub(crate) fn write_out(ptr: u32, cap: u32, bytes: &[u8]) -> Result<(), ()> {
    ARENA.with(|arena| arena.borrow_mut().write_out(ptr, cap, bytes))
}

/// [`Arena::write_u32`] on this thread's arena.
pub(crate) fn write_u32(ptr: u32, value: u32) -> Result<(), ()> {
    ARENA.with(|arena| arena.borrow_mut().write_u32(ptr, value))
}

/// Test-only: [`Arena::write_for_test`] on this thread's arena.
#[cfg(test)]
pub(crate) fn write_for_test(ptr: u32, bytes: &[u8]) {
    ARENA.with(|arena| arena.borrow_mut().write_for_test(ptr, bytes));
}
