use crate::components::message::MessageBubble;
use crate::state::ChatState;
use leptos::prelude::*;

#[component]
pub fn Chat() -> impl IntoView {
    let state = use_context::<ChatState>().expect("ChatState in context");
    let container_ref: NodeRef<leptos::html::Div> = NodeRef::new();
    let user_scrolled_up = RwSignal::new(false);

    // Auto-scroll when messages change
    Effect::new(move |_| {
        // Track messages signal to trigger on changes
        let _msgs = state.messages.get();
        if user_scrolled_up.get() {
            return;
        }
        if let Some(el) = container_ref.get() {
            let el: &web_sys::Element = &el;
            el.set_scroll_top(el.scroll_height());
        }
    });

    // Detect if user scrolled up
    let on_scroll = move |_| {
        if let Some(el) = container_ref.get() {
            let el: &web_sys::Element = &el;
            let at_bottom = el.scroll_height() - el.scroll_top() - el.client_height() < 40;
            user_scrolled_up.set(!at_bottom);
        }
    };

    view! {
        <div class="chat-container">
            <div class="message-list" node_ref=container_ref on:scroll=on_scroll>
                {move || {
                    state.messages.get().into_iter().map(|msg| {
                        view! { <MessageBubble msg=msg/> }
                    }).collect::<Vec<_>>()
                }}
            </div>
        </div>
    }
}
