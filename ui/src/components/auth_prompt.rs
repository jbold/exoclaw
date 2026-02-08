use crate::state::ChatState;
use leptos::prelude::*;

#[component]
pub fn AuthPrompt() -> impl IntoView {
    let state = use_context::<ChatState>().expect("ChatState in context");
    let token_input = RwSignal::new(String::new());

    let on_submit = move |_| {
        let tok = token_input.get();
        let tok = tok.trim().to_string();
        if !tok.is_empty() {
            state.auth_token.set(Some(tok));
            state.needs_auth.set(false);
        }
    };

    let on_keydown = move |ev: web_sys::KeyboardEvent| {
        if ev.key() == "Enter" {
            ev.prevent_default();
            let tok = token_input.get();
            let tok = tok.trim().to_string();
            if !tok.is_empty() {
                state.auth_token.set(Some(tok));
                state.needs_auth.set(false);
            }
        }
    };

    view! {
        <div class="auth-overlay" data-testid="auth-overlay">
            <div class="auth-modal">
                <h2>"Authentication Required"</h2>
                <p>"Enter your gateway token to connect."</p>
                <input
                    type="password"
                    class="auth-input"
                    data-testid="auth-token-input"
                    placeholder="Token..."
                    prop:value=move || token_input.get()
                    on:input:target=move |ev| {
                        token_input.set(ev.target().value());
                    }
                    on:keydown=on_keydown
                />
                <button class="auth-button" data-testid="auth-connect-button" on:click=on_submit>
                    "Connect"
                </button>
            </div>
        </div>
    }
}
