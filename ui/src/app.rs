use crate::components::auth_prompt::AuthPrompt;
use crate::components::chat::Chat;
use crate::components::input::MessageInput;
use crate::components::status::ConnectionStatus;
use crate::state::ChatState;
use leptos::prelude::*;

#[component]
pub fn App() -> impl IntoView {
    let state = ChatState::new();
    provide_context(state);

    // Attempt initial WebSocket connection
    wasm_bindgen_futures::spawn_local(async move {
        match crate::ws::connect(None).await {
            Ok(_) => {
                state.is_connected.set(true);
            }
            Err(e) => {
                if e.contains("auth") {
                    state.needs_auth.set(true);
                }
            }
        }
    });

    view! {
        <style>{STYLES}</style>
        <main class="app">
            <header class="app-header">
                <h1>"exoclaw"</h1>
                <ConnectionStatus/>
            </header>
            <Chat/>
            <MessageInput/>
            {move || {
                if state.needs_auth.get() {
                    view! { <AuthPrompt/> }.into_any()
                } else {
                    view! {}.into_any()
                }
            }}
        </main>
    }
}

const STYLES: &str = r#"
    * {
        margin: 0;
        padding: 0;
        box-sizing: border-box;
    }

    body {
        background: #1a1a2e;
        color: #e0e0e0;
        font-family: -apple-system, BlinkMacSystemFont, "Segoe UI", Roboto, sans-serif;
        height: 100vh;
        overflow: hidden;
    }

    .app {
        display: flex;
        flex-direction: column;
        height: 100vh;
        max-width: 900px;
        margin: 0 auto;
    }

    .app-header {
        display: flex;
        align-items: center;
        justify-content: space-between;
        padding: 12px 16px;
        border-bottom: 1px solid #2a2a4a;
    }

    .app-header h1 {
        font-size: 1.1rem;
        font-weight: 600;
        color: #8888cc;
    }

    .chat-container {
        flex: 1;
        overflow: hidden;
        display: flex;
        flex-direction: column;
    }

    .message-list {
        flex: 1;
        overflow-y: auto;
        padding: 16px;
        display: flex;
        flex-direction: column;
        gap: 12px;
    }

    .message-list::-webkit-scrollbar {
        width: 6px;
    }

    .message-list::-webkit-scrollbar-track {
        background: transparent;
    }

    .message-list::-webkit-scrollbar-thumb {
        background: #3a3a5a;
        border-radius: 3px;
    }

    .message {
        max-width: 80%;
        padding: 10px 14px;
        border-radius: 12px;
        line-height: 1.5;
        word-wrap: break-word;
    }

    .message-user {
        align-self: flex-end;
        background: #2a4a8a;
        color: #e0e8ff;
        border-bottom-right-radius: 4px;
    }

    .message-assistant {
        align-self: flex-start;
        background: #2a2a3e;
        color: #d0d0e0;
        border-bottom-left-radius: 4px;
    }

    .message-error {
        align-self: center;
        background: #4a1a1a;
        color: #ff8888;
        border-radius: 8px;
        font-size: 0.9em;
    }

    .message-content p {
        margin: 0.4em 0;
    }

    .message-content p:first-child {
        margin-top: 0;
    }

    .message-content p:last-child {
        margin-bottom: 0;
    }

    .message-content pre {
        background: #111122;
        padding: 10px;
        border-radius: 6px;
        overflow-x: auto;
        margin: 0.5em 0;
    }

    .message-content code {
        font-family: "JetBrains Mono", "Fira Code", monospace;
        font-size: 0.9em;
    }

    .message-content pre code {
        background: none;
        padding: 0;
    }

    .message-content code {
        background: #111122;
        padding: 2px 5px;
        border-radius: 3px;
    }

    .streaming-dot {
        display: inline-block;
        animation: pulse 1s ease-in-out infinite;
        color: #8888cc;
    }

    @keyframes pulse {
        0%, 100% { opacity: 1; }
        50% { opacity: 0.3; }
    }

    .input-area {
        display: flex;
        gap: 8px;
        padding: 12px 16px;
        border-top: 1px solid #2a2a4a;
        background: #1a1a2e;
    }

    .chat-input {
        flex: 1;
        background: #222240;
        color: #e0e0e0;
        border: 1px solid #3a3a5a;
        border-radius: 8px;
        padding: 10px 12px;
        font-family: inherit;
        font-size: 0.95rem;
        resize: none;
        min-height: 44px;
        max-height: 160px;
        outline: none;
    }

    .chat-input:focus {
        border-color: #5a5a8a;
    }

    .chat-input:disabled {
        opacity: 0.5;
    }

    .chat-input::placeholder {
        color: #666688;
    }

    .send-button {
        background: #3a5aaa;
        color: #e0e8ff;
        border: none;
        border-radius: 8px;
        padding: 10px 20px;
        font-size: 0.95rem;
        cursor: pointer;
        align-self: flex-end;
    }

    .send-button:hover:not(:disabled) {
        background: #4a6abb;
    }

    .send-button:disabled {
        opacity: 0.4;
        cursor: not-allowed;
    }

    .connection-status {
        display: flex;
        align-items: center;
        gap: 6px;
        cursor: pointer;
        font-size: 0.85rem;
    }

    .status-dot {
        width: 8px;
        height: 8px;
        border-radius: 50%;
    }

    .status-dot.connected {
        background: #44bb66;
    }

    .status-dot.disconnected {
        background: #cc4444;
    }

    .status-label {
        color: #888;
    }

    .auth-overlay {
        position: fixed;
        top: 0;
        left: 0;
        right: 0;
        bottom: 0;
        background: rgba(0, 0, 0, 0.7);
        display: flex;
        align-items: center;
        justify-content: center;
        z-index: 100;
    }

    .auth-modal {
        background: #222240;
        border: 1px solid #3a3a5a;
        border-radius: 12px;
        padding: 32px;
        max-width: 400px;
        width: 90%;
    }

    .auth-modal h2 {
        margin-bottom: 8px;
        color: #aaaadd;
    }

    .auth-modal p {
        margin-bottom: 16px;
        color: #888;
        font-size: 0.9rem;
    }

    .auth-input {
        width: 100%;
        background: #1a1a2e;
        color: #e0e0e0;
        border: 1px solid #3a3a5a;
        border-radius: 8px;
        padding: 10px 12px;
        font-size: 0.95rem;
        margin-bottom: 12px;
        outline: none;
    }

    .auth-input:focus {
        border-color: #5a5a8a;
    }

    .auth-button {
        width: 100%;
        background: #3a5aaa;
        color: #e0e8ff;
        border: none;
        border-radius: 8px;
        padding: 10px;
        font-size: 0.95rem;
        cursor: pointer;
    }

    .auth-button:hover {
        background: #4a6abb;
    }
"#;
