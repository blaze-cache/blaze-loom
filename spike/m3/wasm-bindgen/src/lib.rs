//! M3 spike: generated-glue arm using wasm-bindgen. Same logical operations as the raw arms.

use std::cell::RefCell;
use std::collections::HashMap;
use wasm_bindgen::prelude::*;

thread_local! {
    static STORE: RefCell<HashMap<Vec<u8>, Vec<u8>>> = RefCell::new(HashMap::new());
}

#[wasm_bindgen]
pub fn set(k: &[u8], v: &[u8]) {
    STORE.with(|s| {
        s.borrow_mut().insert(k.to_vec(), v.to_vec());
    });
}

#[wasm_bindgen]
pub fn has(k: &[u8]) -> bool {
    STORE.with(|s| s.borrow().contains_key(k))
}

#[wasm_bindgen]
pub fn get_sync(k: &[u8]) -> Option<Vec<u8>> {
    STORE.with(|s| s.borrow().get(k).cloned())
}
