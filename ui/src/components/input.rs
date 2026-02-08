use crate::state::ChatState;
use crate::ws;
use futures::{FutureExt, StreamExt};
use gloo_net::websocket::Message;
use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use log::{debug, warn};

const STREAM_IDLE_TIMEOUT_MS: u32 = 60_000;

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
                    state.is_connected.set(true);
                    let request_id = format!("web-{}", js_sys::Date::now() as u64);
                    if let Err(e) = ws::send_chat(&mut conn.write, &content, &request_id).await {
                        state.add_error(e);
                        state.is_streaming.set(false);
                        state.is_connected.set(false);
                        return;
                    }

                    loop {
                        let next_msg = conn.read.next().fuse();
                        let timeout = TimeoutFuture::new(STREAM_IDLE_TIMEOUT_MS).fuse();
                        futures::pin_mut!(next_msg, timeout);

                        let msg = futures::select! {
                            msg = next_msg => msg,
                            _ = timeout => {
                                warn!("stream timed out for request_id={request_id}");
                                state.complete_message();
                                state.add_error("Timed out waiting for model response.".to_string());
                                state.is_streaming.set(false);
                                state.is_connected.set(false);
                                return;
                            }
                        };

                        match msg {
                            Some(Ok(Message::Text(text))) => {
                                debug!(
                                    "received text websocket frame (request_id={request_id}, bytes={})",
                                    text.len()
                                );
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
                            Some(Ok(Message::Bytes(_))) => {}
                            Some(Err(e)) => {
                                state.complete_message();
                                state.add_error(format!("WebSocket error: {}", e));
                                state.is_streaming.set(false);
                                state.is_connected.set(false);
                                break;
                            }
                            None => {
                                warn!("websocket closed before stream completion");
                                if state.is_streaming.get() {
                                    state.complete_message();
                                    state.add_error(
                                        "Connection closed before response completed.".to_string(),
                                    );
                                    state.is_streaming.set(false);
                                }
                                state.is_connected.set(false);
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
                    state.is_connected.set(false);
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
                data-testid="chat-input"
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
                data-testid="send-button"
                prop:disabled=is_disabled
                on:click=on_send_click
            >
                "Send"
            </button>
        </div>
    }
}
