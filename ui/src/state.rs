use leptos::prelude::*;

#[derive(Clone, Debug, PartialEq)]
pub enum MessageRole {
    User,
    Assistant,
    Error,
}

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub role: MessageRole,
    pub content: String,
    pub is_complete: bool,
}

#[derive(Clone, Copy)]
pub struct ChatState {
    pub messages: RwSignal<Vec<ChatMessage>>,
    pub input_text: RwSignal<String>,
    pub is_streaming: RwSignal<bool>,
    pub is_connected: RwSignal<bool>,
    pub needs_auth: RwSignal<bool>,
    pub auth_token: RwSignal<Option<String>>,
}

impl ChatState {
    pub fn new() -> Self {
        Self {
            messages: RwSignal::new(Vec::new()),
            input_text: RwSignal::new(String::new()),
            is_streaming: RwSignal::new(false),
            is_connected: RwSignal::new(false),
            needs_auth: RwSignal::new(false),
            auth_token: RwSignal::new(None),
        }
    }

    pub fn add_user_message(&self, content: String) {
        self.messages.update(|msgs| {
            msgs.push(ChatMessage {
                role: MessageRole::User,
                content,
                is_complete: true,
            });
        });
    }

    pub fn start_assistant_message(&self) {
        self.messages.update(|msgs| {
            msgs.push(ChatMessage {
                role: MessageRole::Assistant,
                content: String::new(),
                is_complete: false,
            });
        });
    }

    pub fn append_token(&self, text: &str) {
        self.messages.update(|msgs| {
            if let Some(last) = msgs.last_mut() {
                if last.role == MessageRole::Assistant && !last.is_complete {
                    last.content.push_str(text);
                }
            }
        });
    }

    pub fn complete_message(&self) {
        self.messages.update(|msgs| {
            if let Some(last) = msgs.last_mut() {
                if last.role == MessageRole::Assistant {
                    last.is_complete = true;
                }
            }
        });
    }

    pub fn add_error(&self, msg: String) {
        self.messages.update(|msgs| {
            msgs.push(ChatMessage {
                role: MessageRole::Error,
                content: msg,
                is_complete: true,
            });
        });
    }
}
