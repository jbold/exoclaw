use async_nats::Client;
use tracing::info;

/// NATS JetStream message bus for routing messages between components.
///
/// Subject pattern: exoclaw.{channel}.{account}.{peer}
/// This maps directly to the session routing hierarchy.
pub struct MessageBus {
    client: Option<Client>,
}

impl MessageBus {
    pub fn new() -> Self {
        Self { client: None }
    }

    /// Connect to a NATS server. If no server is available, runs in
    /// local-only mode (in-process routing, no persistence/replay).
    pub async fn connect(&mut self, url: &str) -> anyhow::Result<()> {
        match async_nats::connect(url).await {
            Ok(client) => {
                info!("connected to NATS at {url}");
                self.client = Some(client);
                Ok(())
            }
            Err(e) => {
                info!("NATS not available ({e}), running in local-only mode");
                Ok(())
            }
        }
    }

    /// Publish a message to a subject.
    pub async fn publish(&self, subject: &str, payload: &[u8]) -> anyhow::Result<()> {
        if let Some(client) = &self.client {
            client
                .publish(subject.to_string(), payload.to_vec().into())
                .await?;
        }
        Ok(())
    }

    /// Check if NATS is connected.
    pub fn is_connected(&self) -> bool {
        self.client.is_some()
    }
}

impl Default for MessageBus {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::MessageBus;

    #[test]
    fn new_and_default_start_disconnected() {
        let bus = MessageBus::new();
        assert!(!bus.is_connected());

        let bus_default = MessageBus::default();
        assert!(!bus_default.is_connected());
    }

    #[tokio::test]
    async fn publish_without_connection_is_noop() {
        let bus = MessageBus::new();
        bus.publish("exoclaw.web.account.peer", b"payload")
            .await
            .expect("publish should be a no-op when disconnected");
        assert!(!bus.is_connected());
    }

    #[tokio::test]
    async fn connect_to_unreachable_server_falls_back_to_local_mode() {
        let mut bus = MessageBus::new();
        bus.connect("nats://127.0.0.1:1")
            .await
            .expect("unreachable nats should not hard-fail");
        assert!(!bus.is_connected());
    }
}
