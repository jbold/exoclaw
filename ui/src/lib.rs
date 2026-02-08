pub mod app;
pub mod components;
pub mod markdown;
pub mod state;
pub mod ws;

pub use app::App;

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
pub fn main() {
    leptos::mount::mount_to_body(App);
}
