use smails_core::{initials, mailbox_name_from_token, message_path, short_id};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn smails_initials(name: &str) -> String {
    initials(name)
}

#[wasm_bindgen]
pub fn smails_short_id(id: &str) -> String {
    short_id(id).to_owned()
}

#[wasm_bindgen]
pub fn smails_message_path(id: &str) -> String {
    message_path(id)
}

#[wasm_bindgen]
pub fn smails_token_address(token: &str) -> String {
    mailbox_name_from_token(token).unwrap_or_default()
}
