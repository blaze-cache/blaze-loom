//! M3 spike: minimal hand-written flat raw ABI. No generator, no dependencies.
//!
//! The other arms copy this core verbatim so only the binding layer differs.

use std::cell::RefCell;
use std::collections::HashMap;

thread_local! {
    static STORE: RefCell<HashMap<Vec<u8>, Vec<u8>>> = RefCell::new(HashMap::new());
    static SCRATCH: RefCell<(Vec<u8>, usize)> = RefCell::new((vec![0u8; 1 << 20], 0));
}

#[no_mangle]
pub extern "C" fn __blaze_abi_version() -> i32 {
    0
}

#[no_mangle]
pub extern "C" fn reset() {
    SCRATCH.with(|s| s.borrow_mut().1 = 0);
}

#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    SCRATCH.with(|s| {
        let mut s = s.borrow_mut();
        let off = s.1;
        let end = off.saturating_add(len);
        if s.0.len() < end {
            s.0.resize(end, 0);
        }
        s.1 = end;
        unsafe { s.0.as_mut_ptr().add(off) }
    })
}

#[no_mangle]
pub extern "C" fn set(kp: *const u8, kl: usize, vp: *const u8, vl: usize) -> i32 {
    let k = unsafe { std::slice::from_raw_parts(kp, kl) }.to_vec();
    let v = unsafe { std::slice::from_raw_parts(vp, vl) }.to_vec();
    STORE.with(|s| {
        s.borrow_mut().insert(k, v);
    });
    0
}

#[no_mangle]
pub extern "C" fn has(kp: *const u8, kl: usize) -> i32 {
    let k = unsafe { std::slice::from_raw_parts(kp, kl) };
    STORE.with(|s| i32::from(s.borrow().contains_key(k)))
}

#[no_mangle]
pub extern "C" fn get_sync(kp: *const u8, kl: usize, out: *mut u8, cap: usize) -> i32 {
    let k = unsafe { std::slice::from_raw_parts(kp, kl) };
    STORE.with(|s| match s.borrow().get(k) {
        Some(v) if v.len() <= cap => {
            unsafe { std::ptr::copy_nonoverlapping(v.as_ptr(), out, v.len()) };
            v.len() as i32
        }
        _ => -1,
    })
}
