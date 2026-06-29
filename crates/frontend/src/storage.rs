use std::cell::RefCell;
use web_sys::window;

const TOKEN_KEY: &str = "smails_token";
const ADDRESS_KEY: &str = "smails_address";

thread_local! {
    static MEM_TOKEN: RefCell<Option<String>> = const { RefCell::new(None) };
    static MEM_ADDRESS: RefCell<Option<String>> = const { RefCell::new(None) };
}

pub fn token() -> Option<String> {
    MEM_TOKEN
        .with(|value| value.borrow().clone())
        .or_else(|| stored(TOKEN_KEY))
}

pub fn address() -> Option<String> {
    MEM_ADDRESS
        .with(|value| value.borrow().clone())
        .or_else(|| stored(ADDRESS_KEY))
}

pub fn save(address: &str, token: &str) {
    MEM_ADDRESS.with(|value| *value.borrow_mut() = Some(address.to_owned()));
    MEM_TOKEN.with(|value| *value.borrow_mut() = Some(token.to_owned()));

    if let Some(storage) = local_storage() {
        let _ = storage.set_item(ADDRESS_KEY, address);
        let _ = storage.set_item(TOKEN_KEY, token);
    }
}

pub fn clear() {
    MEM_ADDRESS.with(|value| *value.borrow_mut() = None);
    MEM_TOKEN.with(|value| *value.borrow_mut() = None);

    if let Some(storage) = local_storage() {
        let _ = storage.remove_item(ADDRESS_KEY);
        let _ = storage.remove_item(TOKEN_KEY);
    }
}

fn stored(key: &str) -> Option<String> {
    local_storage()?.get_item(key).ok().flatten()
}

fn local_storage() -> Option<web_sys::Storage> {
    window()?.local_storage().ok().flatten()
}
