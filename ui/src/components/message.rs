use crate::markdown;
use crate::state::{ChatMessage, MessageRole};
use leptos::prelude::*;

#[component]
pub fn MessageBubble(msg: ChatMessage) -> impl IntoView {
    let class = match msg.role {
        MessageRole::User => "message message-user",
        MessageRole::Assistant => "message message-assistant",
        MessageRole::Error => "message message-error",
    };

    let content_view = match msg.role {
        MessageRole::Assistant => {
            let html = markdown::render(&msg.content);
            view! {
                <div class="message-content" inner_html=html></div>
            }
            .into_any()
        }
        _ => {
            let text = msg.content.clone();
            view! {
                <div class="message-content">{text}</div>
            }
            .into_any()
        }
    };

    let streaming_indicator = if !msg.is_complete {
        view! { <span class="streaming-dot">"..."</span> }.into_any()
    } else {
        view! {}.into_any()
    };

    view! {
        <div class=class>
            {content_view}
            {streaming_indicator}
        </div>
    }
}
