mod api;
mod app;
mod components;
mod pages;
mod storage;
mod ws;

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    yew::Renderer::<app::App>::new().render();
    Ok(())
}
