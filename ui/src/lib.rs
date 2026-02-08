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
    console_error_panic_hook::set_once();
    let _ = console_log::init_with_level(log::Level::Info);
    log::info!("exoclaw-ui boot");
    leptos::mount::mount_to_body(App);
}
