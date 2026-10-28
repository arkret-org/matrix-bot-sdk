use std::time::Duration;

use async_trait::async_trait;

use crate::client::MatrixClient;

#[async_trait]
pub trait JoinRoomStrategy: Send + Sync {
    async fn join_room(&self, client: &MatrixClient, room_id: &str) -> anyhow::Result<String>;
}

#[derive(Debug, Default)]
pub struct AppserviceJoinRoomStrategy;

impl AppserviceJoinRoomStrategy {
    pub fn new() -> Self {
        Self
    }
}

#[async_trait]
impl JoinRoomStrategy for AppserviceJoinRoomStrategy {
    async fn join_room(&self, client: &MatrixClient, room_id: &str) -> anyhow::Result<String> {
        client.join_room(room_id).await
    }
}

#[derive(Debug, Clone)]
pub struct SimpleRetryJoinStrategy {
    pub schedule: Vec<Duration>,
}

impl Default for SimpleRetryJoinStrategy {
    fn default() -> Self {
        Self {
            schedule: vec![
                Duration::from_secs(0),
                Duration::from_secs(2),
                Duration::from_secs(5),
                Duration::from_secs(10),
            ],
        }
    }
}

impl SimpleRetryJoinStrategy {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_schedule(schedule: Vec<Duration>) -> Self {
        Self { schedule }
    }

    pub async fn retry<F, Fut>(&self, mut api_call: F) -> anyhow::Result<String>
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = anyhow::Result<String>>,
    {
        let mut last_error = None;

        for delay in &self.schedule {
            if !delay.is_zero() {
                tokio::time::sleep(*delay).await;
            }

            match api_call().await {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| anyhow::anyhow!("All retry attempts failed")))
    }
}

#[async_trait]
impl JoinRoomStrategy for SimpleRetryJoinStrategy {
    async fn join_room(&self, client: &MatrixClient, room_id: &str) -> anyhow::Result<String> {
        self.retry(|| client.join_room(room_id)).await
    }
}
