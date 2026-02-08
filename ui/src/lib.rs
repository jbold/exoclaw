pub mod app;
pub mod components;
pub mod markdown;
pub mod state;
pub mod ws;

pub use app::App;

#[cfg(not(test))]
use wasm_bindgen::prelude::*;

#[cfg(not(test))]
#[wasm_bindgen(start)]
pub fn start() {
    leptos::mount::mount_to_body(App);
}
