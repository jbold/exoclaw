use crate::state::ChatState;
use leptos::prelude::*;

#[component]
pub fn ConnectionStatus() -> impl IntoView {
    let state = use_context::<ChatState>().expect("ChatState in context");

    let dot_class = move || {
        if state.is_connected.get() {
            "status-dot connected"
        } else {
            "status-dot disconnected"
        }
    };

    let label = move || {
        if state.is_connected.get() {
            "Connected"
        } else {
            "Disconnected"
        }
    };

    let on_click = move |_| {
        if !state.is_connected.get() {
            // Trigger reconnect by spawning connection attempt
            let token = state.auth_token.get();
            wasm_bindgen_futures::spawn_local(async move {
                match crate::ws::connect(token).await {
                    Ok(_) => {
                        state.is_connected.set(true);
                    }
                    Err(e) => {
                        if e.contains("auth") {
                            state.needs_auth.set(true);
                        }
                        state.add_error(format!("Reconnect failed: {}", e));
                    }
                }
            });
        }
    };

    view! {
        <div class="connection-status" on:click=on_click title=label>
            <span class=dot_class></span>
            <span class="status-label">{label}</span>
        </div>
    }
}
