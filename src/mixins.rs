use async_trait::async_trait;

use crate::client::MatrixClient;

#[async_trait]
pub trait AutojoinRoomsMixin: Send + Sync {
    async fn on_invite(&self, client: &MatrixClient, room_id: &str) -> anyhow::Result<()>;
}

#[async_trait]
pub trait AutojoinUpgradedRoomsMixin: Send + Sync {
    async fn on_room_upgraded(
        &self,
        client: &MatrixClient,
        old_room_id: &str,
        new_room_id: &str,
    ) -> anyhow::Result<()>;
}

#[derive(Debug, Default)]
pub struct DefaultAutojoinRoomsMixin;

#[derive(Debug, Default)]
pub struct DefaultAutojoinUpgradedRoomsMixin;

#[async_trait]
impl AutojoinRoomsMixin for DefaultAutojoinRoomsMixin {
    async fn on_invite(&self, client: &MatrixClient, room_id: &str) -> anyhow::Result<()> {
        let _ = client.join_room(room_id).await?;
        Ok(())
    }
}

#[async_trait]
impl AutojoinUpgradedRoomsMixin for DefaultAutojoinUpgradedRoomsMixin {
    async fn on_room_upgraded(
        &self,
        client: &MatrixClient,
        old_room_id: &str,
        new_room_id: &str,
    ) -> anyhow::Result<()> {
        client.leave_room(old_room_id, None).await?;
        let _ = client.join_room(new_room_id).await?;
        Ok(())
    }
}
