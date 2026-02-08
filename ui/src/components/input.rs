use crate::state::ChatState;
use crate::ws;
use leptos::prelude::*;

#[component]
pub fn MessageInput() -> impl IntoView {
    let state = use_context::<ChatState>().expect("ChatState in context");

    let on_send = move || {
        let content = state.input_text.get();
        let content = content.trim().to_string();
        if content.is_empty() || state.is_streaming.get() {
            return;
        }

        state.add_user_message(content.clone());
        state.input_text.set(String::new());
        state.is_streaming.set(true);
        state.start_assistant_message();

        wasm_bindgen_futures::spawn_local(async move {
            let token = state.auth_token.get();
            let conn = ws::connect(token).await;
            match conn {
                Ok(mut conn) => {
                    if let Err(e) = ws::send_chat(&mut conn.write, &content, 1).await {
                        state.add_error(e);
                        state.is_streaming.set(false);
                        return;
                    }

                    use futures::StreamExt;
                    use gloo_net::websocket::Message;

                    while let Some(msg) = conn.read.next().await {
                        match msg {
                            Ok(Message::Text(text)) => {
                                if let Some(event) = ws::parse_event(&text) {
                                    match event {
                                        ws::StreamEvent::Text(t) => {
                                            state.append_token(&t);
                                        }
                                        ws::StreamEvent::ToolUse { name, input } => {
                                            state.append_token(&format!(
                                                "\n[tool: {} input: {}]\n",
                                                name, input
                                            ));
                                        }
                                        ws::StreamEvent::Done => {
                                            state.complete_message();
                                            state.is_streaming.set(false);
                                            break;
                                        }
                                        ws::StreamEvent::Error(e) => {
                                            state.complete_message();
                                            state.add_error(e);
                                            state.is_streaming.set(false);
                                            break;
                                        }
                                    }
                                }
                            }
                            Ok(Message::Bytes(_)) => {}
                            Err(e) => {
                                state.complete_message();
                                state.add_error(format!("WebSocket error: {}", e));
                                state.is_streaming.set(false);
                                break;
                            }
                        }
                    }
                }
                Err(e) => {
                    if e.contains("auth") {
                        state.needs_auth.set(true);
                    }
                    state.add_error(e);
                    state.is_streaming.set(false);
                }
            }
        });
    };

    let on_send_click = {
        let on_send = on_send.clone();
        move |_| on_send()
    };

    let on_keydown = move |ev: web_sys::KeyboardEvent| {
        if ev.key() == "Enter" && !ev.shift_key() {
            ev.prevent_default();
            on_send();
        }
    };

    let is_disabled = move || state.is_streaming.get() || !state.is_connected.get();
    let placeholder = move || {
        if state.is_streaming.get() {
            "Agent is responding..."
        } else if !state.is_connected.get() {
            "Disconnected..."
        } else {
            "Type a message... (Enter to send, Shift+Enter for newline)"
        }
    };

    view! {
        <div class="input-area">
            <textarea
                class="chat-input"
                prop:value=move || state.input_text.get()
                prop:disabled=is_disabled
                placeholder=placeholder
                on:input:target=move |ev| {
                    state.input_text.set(ev.target().value());
                }
                on:keydown=on_keydown
            ></textarea>
            <button
                class="send-button"
                prop:disabled=is_disabled
                on:click=on_send_click
            >
                "Send"
            </button>
        </div>
    }
}
