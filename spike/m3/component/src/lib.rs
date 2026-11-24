//! M3 spike: Component Model / WIT arm. Same logical operations as the other three arms.

use std::cell::RefCell;
use std::collections::HashMap;

wit_bindgen::generate!({
    path: "wit",
    world: "spike",
});

thread_local! {
    static STORE: RefCell<HashMap<Vec<u8>, Vec<u8>>> = RefCell::new(HashMap::new());
}

struct Spike;

impl exports::blaze::spike::store::Guest for Spike {
    fn set(k: Vec<u8>, v: Vec<u8>) {
        STORE.with(|s| {
            s.borrow_mut().insert(k, v);
        });
    }

    fn has(k: Vec<u8>) -> bool {
        STORE.with(|s| s.borrow().contains_key(&k))
    }

    fn get_sync(k: Vec<u8>) -> Option<Vec<u8>> {
        STORE.with(|s| s.borrow().get(&k).cloned())
    }
}

export!(Spike);
